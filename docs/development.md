# Development

How Faber is built, tested and kept clean. These rules apply to every change, by people and AI
agents alike.

## Ground rules

1. **Test first (TDD).** Write the failing test, make it pass, then clean up. A change without a
   test is unfinished. The exception is pure styling, which is checked by eye and by the e2e smoke test.
2. **Fix causes, not symptoms.** When something breaks, find *why*. Ask whether it's a design flaw
   that will break again elsewhere, and fix it at that level. No hotfixes, no "just retry", no
   special cases that hide a bug. A regression test that reproduces the original failure comes first.
3. **The architecture is enforced by tools, not by memory.** The boundaries below are checked in CI
   (`cargo run -p faber-arch-check`, and dependency rules in the shell). Breaking a boundary means
   changing this document first, as a deliberate decision.
4. **Everything must pass before a commit:** `cargo fmt --check`, `cargo clippy -- -D warnings`,
   `cargo test`, `npm run check` and `npm test` in `shell/`, and the WIT validation.
5. **Glossary terms only** ([glossary.md](glossary.md)) in code, UI text and errors.

## Repository layout

```
core/                     faber-core: the single Rust process (lib + binary)
sdk/rust/capabilities/    faber-capabilities: capability data types (mirror of wit/)
sdk/rust/connector/       faber-connector: the connector trait and manifest types
connectors/ldrobot-ld19/  first connector (built-in Rust in Phase 1, WASM in Phase 3)
shell/                    the web desktop (Svelte 5 + Vite + TypeScript)
tools/arch-check/         enforces the crate dependency rules below
wit/                      capability interfaces (the future plugin API)
fixtures/                 test data, e.g. a real LD19 capture
```

## Architecture rules (enforced)

**Crate dependencies** point only downwards:

```
faber-core ──► faber-connector ──► faber-capabilities
    └────────► ldrobot-ld19 ──► faber-connector
```

| Crate | May depend on | Must not |
|---|---|---|
| `faber-capabilities` | `serde`, plus `ts-rs` behind the `typescript` feature (code generation only) | anything with I/O |
| `faber-connector` | `faber-capabilities`, `serde`, `toml` | I/O, async runtimes, `faber-core` |
| connectors (`connectors/*`) | `faber-connector`, `faber-capabilities`, `toml` | **any I/O crate** (serial, net, fs, tokio). Connectors get bytes in and give readings out. The core owns all I/O. |
| `faber-core` | everything above, plus I/O crates | — |

**Inside `faber-core`**, the modules form layers. A module may only use the layers below it:

| Layer | Modules | Job |
|---|---|---|
| 5 | `api` | HTTP + WebSocket. Translates between the wire and the machine. No logic of its own. |
| 4 | `machine` | Ties everything together. The only place that changes what runs: it writes `parts.toml` first, then reconciles. |
| 3 | `runtime`, `suggest` | Supervising parts, and detect-then-suggest. Built on layer 2. |
| 2 | `backup`, `config`, `connectors`, `disks`, `estop`, `files`, `host`, `leases`, `logs`, `ports`, `router`, `settings` | One job each. They don't know about HTTP or each other. |
| 1 | `clock`, `names` | Small pure building blocks. |

This is enforced by `core/tests/layers.rs`, which fails with the file and line of any import that
points up or sideways. Every module that does I/O does it behind a trait, so its tests use fakes
(fake clock, fake serial discovery, in-memory ports).

**In the shell:**

| Folder | Job | May import |
|---|---|---|
| `src/lib/api/` | The only code that talks to the core (REST + WebSocket, MessagePack). Generated types live in `generated/`. | nothing from the UI |
| `src/lib/state/` | App state as Svelte 5 runes (parts, ports, windows) | `api` |
| `src/lib/desktop/` | Plain TypeScript logic for the desktop and its accessories (windows, icons, menus, file paths, the Notes document), no Svelte | nothing |
| `src/lib/viewers/` | Canvas drawing for capability viewers, and the display units (`units.ts`), as pure functions | nothing |
| `src/components/`, `src/windows/` | Svelte UI | `state`, `desktop`, `viewers`, never `api` directly |

## Testing

| What | Tool | Where |
|---|---|---|
| Rust units | `cargo test` | next to the code (`#[cfg(test)]`) |
| Rust integration | `cargo test` | `core/tests/`. A real LD19 capture goes through the whole core: replay port → connector → router → WebSocket |
| Shell units | Vitest + Testing Library | `shell/src/**/*.test.ts` |
| End-to-end | Playwright | `shell/e2e/`. Builds the shell, starts the real core with it compiled in (`embed-shell`), adds a part on the demo recording and checks that a scan is drawn, at desktop and 7" sizes |
| Interfaces | `wasm-tools component wit wit/capabilities` | CI |

## End-to-end tests

```bash
npm --prefix shell run test:e2e
```

Playwright builds and starts everything itself. It needs a browser: run `npx playwright install chromium`
once in `shell/`, or set `FABER_E2E_CHANNEL=chrome` to use an installed Chrome.

## Running it (Phase 1)

```bash
cargo run -p faber-core -- --data ./data
```

Then in a second terminal:

```bash
npm --prefix shell run dev
```

Open the address Vite prints. During development Vite forwards `/api` to the core. Add `?wire=json`
to the page address to switch the WebSocket to JSON and read every message in the browser's dev tools. A release
build (`cargo build --release -p faber-core --features embed-shell`) compiles the shell into
the core binary, so the machine only runs one file and no Node.js. Debug builds with `embed-shell`
compile it in too (`debug-embed`), so a running core always serves the shell it was built with.
After changing the shell, rebuild and restart the core, or use the Vite dev server.

The shell and the core each carry a fingerprint of the generated API types. If a shell meets a
core built from a different API (an old core still running, say), it shows "Restart the Faber
core" instead of failing with 404s. Both sides compute it the same way: `core/build/api_fingerprint.rs`
and `shell/apiFingerprint.ts`, tested against one fixture (`fixtures/api-fingerprint`).
