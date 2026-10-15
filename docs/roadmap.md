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
- [x] Capability field lists ([capabilities.md](capabilities.md)) and WIT sketches
- [x] Design principles ([principles.md](principles.md))
- [ ] CI skeleton

**Done when:** the repo exists and the docs are agreed.

## Phase 1: Core on the PC (no Pi needed)

Goal: `faber-core` runs on Windows and shows live data from real hardware over USB.

- [x] Core skeleton: config (`parts.toml`, comment-preserving edits, errors with line numbers), lifecycle states, logging
- [x] Router with latest-value and every-message signals
- [x] Port manager for serial ports (USB ids, claims and conflicts) plus replay ports, with the LD19 demo recording built in
- [x] First connector, built-in Rust (no WASM yet): `ldrobot-ld19`, tested against a real capture
- [x] Supervisor: port failures retried, connector crashes restarted with backoff, then stopped
- [x] API v0: REST + WebSocket (MessagePack, JSON debug mode), generated TypeScript types
- [x] Architecture enforced in tests: crate rules (`faber-arch-check`) and core module layers
- [ ] Shell v0: desktop, windows, Parts window with Add part, laser-scan viewer, Serial monitor, Logs
- [ ] End-to-end test in a real browser
- [ ] Tested with the real LD19 on Mikkel's PC

A `distance` viewer moves to Phase 3, together with the first distance connector (no viewer
without something to view).

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

- Port manager: I2C, SPI, GPIO, Pi UARTs, device-tree overlays, detect-then-suggest for I²C and USB
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
- Docs site (Astro Starlight): "your first connector" guide; every example tested in CI ([principles](principles.md))

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
