# Roadmap

Ordered by dependency, not by date. Each phase ends with something that **works and can be shown**.
SENNEP decides what is "enough": a phase is done when SENNEP can use it. Everything is "not
started" as of 2026-10-15.

## Phase 0: Design (now)

- [x] Prior-art research ([research/](research/))
- [x] Glossary, architecture draft, decisions log
- [x] Name: Faber (D25)
- [x] Mikkel reviewed all decisions (D01–D28 accepted)
- [x] Repo with licence and README
- [ ] Write the starter capability WIT files on paper (final field lists)
- [ ] CI skeleton

**Done when:** the repo exists and the docs are agreed.

## Phase 1: Core on the PC (no Pi needed)

Goal: `faber-core` runs on Windows and shows live data from real hardware over USB.

- Core skeleton: config loading (TOML), lifecycle states, logging
- Router with latest-value and every-message signals
- Port manager for **USB serial** only (works on Windows)
- Shell v0: a desktop, windows, a Parts window, a viewer for `distance` and `laser-scan`
- Built-in tools: **Serial monitor**, **Logs**
- First connector, written as built-in Rust first (no WASM yet): `ldrobot-ld19` (the LD19 is the one part with working code today, in `SENNEP V2/lidar/`)
- Mock port for tests

**Done when:** you plug the LD19 into the PC, add it as a part, and see the scan live in a
browser window at `localhost`.

## Phase 2: Packages and sandboxing

- WIT interfaces for the starter capabilities and the port API
- Wasmtime runtime: load, limit, restart, permissions
- Move `ldrobot-ld19` to a `.wasm` connector, and prove it behaves identically
- Declarative connector engine, plus `ti-tmp102`-style examples
- Package format (`.faberpkg`), local install from a file
- Rust connector template + `faber pack` CLI

**Done when:** a third party could write a connector from the template, and the core runs it sandboxed.

## Phase 3: Bridges and safety

- **FaberLink** v1 protocol and Arduino library, designed fresh (D22). SENNEP's Mega firmware is retrofitted onto it
- `faber-bridge` connector
- Leases, safe states, e-stop button, bridge heartbeat
- Built-in tool: **Arduino flasher**

**Done when:** a Mega with FaberLink exposes motors and distance sensors, an app holding a lease
drives a motor, and killing the app stops the motor.

## Phase 4: Apps and the SENNEP app

- App manifest, roles, install-time role assignment
- `@faber-os/shell` JS SDK for app UIs
- **`sennep` app v0:** LiDAR view, manual drive (on-screen joystick), battery, proximity ring
- Derived signals: `sennep:pose` published and shown in the shell (D24)
- Machine-config export/import

**Done when:** SENNEP drives by hand from a browser window through Faber on the PC (Mega over USB).

## Phase 5: The Pi and the image

- Port manager: I2C, SPI, GPIO, Pi UARTs, device-tree overlays
- Tools: **I2C scanner**, **GPIO panel**, **Ports**, **Signal plotter**
- rpi-image-gen image: read-only A/B (Rugix Ctrl), `/data`, `faber.txt` provisioning
- Kiosk on an attached screen (Cage + Chromium)
- Network: mDNS, hotspot fallback with captive portal, Tailscale toggle
- Auth: users, roles, login

**Done when:** flash the card, boot SENNEP's Pi, open `faber.local`, and get the same desktop as
on the 7" screen.

## Phase 6: The store (first public release, v0.1)

- `faber-registry` repo + CI validation
- Store window in the shell: browse, permissions dialog, install, update, rollback
- Third-party registries by URL
- Signed registry index and OS images with minisign, later TUF (D27)
- Docs site: "write your first connector in 15 minutes"

**Done when:** someone who isn't Mikkel publishes a connector and someone else installs it.

## Later (unordered)

- MQTT bridge, which also brings Home Assistant discovery for free
- ROS 2 bridge
- Recording and replay of signals (great for debugging robots)
- A visual "wiring" view of parts → apps (low-code, Node-RED-like, but typed)
- More capabilities through registry review (`gas`, `light-level`, `load-cell`, `encoder`, …)
- Camera pipeline + the AI Camera's on-sensor inference
- Multiple machines (SENNEP's Pi plus a base station)
- Optional Raspberry Pi Connect update channel
