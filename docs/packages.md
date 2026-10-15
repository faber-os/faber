# Phase 3 design: packages and sandboxing

Status: **design v1, proposed** (2026-10-15). Builds on D03, D07, D08, D09, D10 and D18. The
roadmap's Phase 3 says what "done" means; this says how. Built so far: the wasm sandbox and its
WIT (§3, §4, Phase 3a) and the declarative engine (§6, Phase 3c); packages, the installer and the
tools (§5, §7) are next.

## 1. What a user and a contributor do

- **A user** double-clicks a `.faberpkg` file in Files (or drops it on the desktop). A dialog shows
  the package's name, kind, publisher, what it can touch ("a serial port you pick", "nothing
  else"), and an Install button. The connector then appears in Add part like the built-in ones.
- **A contributor** runs `faber new connector my-sensor`, gets a Rust project that already
  builds and passes its tests against a recorded capture, writes the parser, and runs
  `faber pack` to get `my-sensor-0.1.0.faberpkg`. No Faber source checkout needed.

## 2. Three kinds of connector, one behaviour

| Kind | Runs as | Sandboxed | Phase 3 scope |
|---|---|---|---|
| built-in | Rust inside the core (today's `ldrobot-ld19`) | n/a (part of Faber) | stays for core-team connectors |
| **wasm** | a WebAssembly component in Wasmtime, inside the core process (D09) | **yes** | the main path |
| **declarative** | only `faber.toml`; the core's engine runs it | **yes** (no code) | serial line protocols now; I2C/SPI registers when Phase 6 brings the buses |
| native | a separate Linux process | no (store warns) | **not in Phase 3** (needs the image's systemd sandboxing) |

The proof (roadmap): `ldrobot-ld19` builds both as built-in and as `.wasm`, and a test feeds the
same recorded capture to both and requires **identical readings**.

## 3. The WebAssembly interface (WIT)

One WIT package, `faber:connector@0.1.0`, next to the capability types in `wit/`:

```wit
interface context {
  use faber:capabilities/types.{reading, health};
  publish: func(signal: string, reading: reading);   // only signals in the manifest
  set-health: func(health: health);
  log: func(level: log-level, message: string);
}

world connector {
  import context;
  export create: func(settings: list<tuple<string, setting-value>>) -> result<_, settings-error>;
  export on-bytes: func(t-us: u64, bytes: list<u8>);
  export on-tick: func(now-us: u64);
  export command: func(cmd: command) -> result<applied, command-error>;   // Phase 2's command types
}
```

- **No WASI filesystem, network, environment or clock.** Time is passed in. A connector can only
  publish its declared signals and talk to the port the core hands it. That is the sandbox: it
  is not a policy check, the functions simply do not exist inside the component.
- Bytes in, readings out, exactly like the Rust `Connector` trait today, so built-in and wasm
  connectors share one supervisor (restart with backoff, health, logs).

**Built (Phase 3a).** The WIT is `wit/connector/connector.wit`; the readings, commands, health and
settings values are the capability types from `wit/capabilities` (`setting-value` carries anything
WIT has no case for as TOML text, so a connector sees exactly what parts.toml holds). A connector
author writes the same Rust as for a built-in one and adds one line,
`faber_connector_wasm::export!(definition);` (`sdk/rust/connector-wasm`). The core's host is
`core/src/wasm/`. It loads every `kind = "wasm"` folder in `<data>/connectors/<id>/` (`faber.toml`
plus `connector.wasm`) at start; `GET /api/v0/connectors` shows them as `kind: "wasm"`, and
`GET /api/v0/connectors/problems` says why a folder couldn't be used. The proof is
`core/tests/wasm.rs`: the LD19, built both ways, gives identical readings on the recorded capture
(232 of 232, and the same health changes).

- **Target: `wasm32-unknown-unknown`, then wrapped as a component** (`wasm-tools component new`, or
  the `wit-component` crate as the tests do). `wasm32-wasip2` makes a component directly, but Rust's
  standard library there imports 14 WASI interfaces (stdio, environment, clocks, exit) even for a
  connector that uses none of them, and the core refuses those. `unknown-unknown` imports nothing
  but the connector world.
- A component may import only `faber:` interfaces, and functions only from `context` (the
  capability interfaces it takes types from also declare commands for apps). Anything else is
  refused when it loads, with the names it asked for.
- A panic in the connector is logged by the SDK (`panicked: …`) just before the trap, so the part's
  problem says what panicked, as it does for a built-in connector.

## 4. Limits

| Limit | Default | Why |
|---|---|---|
| Memory | 32 MB per instance | an LD19 parser needs < 1 MB; a Pi 5 runs dozens |
| Time per call | 20 ms (Wasmtime epoch interruption) | a stuck parser must not stall the part's thread |
| Instances | one per part | a crash only takes down that part |

A connector that hits a limit is failed with a plain message ("my-sensor took longer than 20 ms
to handle data and was stopped") and restarted with the usual backoff.

**Built (Phase 3a).** Memory through a Wasmtime `ResourceLimiter`; time through epoch
interruption, with a 5 ms epoch tick, so a call gets at least 20 ms and at most 25 ms. A trap, a
limit or a publish to an undeclared signal ends the call by unwinding with the problem to show;
the runtime treats it as a crash (failed, then restarted with backoff, stopped after repeated
crashes) and starts a fresh instance. Connector log lines are capped at 50 a second per instance.

## 5. The package file

`.faberpkg` is a zip:

```
faber.toml          the manifest (architecture.md §4), with kind = "wasm" or "declarative"
connector.wasm      wasm only
README.md           shown in the install dialog and the store
LICENSE
icon.svg            optional
captures/*.fbcap    optional: recordings, used by `faber check` and offered as samples
```

- Installed into `<data>/packages/<id>/<version>/`; `<data>/config/packages.toml` lists what is
  installed and which version is active (TOML, D13). Uninstall refuses while a part uses it.
- Local install now, from Files or `POST /api/v0/packages` (the file). Signatures and the store
  come in Phase 7 (D27); until then a local install says "not checked by the Faber registry".
- An update keeps the old version until the new one starts cleanly, then removes it.

## 6. The declarative engine (first cut)

For serial devices that send text lines (a lot of hobby sensors and GPS modules):

```toml
[package]
id = "example-line-sensor"
type = "connector"
kind = "declarative"
version = "0.1.0"
name = "Example line sensor"

[ports]
serial = { baud = 9600 }

[[provides]]
capability = "distance@1"
line = { pattern = "D:{meters:f32}", scale = 0.01 }   # "D:123" → 1.23 m
```

Patterns with typed fields cover most line protocols; NMEA (`position@1`) gets a built-in
parser. Register maps for I2C/SPI follow the TMP102 example in architecture.md §4 once Phase 6
brings the buses; until then they are tested against a fake bus.

## 7. Tools

- `faber` CLI (crate `tools/faber-cli`): `faber new connector <name>` (template), `faber check`
  (builds, runs the manifest checks and the captures), `faber pack`.
- The template targets `wasm32-unknown-unknown` (not `wasm32-wasip2`, see §3) with plain
  `cargo build`, wraps the module as a component, uses the `faber-connector` SDK plus
  `faber-connector-wasm`, and has a capture test from day one (principles: examples tested in CI).

## 8. Where it lives

| Piece | Where | Layer |
|---|---|---|
| WIT | `wit/connector/` | — |
| Wasmtime host | `core/src/wasm/` | 3 (beside `runtime`) |
| Declarative engine | `core/src/declarative/` | 3 |
| Package store on disk | `core/src/packages.rs` | 2 |
| API | `GET/POST/DELETE /api/v0/packages` | 5 |
| CLI and template | `tools/faber-cli`, `templates/connector-rust` | — |

## 9. Open questions for Mikkel

1. **Binary size.** Wasmtime adds roughly 10–20 MB to `faber-core` and makes builds slower. Fine
   for a Pi 5 with an SD card, but worth knowing. (Recommendation: accept; it is the sandbox.)
   **Measured (Phase 3a, Windows x86-64, release):** `faber-core` grew from 9.1 MB to 22.9 MB
   (+13.7 MB), and a clean release build of it from about 1 to about 3 minutes. Speed on the LD19
   capture (14,264 chunks, 232 revolutions): built-in about 65,000 readings/s, wasm about
   35,000 readings/s, so wasm takes about twice as long, roughly 0.45 µs per call. The LD19 needs
   10 readings/s. Compiling the LD19 component (325 KB) at start takes about 40 ms in a release
   core (about 0.6 s in a debug one). Not yet measured on a Pi 5.
2. **Native connectors** (camera, ML) wait for the Pi image in Phase 6, where systemd can sandbox
   them. OK to leave them out of Phase 3?
3. **Who may install a local package** before users exist (Phase 6)? Recommendation: anyone, with
   the "not checked" warning, like enabling whole-machine Files.
