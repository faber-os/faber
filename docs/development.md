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
   `cargo test`, `npm run check`, `npm test` and `npm run i18n:check` in `shell/`, and the WIT
   validation.
5. **Glossary terms only** ([glossary.md](glossary.md)) in code, UI text and errors.

## Repository layout

```
core/                     faber-core: the single Rust process (lib + binary)
sdk/rust/capabilities/    faber-capabilities: capability data types (mirror of wit/)
sdk/rust/connector/       faber-connector: the connector trait and manifest types (and faber.toml)
sdk/rust/connector-wasm/  faber-connector-wasm: builds a connector as a WebAssembly component
sdk/rust/app-wasm/        faber-app-wasm: builds an app's logic as a WebAssembly component (docs/apps.md)
sdk/rust/faberlink/       faberlink: the FaberLink bridge protocol, core side (docs/faberlink.md)
sdk/rust/flash/           faber-flash: the Arduino flasher's protocols (Intel HEX, STK500v1 and v2)
connectors/ldrobot-ld19/  first connector: built in, and the same code as a WebAssembly component
shell/                    the web desktop (Svelte 5 + Vite + TypeScript)
sdk/js/shell/             @faber-os/shell: the JavaScript SDK an app's UI uses, and the app protocol (docs/apps.md §9)
tools/arch-check/         enforces the crate dependency rules below
tools/faber-cli/          the `faber` command: new connector from a template, check, pack
tools/faberlink-interop/  tests the Rust FaberLink session against the C++ library, compiled for the PC
firmware/faberlink/       the FaberLink Arduino library (C++): the board side of a bridge
templates/                the templates `faber new connector` (Rust/wasm, declarative) and `faber new app` copy
examples/apps/            example apps, tested in CI (the wall follower)
wit/                      capability interfaces, the connector world (wit/connector) and the app world (wit/app)
fixtures/                 test data, e.g. a real LD19 capture
```

## Architecture rules (enforced)

**Crate dependencies** point only downwards:

```
faber-core ──► faber-connector ──► faber-capabilities
    └────────► ldrobot-ld19 ──► faber-connector
                    └ (wasm32 only) ──► faber-connector-wasm ──► faber-connector
```

| Crate | May depend on | Must not |
|---|---|---|
| `faber-capabilities` | `serde`, plus `ts-rs` behind the `typescript` feature (code generation only) | anything with I/O |
| `faber-connector` | `faber-capabilities`, `serde`, `toml` | I/O, async runtimes, `faber-core` |
| `faber-connector-wasm` | `faber-connector`, `faber-capabilities`, `toml`, `wit-bindgen` (the component glue; its generated code is the only `unsafe` allowed outside the core's dependencies) | I/O, `faber-core` |
| apps (`examples/apps/*`, and the core's test apps in `core/tests/wasm-guests/app-*`) | `faber-app-wasm`, `faber-capabilities` | anything else. An app sees the machine only through its roles. |
| connectors (`connectors/*`, and the core's other test guests in `core/tests/wasm-guests/*`) | `faber-connector`, `faber-capabilities`, `toml`, and `faber-connector-wasm` for their wasm build | **any I/O crate** (serial, net, fs, tokio). Connectors get bytes in and give readings out. The core owns all I/O. |
| `faber-app-wasm` | `faber-capabilities`, `wit-bindgen` (the component glue, as for `faber-connector-wasm`) | I/O, `faber-core` |
| `faberlink` (the bridge protocol, [faberlink.md](faberlink.md)) | `faber-capabilities` | any I/O crate. It turns bytes into messages and back; the clock is passed in. |
| `faber-flash` (the Arduino flasher: Intel HEX, STK500v1 for Optiboot, STK500v2 for the Mega) | nothing | any I/O crate. The serial port, its reset line and the clock come in through a trait; its simulated bootloaders are the tests' boards. |
| `faberlink-interop` (tests only) | `cc` (a build dependency: it compiles the Arduino library's C++ core and a fake-board harness). Its FFI module is the only other `unsafe` code in the workspace. The core's tests use its fake board (a dev-dependency). | anything else |
| `faber-core` | everything above, plus I/O crates | — |
| `faber-cli` (the `faber` command, for connectors and apps) | `faber-core` (its package checks and its sandbox, so `faber check` accepts exactly what a machine runs), `faber-connector`, `faber-capabilities`, `clap`, `toml`, `wit-component` | anything else |

**Inside `faber-core`**, the modules form layers. A module may only use the layers below it:

| Layer | Modules | Job |
|---|---|---|
| 5 | `api` | HTTP + WebSocket. Translates between the wire and the machine. No logic of its own. |
| 4 | `machine` | Ties everything together. The only place that changes what runs: it writes `parts.toml` first, then reconciles. |
| 3 | `apps`, `bridge`, `declarative`, `runtime`, `simparts`, `suggest`, `wasm` | Running apps' logic in its sandbox and filling their roles ([apps.md](apps.md)), supervising parts, running FaberLink boards and their sub-parts (`faber-bridge`, [faberlink.md](faberlink.md)), the declarative connector engine (connectors that are only a `faber.toml`), the world engine that runs `simulate = "world"` parts, detect-then-suggest, and running wasm connectors in their sandbox (docs/packages.md). Built on layer 2. |
| 2 | `backup`, `config`, `connectors`, `disks`, `estop`, `files`, `host`, `leases`, `logs`, `packages`, `ports`, `roles`, `router`, `sandbox`, `settings`, `sim`, `simulation`, `translations` | One job each. They don't know about HTTP or each other. `sandbox` is the WebAssembly engine, limits and WIT bindings that wasm connectors and app logic share. |
| 1 | `clock`, `names` | Small pure building blocks. |

This is enforced by `core/tests/layers.rs`, which fails with the file and line of any import that
points up or sideways. Every module that does I/O does it behind a trait, so its tests use fakes
(fake clock, fake serial discovery, in-memory ports).

**In the shell:**

| Folder | Job | May import |
|---|---|---|
| `src/lib/i18n/` | Every text the shell shows: the English catalogue (`en.ts`), `t()`, translations and their checks ([translations.md](translations.md)) | nothing |
| `src/lib/api/` | The only code that talks to the core (REST + WebSocket, MessagePack). Generated types live in `generated/`. | `i18n`, nothing from the UI |
| `src/lib/state/` | App state as Svelte 5 runes (parts, ports, windows) | `api`, `i18n` |
| `src/lib/desktop/` | Plain TypeScript logic for the desktop and its accessories (windows, icons, menus, file paths, the Notes document), no Svelte | `i18n` |
| `src/lib/viewers/` | Canvas drawing for capability viewers, and the display units (`units.ts`), as pure functions | `i18n` |
| `src/components/`, `src/windows/` | Svelte UI | `state`, `desktop`, `viewers`, `i18n`, never `api` directly |
| `../sdk/js/shell/src/protocol.ts` | The app protocol's types, shared with the SDK | nothing; only the app window host (`src/lib/state/appHost.ts`) imports it, never the SDK itself |

**Texts** (D44): no visible text is written straight into a component or module. It goes into
`src/lib/i18n/en.ts` and is shown with `t('key')`; `src/lib/i18n/noRawText.test.ts` fails on any
that isn't. How translations work, and `npm run i18n:check`: [translations.md](translations.md).

## Testing

| What | Tool | Where |
|---|---|---|
| Rust units | `cargo test` | next to the code (`#[cfg(test)]`) |
| Rust integration | `cargo test` | `core/tests/`. A real LD19 capture goes through the whole core: replay port → connector → router → WebSocket |
| Shell units | Vitest + Testing Library | `shell/src/**/*.test.ts` |
| End-to-end | Playwright | `shell/e2e/`. Builds the shell, starts the real core with it compiled in (`embed-shell`), adds a part on the demo recording and checks that a scan is drawn, at desktop and 7" sizes |
| Wasm connectors | `cargo test` | `core/tests/wasm.rs` builds the LD19 and a misbehaving test guest for `wasm32-unknown-unknown` and runs them in the sandbox. Without that target the tests are skipped with a message (`rustup target add wasm32-unknown-unknown`); CI sets `FABER_REQUIRE_WASM=1` so they always run there |
| The `faber` CLI and its templates | `cargo test` | `tools/faber-cli`: `tests/first_connector.rs` is the Phase 3 proof (a new connector from the template, tested, checked, packed, installed through the API on a test core, run on a recording and uninstalled), skipped like the wasm tests without the target; the unit tests check and pack the declarative template and write the fixtures the templates and the e2e test use |
| The Arduino flasher | `cargo test` | `sdk/rust/flash`: Intel HEX (checksums, extended addresses, cut-off files) and both bootloader protocols against simulated bootloaders written from the specs (AVR061, AVR068): write, read back and compare a fake flash; no answer, a wrong chip, a board falling silent, a bad flash cell, damaged answers. `core/tests/flash.rs` flashes through the HTTP API behind a fake serial port. Never a real port |
| FaberLink | `cargo test` | `sdk/rust/faberlink`: property tests for the framing, every message and capability layout, and the session state machine. `tools/faberlink-interop`: the Rust session against the Arduino library's C++ core, compiled on the PC with the `cc` crate (MSVC on Windows, g++/clang elsewhere) and driven over an in-memory pipe with a fake clock: handshake, readings, commands, the board's safety rules, corruption and version mismatch. `core/tests/bridge.rs` is the Phase 4 proof: the same C++ board behind a fake serial port, driven through the HTTP API |
| Packages | `cargo test` | `core/tests/packages.rs`: preview, install while the core runs, parts restarting on an update, uninstall refused while used, and every refusal with its hint |
| Apps | `cargo test` | `core/tests/apps.rs` is the Phase 5a proof: the wall-follower example (`examples/apps/wall-follower`), built and packed, installed on a test core, bound to a simulated SENNEP in the apartment, drives along a wall without bumping and publishes its derived signal; turning it off, a part leaving a role, an operator taking over and a trap in a test app (`core/tests/wasm-guests/app-trap`) each end its lease and stop the base. `tools/faber-cli/tests/first_app.rs` takes `faber new app` from the template to running on a core. Skipped like the wasm tests without the target |
| The app SDK | Vitest | `sdk/js/shell`: `npm test` (against a fake shell window; also checks that the example apps carry the built SDK), `npm run check`, `npm run format:check`, `npm run build` |
| App windows | Vitest + Playwright | `shell/src/lib/state/appHost.test.ts` (the host against hostile messages), the window tests, and `shell/e2e/apps.spec.ts` (the packed wall follower, `shell/e2e/fixtures/`, written by `core/tests/apps.rs`; its component is refreshed only with `FABER_REFRESH_FIXTURES=1`, since its bytes depend on the toolchain) |
| Interfaces | `wasm-tools component wit wit/capabilities`, the connector world (see [wit/connector](../wit/connector/README.md)) and the app world (see [wit/app](../wit/app/README.md)) | CI |

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
