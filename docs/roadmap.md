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
- [x] Shell v0: desktop, windows, Parts window with Add part, laser-scan viewer, Serial monitor, Logs
- [x] End-to-end test in a real browser, against the release build (shell compiled into the core)
- [ ] Tested with the real LD19 on Mikkel's PC

A `distance` viewer moves to Phase 2, where simulated parts give it something to view.

**Done when:** you plug the LD19 into the PC, add it as a part, and see the scan live in a
browser window at `localhost`.

## Phase 2: Drive before you wire (simulation)

Goal: write and test driving and safety code with no motors or sensors connected (D36, D37).

Design: [simulation.md](simulation.md).

- [x] Actuator and ranging capabilities: `motor`, `planar-base`, `distance`, with commands over the API
- [x] **Leases, safe states and the e-stop** in the core (moved here from the bridges phase, so they are
  tested in simulation before real motors exist)
- [x] **Simulated parts** for every implemented capability: sensors give values you set (slider) or a
  pattern (steady, sine, noise, step); actuators accept commands and report a plausible state
- [x] **The world**: 2D and kinematic only. A floor plan, the machine's footprint and pose, and
  simulated `laser-scan` / `distance` / `planar-base` computed from each part's `mount`
- [x] **Prebuilt worlds**, realistic layouts to test against:
  - an empty test floor;
  - an apartment: rooms, doorways, a hallway and furniture with thin legs (hard for LiDARs);
  - an office or classroom: desks, chairs, narrow gaps;
  - a warehouse: long aisles and shelves;
  - a corridor maze;
  - an obstacle course with a person walking a path (a moving obstacle)
- [x] A manual-drive window (on-screen joystick) and a world viewer: the room, the machine, and its rays
- [x] Your own driving code can run as a program that talks to the API, until apps exist (Phase 5):
  `examples/drive-square.py`

**Status (2026-10-15):** done. `core/tests/world.rs` proves the "done when" below, and it was
checked by hand in the browser.

**Done when:** SENNEP's parts are simulated in the apartment world, a program drives the base
with the lease held, the simulated LiDAR scan shows the room, and dropping the lease or pressing
the e-stop stops the base.

## Phase 3: Packages and sandboxing

Design: [packages.md](packages.md). Guide: [your-first-connector.md](your-first-connector.md).

- [x] WIT interfaces for the starter capabilities and the port API
- [x] Wasmtime runtime: load, limit, restart, permissions
- [x] Move `ldrobot-ld19` to a `.wasm` connector, and prove it behaves identically
- [x] Declarative connector engine, plus examples for serial line devices (`line-distance`,
  `nmea-gps`). The `ti-tmp102`-style register maps wait for the I²C bus (Phase 6)
- [x] Package format (`.faberpkg`), local install from a file (Files, a drop, or the API), live,
  with no restart; uninstall in Settings → Connectors
- [x] Rust connector template + `faber` CLI (`faber new connector`, `faber check`, `faber pack`)
- [x] A `position@1` viewer (coordinates, fix, satellites; a map link when online)

**Status (2026-10-15):** done. `tools/faber-cli/tests/first_connector.rs` proves the "done when"
below: a connector made from the template in a fresh folder is tested, checked, packed,
installed on a running core, gives readings from inside the sandbox, and is uninstalled. It was
also run by hand against a separate core, and `shell/e2e/packages.spec.ts` installs a package
through the shell.

**Done when:** a third party could write a connector from the template, and the core runs it sandboxed.

## Phase 4: Bridges

- [x] **FaberLink** v1 protocol and Arduino library, designed fresh (D22) ([faberlink.md](faberlink.md))
- [ ] SENNEP's Mega firmware retrofitted onto it
- [x] `faber-bridge` connector: a board's parts as sub-parts (`mega.left`), leases, `SAFE`, the
  e-stop both ways, reconnect, and "Check for a FaberLink board" in Add part
  ([faberlink.md §9](faberlink.md#9-in-the-core-the-faber-bridge-connector))
- [x] Bridge heartbeat (leases, safe states and the e-stop exist since Phase 2)
- [ ] Built-in tool: **Arduino flasher**

**Status (2026-10-15):** the connector is built. `core/tests/bridge.rs` proves the "done when"
below against the Arduino library's C++ core behind a fake cable; it still needs a run on a real
Mega ([connect-an-arduino.md](connect-an-arduino.md) says which steps need hardware).

**Done when:** a Mega with FaberLink exposes motors and distance sensors, an app holding a lease
drives a motor, and killing the app stops the motor.

## Phase 5: Apps and the SENNEP app

- App manifest, roles, install-time role assignment
- `@faber-os/shell` JS SDK for app UIs
- **`sennep` app v0:** LiDAR view, manual drive (on-screen joystick), battery, proximity ring
- Derived signals: `sennep:pose` published and shown in the shell (D24)
- Machine-config export/import

**Done when:** SENNEP drives by hand from a browser window through Faber on the PC (Mega over USB).

## Phase 6: The Pi and the image

- Port manager: I2C, SPI, GPIO, Pi UARTs, device-tree overlays, detect-then-suggest for I²C and USB
- Tools: **I2C scanner**, **GPIO panel**, **Ports**, **Signal plotter**
- rpi-image-gen image: read-only A/B (Rugix Ctrl), `/data`, `faber.txt` provisioning
- Kiosk on an attached screen (Cage + Chromium)
- Network: mDNS, hotspot fallback with captive portal, Tailscale toggle
- Auth: users, roles, login

**Done when:** flash the card, boot SENNEP's Pi, open `faber.local`, and get the same desktop as
on the 7" screen.

## Phase 7: The store (first public release, v0.1)

- `faber-registry` repo + CI validation
- Store window in the shell: browse, permissions dialog, install, update, rollback
- Third-party registries by URL
- Signed registry index and OS images with minisign, later TUF (D27)
- Docs site (Astro Starlight): "your first connector" guide; every example tested in CI ([principles](principles.md))

**Done when:** someone who isn't Mikkel publishes a connector and someone else installs it.

## Later (unordered)

- **World creator**: draw your own sandbox (walls, furniture, obstacles, moving paths) for your
  project, or start from a real LiDAR recording of a room
- Optional mecanum wheel model in the world, to test wheel kinematics (`motor` ×4 → `planar-base`)

- MQTT bridge, which also brings Home Assistant discovery for free
- ROS 2 bridge
- Recording and replay of signals (great for debugging robots)
- A visual "wiring" view of parts → apps (low-code, Node-RED-like, but typed)
- More capabilities through registry review (`gas`, `light-level`, `load-cell`, `encoder`, …)
- Camera pipeline + the AI Camera's on-sensor inference
- Multiple machines (SENNEP's Pi plus a base station)
- Optional Raspberry Pi Connect update channel
