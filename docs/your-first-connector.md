# Your first connector

From nothing to a connector installed on a Faber machine: `faber new connector`, change the
parser, `faber check`, `faber pack`, then install the package in the shell.

A **connector** makes one kind of hardware usable: it gets the bytes that arrive on a port and
turns them into readings (a `distance`, a `position`, …). Faber opens the port, so a connector
never does I/O itself, and it runs sandboxed: it can't touch files, the network or other
hardware. More: [packages.md](packages.md).

Every step below is tested on every build. The tag after it says where:

- **[proof]** `tools/faber-cli/tests/first_connector.rs`: this whole guide for a Rust connector,
  in order, on a temporary folder and a test core;
- **[cli]** the unit tests in `tools/faber-cli/src/`;
- **[e2e]** `shell/e2e/packages.spec.ts`: installing through the shell, in a real browser;
- **[shell]** `shell/src/windows/InstallPackageWindow.test.ts`: the Install window, including a
  file dropped from your computer.

## Which kind?

Start with the easiest one that fits (docs/principles.md):

| Your device… | Kind | You write |
|---|---|---|
| prints text lines (`D:123`, `T=21.5`, NMEA from a GPS) | **declarative** | only `faber.toml` ([connectors-declarative.md](connectors-declarative.md)) |
| speaks a binary protocol, needs state or maths | **wasm** | Rust, built as a sandboxed WebAssembly component |

Both are made, checked, packed and installed the same way. This guide uses Rust; for a
declarative one, add `--kind declarative` to `faber new` and skip step 3.

## What you need

- [Rust](https://rustup.rs), with the WebAssembly target:

  ```bash
  rustup target add wasm32-unknown-unknown
  ```

- A checkout of Faber, and the `faber` command built from it:

  ```bash
  cargo install --path tools/faber-cli
  ```

  For now the template uses the connector SDK (`faber-connector`, `faber-connector-wasm`) from
  that checkout, by path. Once the SDK is on crates.io, the template will use version numbers
  and no checkout will be needed.

- A Faber core to install it on (see the README's quick start). No hardware needed: the
  template comes with a recording.

## 1. Make the project

```bash
faber new connector acme-ranger
cd acme-ranger
```

The name is the connector's id: lowercase words joined by `-`. **[proof] [cli]** You get:

```
acme-ranger/
  Cargo.toml          the SDK by path, and the wasm glue for wasm32 only
  faber.toml          what it is, what port it needs, what it provides
  README.md           shown in Faber's install dialog
  LICENSE             MIT; change it if you like
  src/lib.rs          the connector: a line parser to replace with yours
  tests/capture.rs    replays the recording through the connector
  captures/sample.fbcap   a recording of the sample device
```

## 2. Run its tests

```bash
cargo test
```

They pass before you change a line. **[proof]** `tests/capture.rs` feeds
`captures/sample.fbcap` to the connector exactly as Faber does and checks the five distances it
should give.

## 3. Write your parser

The template reads lines like `D:123` (123 cm). Your device is different, so change
`parse_line` in `src/lib.rs`:

```rust
/// One line from the device, or `None` if it isn't one we understand.
pub fn parse_line(line: &str) -> Option<f32> {
    let cm: f32 = line.trim().strip_prefix("D:")?.parse().ok()?;
    Some(cm / 100.0) // Faber works in metres, always
}
```

Then:

- set the baud rate and the range from your datasheet in `faber.toml` (`[ports]`, `info`). The
  Rust reads `faber.toml` when it is built, so it is the only place to change them;
- record your own device: in Faber, open the **Serial monitor**, pick its port and baud rate,
  press **Record**, and copy the `.fbcap` file from `Recordings` into `captures/`;
- change `tests/capture.rs` to expect what your recording should give.

How the code is built, briefly: `definition()` is what Faber starts; `create` makes one
`MySensor` per part; `on_bytes` gets the bytes as they arrive (a line can be split across two
reads, so it keeps the unfinished line); it publishes a `distance` reading per good line and
reports its health (`starting`, `running`, or `degraded` when data arrives that it can't read).
The one line `faber_connector_wasm::export!(definition);` turns the same code into a
WebAssembly component.

## 4. Check it

```bash
faber check
```

```
Checking acme-ranger 0.1.0 (wasm connector)
Building for wasm32-unknown-unknown …
ok   The component (696 KB) loads in the sandbox and asks only for the connector world.
ok   captures/sample.fbcap: 5 chunks, 5 on distance; health running
       last reading: 0.400 m, ok
All good. Run faber pack to make the package.
```

It builds the component, loads it in the same sandbox the core uses, and replays every
recording in `captures/`, saying how many readings each gave and the health it ended with.
**[proof] [cli]**

## 5. Pack it

```bash
faber pack
```

It checks again, then writes `acme-ranger-0.1.0.faberpkg`: a zip with `faber.toml`,
`connector.wasm`, `README.md`, `LICENSE` and your recordings. A recording that fails stops the
pack. **[proof] [cli]**

## 6. Install it

Drop the file on Faber's desktop or into **Files** **[shell]**, or double-click it in Files if it
is already on the machine **[e2e]**. The **Install** window shows the connector's name, kind, version and publisher, what it
can touch ("a serial port you pick, at 9600 baud", nothing else), its README, and that it is
**not checked by the Faber registry**. Click **Install**. **[e2e] [proof]**

No restart: the connector is in **Add part** at once, and **Add part** opens on it.

Without the shell: `POST /api/v0/packages` with the file as the body
([api.md](api.md#packages)). **[proof]**

## What you should see

Add a part with your connector on your device's port (or on your recording: it is a replay
port, `replay:<name>`). The part turns green, and its viewer shows the readings. **[proof] [e2e]**

## Updating and uninstalling

- **A new version:** change `version` in `faber.toml`, pack, install the new file. The window
  says which version it replaces; parts using the connector restart with the new one.
- **Uninstall:** Settings → Connectors → Uninstall. Faber refuses while a part uses the
  connector and names the part; remove it in Parts first. **[e2e] [proof]**

## If it doesn't work

| What you see | What to do |
|---|---|
| `Rust can't build for wasm32-unknown-unknown on this computer yet.` | `rustup target add wasm32-unknown-unknown` |
| `The connector didn't build.` | Fix the errors cargo printed above it. |
| `… asks for wasi:…, which Faber doesn't give connectors.` | Don't build with `--target wasm32-wasip2`, and don't use crates that read files, the clock or the network: Faber passes the time in. |
| `FAIL captures/x.fbcap: … no readings` | The parser recognised nothing in that recording. Look at the recording in the serial monitor: is it from this device, at this baud rate? |
| `faber.toml line 14: …` | The line and the fix are in the message. |
| The Install window says the package can't be installed | The message says why and what to do; the core checks the package in full before it installs anything. |
| The part says **Data arrives, but no line could be read** | The baud rate in `faber.toml` doesn't match the device. |
