# Faber architecture

Status: **draft v0.2** (2026-10-15). Nothing here is built yet. The terms used below are defined in
[glossary.md](glossary.md), and the reasons behind the choices are in [decisions.md](decisions.md).
Anything still undecided is marked **OPEN**.

## 1. Goals and non-goals

**Goals**
1. **Flash, boot, open `faber.local`.** No Linux knowledge needed, ever.
2. **Any hardware becomes typed capabilities.** Add a part, and you instantly get a live viewer
   in the shell and a signal that apps can use.
3. **Open ecosystem.** Anyone can publish connectors and apps through a GitHub-based store.
   Third-party code is sandboxed and must declare its permissions.
4. **One UI, two screens.** The same web desktop runs in a remote browser and on an attached screen.
5. **Safe for things that move.** Leases, safe states and an e-stop are core concepts, not
   something left to each app.
6. **Local-first.** Everything works with no internet and no account. Remote access is opt-in.

**Non-goals** (for now, maybe forever)
- Hard real-time on the Pi. That belongs on a microcontroller behind a [bridge](#6-bridges).
- Being a general home server like CasaOS or Umbrel (media, NAS, Docker apps).
- Replacing ROS 2 for research robots. We may *bridge* to ROS later.
- Fleets of hundreds of machines.
- Writing a kernel or drivers. Faber sits on Debian and the Raspberry Pi kernel.

## 2. The big picture

```
 Remote browser ─┐                                   ┌─ Attached screen (kiosk)
  (faber.local,   │            HTTPS + WebSocket      │   Chromium in Cage
   Tailscale)    └──────────────────┬────────────────┘
                                    ▼
┌────────────────────────────── faber-core (one Rust process) ──────────────────────────────┐
│  Shell server   ── serves the web desktop, the API, auth                                  │
│  Router         ── signals + commands between parts, apps, shell, MQTT/ROS bridges        │
│  Runtime        ── loads packages; WASM sandboxes (Wasmtime); supervises native ones      │
│  Safety         ── leases, safe states, e-stop, watchdog, update lock                     │
│  Port manager   ── owns I2C/SPI/UART/GPIO/USB, grants claims, detects conflicts           │
│  Config         ── reads/writes /data/config/*.toml, validates, reconciles                │
│  Store client   ── registries, install, update, permissions                               │
│  System         ── network (mDNS, hotspot, Tailscale), updates (Rugix), backups, logs     │
└──────┬──────────────────────┬──────────────────────────┬─────────────────────────────────┘
       │ WASM connectors/apps │ native connectors        │ ports
       │ (in-process sandbox) │ (child processes,        │
       │                      │  Unix socket)            ▼
       ▼                      ▼                  I2C · SPI · UART · GPIO · USB · camera
   ld19.wasm,             rpi-camera                     │
   sennep.wasm …                                         ▼
                                              Bridge: Arduino/Pico running FaberLink
                                              (motors, safety stops, fast sensors)
───────────────────────────────────────────────────────────────────────────────────────────
 Debian (Trixie) + Raspberry Pi kernel · read-only A/B root (Rugix Ctrl) · /data partition
```

**One process on purpose.** The core is a single Rust binary (tokio async). WASM packages run
*inside* it, but each one in its own sandboxed Wasmtime instance with memory and CPU limits, so
one crashing connector can't take down the others. Only native connectors are separate
processes. This keeps the system small and easy to debug, and it runs unchanged on a Windows or
Linux PC during development.

## 3. Capabilities

Capabilities are the contract the whole ecosystem depends on, so they get the most care.

- They are defined in **WIT** files in `wit/capabilities/`, versioned (`laser-scan@1`). A breaking
  change means a new major version, and the core can run both side by side.
- Each capability defines:
  - **signals**: typed readings streamed from the part;
  - **commands**: typed requests to the part;
  - **settings**: per-part configuration;
  - **safe state**: required for actuators;
  - a **viewer**: the shell's built-in live view.
- **Data shapes follow ROS `sensor_msgs` / `geometry_msgs`** where they exist (LaserScan, Imu,
  NavSatFix, Twist), in SI units, timestamped in µs. That keeps a ROS bridge cheap later.
- The core ships a **fixed starter set** of 41 capabilities, specified in
  **[capabilities.md](capabilities.md)** (common rules, the lease, and every field). Third parties
  can propose new capabilities through registry review, but cannot invent them ad hoc. This keeps
  apps portable.

**Starter set (v1).** Chosen so SENNEP is fully covered and a typical maker project mostly is:

| Group | Capabilities |
|---|---|
| GPIO-level | `digital-in`, `digital-out`, `analog-in`, `pwm-out`, `servo` |
| Environment | `temperature`, `humidity`, `pressure` |
| Ranging & motion | `distance`, `imu`, `laser-scan`, `position` (GNSS), `pose` (position + orientation in a map frame, mostly published as a derived signal) |
| Actuation | `motor` (velocity/position), `planar-base` (vx, vy, ω for a whole drive base), `light` (RGB strip/segments) |
| Power & control | `battery`, `rc-input` (joystick/radio channels), `estop` |
| Media | `camera` (frames + metadata, served as a video stream to the shell) |

Example (sketch, not final):

```wit
package faber:capabilities@1.0.0;

interface distance {
  record reading { meters: f32, variance: f32, timestamp-us: u64 }
  record settings { min-meters: f32, max-meters: f32, rate-hz: f32 }
  // signal: stream<reading>   (no commands, no safe state)
}

interface motor {
  record state   { velocity-rad-s: f32, position-rad: option<f32>, timestamp-us: u64 }
  record command { velocity-rad-s: f32 }
  // signal: stream<state>; command: set-velocity (needs a lease)
  // safe state: velocity 0
}
```

## 4. Connectors

A connector turns a **part** into capabilities. There are three kinds, and the store shows the kind
clearly, because they carry different trust.

| Kind | What it is | Sandboxed | For |
|---|---|---|---|
| **declarative** | Only an `faber.toml`: which port, which registers/commands, how to scale the value, which capability it outputs. The core executes it. | yes (no code at all) | simple I2C/SPI/UART sensors, GPIO devices |
| **wasm** | A `.wasm` component compiled from Rust/C/Go/… against the WIT capability and port interfaces. | **yes** | anything with real logic: packet parsers, protocols |
| **native** | A normal Linux program that speaks the router protocol over a Unix socket. | **no** (store shows a warning) | camera pipelines, ML, vendor libraries |

All three kinds produce identical capabilities. An app can't tell, and doesn't care, which kind
is behind a part.

**Declarative example** (TMP102 temperature sensor; register values from the TI datasheet):

```toml
faber-manifest = 1

[package]
id = "ti-tmp102"
type = "connector"
kind = "declarative"
version = "1.0.0"
name = "TI TMP102 temperature sensor"

[ports]
i2c = { addresses = [0x48, 0x49, 0x4A, 0x4B], default = 0x48 }

[[provides]]
capability = "temperature@1"
poll = { default-hz = 1 }
read = { register = 0x00, bytes = 2, format = "i16be", shift = 4, scale = 0.0625, unit = "celsius" }
```

**Wasm example manifest** (LD19 LiDAR; the real one is `connectors/ldrobot-ld19/faber.toml`):

```toml
faber-manifest = 1                  # the faber.toml format version; every manifest starts with it

[package]
id = "ldrobot-ld19"
type = "connector"
kind = "wasm"
version = "0.1.0"
name = "LDROBOT LD19 / STL-19P LiDAR"
module = "ld19.wasm"

[ports]
serial = { baud = 230400 }          # any UART or USB-serial port the user picks

[[provides]]
capability = "laser-scan@1"
info = { range-min-m = 0.02, range-max-m = 12.0 }   # optional: its limits, kebab-case

[[usb-hints]]                       # optional: adapters it often comes with (detect-then-suggest)
vid = 0x10c4
pid = 0xea60
reason = "The LD19 kit comes with this CP2102 adapter."
```

**Settings** are declared in the manifest too, one `[[settings]]` table each: its key, a label and
a one-sentence description, its kind (`number`, `bool`, `choice`, `string` or `part-list`), and
for a number its unit, default and limits (`min`, `min-exclusive`, `max`, and a `suggested` value a
new part's form starts at). The shell builds the Add part form from them, and the connector reads
its settings against them, so a default or a limit is written once. Built-in connectors declare
the same thing in Rust (`faber_connector::SettingSpec`); the LD19 has none.

```toml
[[settings]]
key = "max-m"
label = "Farthest it can measure"
description = "Anything farther reads as too far. From the sensor's datasheet."
kind = "number"
unit = "m"
default = 4.0
min = 0.0
min-exclusive = true
```

A default is what a part gets when parts.toml leaves the setting out, so changing one changes
existing machines; it stays put once released.

**What a connector may touch:** only the ports it declared and the user assigned to the part.
A WASM connector physically cannot open anything else: the port manager hands it a handle, not a
path. Native connectors get only the device nodes of their claimed ports, enforced by systemd
sandboxing (`DeviceAllow=`, a private filesystem view). That is weaker than WASM, hence the warning.

## 5. Parts and the machine config

Your machine is described by plain TOML files in `/data/config/`. **The files are the source of
truth.** The shell edits these same files, so UI, git, backups and hand-editing never disagree.

```toml
# /data/config/parts.toml
[parts.front_lidar]
connector = "ldrobot-ld19"
port = "usb:10c4:ea60"              # CP2102 USB-UART adapter
[parts.front_lidar.settings]
mount = { x = 0.0, y = 0.0, yaw-deg = 0 }

[parts.cabin_temp]
connector = "ti-tmp102"
port = "i2c-1"
address = 0x48

[parts.mega]
connector = "faber-bridge"
port = "uart0"
```

`apps.toml` lists installed apps and their settings, and `system.toml` holds hostname, network
and users. A machine config can be exported and imported as one bundle. That's how you share
"my SENNEP setup".

## 6. Bridges

Real-time work (motor loops, safety stops, fast sensors) runs on a microcontroller. A **bridge** is
a microcontroller running the **FaberLink** library: a small, versioned, framed serial protocol with
a handshake, heartbeat and CRC. Its firmware declares which parts it hosts. `faber-bridge` then
makes them appear as normal parts with normal capabilities: **sub-parts** named after the bridge,
`mega.left`, each with its own signals, viewer and lease ([faberlink.md §9](faberlink.md)).

```
 faber-core ── faber-bridge connector ── UART ── Arduino Mega running FaberLink
                                                ├─ 4× motor        → motor@1 / planar-base@1
                                                ├─ 8× VL53L0X      → distance@1
                                                ├─ IMU             → imu@1
                                                └─ RF joystick     → rc-input@1
```

- **Firmware lives in the store too.** A connector or app can ship bridge firmware, and the
  built-in *Arduino flasher* tool installs it. The core checks the firmware's FaberLink version at
  every connect.
- **Safety is enforced on both sides.** The microcontroller stops its own actuators if the
  heartbeat from the Pi is lost. That's the last line of defence and doesn't rely on Linux at all.
- FaberLink is designed fresh (D22). SENNEP's early Pi ↔ Mega pre-work is not the template. SENNEP's
  Mega firmware is rewritten on top of FaberLink once it exists.

## 7. Apps

An app *uses* capabilities to do a job. It never talks to hardware or connectors directly.

```toml
[package]
id = "sennep"
type = "app"
version = "0.1.0"
name = "SENNEP"
module = "sennep.wasm"            # logic (optional: a UI-only app has none)
ui = "ui/"                        # web assets opened as a window in the shell (optional)

[[requires]]
role = "lidar"
capability = "laser-scan@1"

[[requires]]
role = "base"
capability = "planar-base@1"
command = true                    # needs a lease on this part

[[requires]]
role = "proximity"
capability = "distance@1"
count = "1.."                     # one or more

[[suggests]]                      # offered when the user installs the app
connector = "ldrobot-ld19"
for-role = "lidar"

[[publishes]]                     # derived signal, see below
name = "pose"
capability = "pose@1"
```

- **Roles:** on install, the shell asks the user which part fills each role. It offers matching
  parts, or connectors to install from the store if nothing fits. So SENNEP works with any LiDAR
  that provides `laser-scan@1`.
- **Logic** runs as WASM, under the same sandbox and permission rules as connectors.
- **UI** is plain web assets in a shell window. They talk to the core through a small JS SDK
  (`@faber-os/shell`): subscribe to signals, send commands, read and write settings.
- An app can also be **UI-only**, e.g. a dashboard of gauges.
- **Derived signals (D24).** An app may publish computed data under its own name (`sennep:pose`,
  `sennep:nearest-obstacle`), declared in `[[publishes]]` with a capability shape. Derived signals
  are read-only: no ports, no commands, no leases. Another app's role accepts a derived signal only if
  that role says `accept = ["part", "derived"]` (the default is parts only). The shell shows them with
  an "app" badge, so it's always clear what is hardware and what is computed.

## 8. Lifecycle and supervision

Every part and app goes through the same states, shown in the shell:

```
 declared → validated → ports claimed → dependencies ready → running ⇄ degraded
                ↓              ↓                                 ↓
             invalid       conflict                        failed (backoff restart)
```

- **Validate before start:** settings are checked against the manifest schema, and ports against
  conflicts. Errors point at the exact line in the config file.
- **Reconcile:** the core compares the config files with what is running and makes the minimum
  changes. A changed part is rebuilt, not patched (Viam's approach).
- **Crash restart** uses exponential backoff, plus a "stopped after N crashes" state so a broken
  connector doesn't flap forever.

## 9. Router

- Addresses: `part/name` for signals, where the name defaults to the capability's name (`part/capability`) (`front_lidar/laser-scan`), `part/capability.command`
  for commands, and `app:name` for derived signals (`sennep:pose`). The `/` vs `:` separator makes
  hardware vs computed visible in every address.
- Two delivery modes per signal, chosen by the capability: **latest-value** (a sensor at 100 Hz,
  where a slow viewer just gets the newest value) and **every-message** (events, button presses).
  This gives back-pressure for free: a slow subscriber never slows a fast part.
- Rates are capped per subscriber. The shell gets display-rate data, apps get full rate.
- **Outside bridges** (later): an MQTT bridge, which also makes Faber parts show up in Home
  Assistant via MQTT discovery, and a ROS 2 bridge.
- The wire format for WebSocket and native connectors is MessagePack, with JSON as a debug fallback (D26).

## 10. Port manager

- Lists every port on the machine (from the device tree, `/dev`, USB descriptors) and which part
  claims it. The *Ports* tool in the shell shows this live.
- Detects conflicts before anything starts: two parts on one UART, two I2C devices at the same
  address on the same bus, a GPIO used twice.
- **Device-tree overlays:** when a part needs a port that isn't enabled (e.g. a second UART), the
  port manager edits the boot config and asks for a reboot, instead of the user hand-editing
  `config.txt`.
- **Detect, then suggest** ([principles](principles.md)): the port manager scans I²C buses and
  USB, and the registry maps known addresses and USB ids to connectors (*"Found a device at 0x76,
  probably a BME280. Install its connector?"*). The user confirms, and doesn't have to type anything.
- **Built (Phase 6b):** ports `i2c-<bus>` (claimed per address), `spi-<bus>.<cs>` and
  `gpio-<line>` (claimed per line, a bus's own pins reserved while it is on), behind a hardware
  trait: Linux backends (`I2C_RDWR`, spidev, the GPIO character device's v2 interface) and fake
  buses for tests and `--fake-hardware`. The I²C scan only reads (docs/api.md, "I²C scan").
- USB devices are matched by vendor:product id (plus serial number when there are two of the same
  kind), so `ttyUSB0`/`ttyUSB1` swapping after a reboot doesn't break anything.

## 11. Safety

1. **Leases.** Only one holder at a time can command an actuator part. A lease has a deadline (e.g.
   250 ms), and every command renews it. If it lapses (the app crashed, hung or lost Wi-Fi), the
   core sends that part's **safe state**. Operators can always take over. The full rules are in
   [capabilities.md](capabilities.md#the-lease).
2. **E-stop** is a core capability with a button that is always visible in the shell (and the
   kiosk). It drops all leases and sends every actuator to its safe state. A hardware e-stop on a
   bridge is the bridge's own `estop@1` signal (`mega/estop`): pressing it engages the e-stop,
   which then can't be released until the button is reset on the board.
3. **Update lock.** No core, package or OS update is applied while any actuator lease is held.
4. **Bridge heartbeat.** The microcontroller makes itself safe without the Pi (see §6).
5. **Connector death.** If a connector driving an actuator dies, its parts go to their safe state
   through the bridge, or the port is released.

## 12. Shell and kiosk

- The **shell** is a single-page web app served by the core: desktop, windows, taskbar, launcher,
  notifications, e-stop button. It shows built-in tools, app windows and part viewers.
- **The same shell runs on the attached screen.** The image starts a minimal Wayland kiosk
  (**Cage** plus Chromium) pointing at `http://localhost`. It is touch-friendly, and windows go
  full-screen on small displays.
- Built-in **tools** (v1): Parts, Ports, Serial monitor, I2C scanner, GPIO panel, Signal plotter,
  Arduino flasher, Logs, Terminal (admin only), Settings, Store.
- Built with **Svelte 5 + Vite** as static files that the core serves (D28). There is no Node.js on
  the machine. Heavy views (scans, plots, maps) draw on `<canvas>`/WebGL.
- **App UIs** run in sandboxed iframes and use `@faber-os/shell`, so app authors can pick any
  framework. A shared **widget kit** of Lit web components (`<faber-gauge>`, `<faber-scan-view>`)
  comes later.

## 13. Networking and access

| Mode | Default | Notes |
|---|---|---|
| **Local (mDNS)** | on | `faber.local`, or `<hostname>.local`. Windows 10+, macOS, Linux, iOS and Android all resolve it. |
| **Hotspot fallback** | on | If no known Wi-Fi is found, the machine opens `Faber-xxxx` with a captive portal. Then `faber.internal` works, because the core runs its own DNS in this mode. |
| **Tailscale** | off (built in) | One toggle plus a login link. Reachable from anywhere at `faber.<tailnet>.ts.net` with a real HTTPS certificate. |
| **Public tunnel** | off | Cloudflare Tunnel or similar, to your own domain. It shows a warning ("this machine can move") and requires login plus HTTPS. |

**Auth** is in the core from day one. Users have roles (**admin**, **operator** who may command
actuators, **viewer**). Remote access always needs a login. The kiosk can optionally require a PIN.
Scripts use API tokens.

## 14. Image, updates and provisioning

- **Image:** Debian Trixie + the Raspberry Pi kernel, built with **rpi-image-gen**. Partitions:
  `boot`, `system-a`, `system-b` (read-only), `data` (everything that is yours).
- **OS updates:** whole-system A/B with **Rugix Ctrl** on the Pi firmware's `tryboot`. A new system
  is written to the inactive slot and booted once. If the core doesn't report healthy, it falls
  back automatically.
- **Package updates** are separate from OS updates and per package, with rollback to the previous
  version kept on disk.
- **Update channel:** signed bundles on GitHub Releases plus a JSON index. No server of our own,
  and no account.
- **Headless provisioning:** an `faber.txt` file on the boot partition (Wi-Fi, hostname, admin
  user, packages to install), written by Raspberry Pi Imager customisation or by hand (DietPi's idea).
- **Backups:** one archive of `/data` (config, package data, users). Restore it onto a fresh card
  and the machine is back.
- **Dev mode:** `faber-core` also runs on a normal PC (Windows/Linux/macOS) with mock ports and USB
  serial. That's how development happens until there is an image.

## 15. Store and registry

- **Registry** = the GitHub repo `faber-registry` (Umbrel's model):

```
faber-registry/
  registry.toml                 # id = "faber", name, description
  packages/
    ldrobot-ld19/
      package.toml              # manifest copy + source repo + release URL + sha256 per version
      icon.svg
    sennep/ …
```

- **Publishing** means a PR adding or updating `packages/<id>/`. CI checks:
  - the manifest is valid;
  - the release exists and its checksum matches;
  - the WASM component implements the capability interfaces it claims;
  - declarative connectors run against a mock port.
  
  A human reviews. Native connectors get a stricter review.
- **Package file:** `<id>-<version>.faberpkg`, a `tar.zst` holding the manifest, `.wasm`, UI assets
  and optional bridge firmware. The registry pins each package's sha256 at review. The index and
  OS images are signed with minisign, and the registry moves to TUF later (D27).
- **Third-party registries** are added by URL in the shell. Their package ids are prefixed with
  the registry id (`acme/fancy-lidar`), so names never collide with the official registry.
- **Install** shows permissions (ports, network, commanded capabilities) before anything runs.

## 16. Tech stack

| Layer | Choice | Why |
|---|---|---|
| Core | Rust, tokio, axum | Same language as SENNEP's stack; safe, fast, a single binary |
| Sandbox | Wasmtime + Component Model / WIT | The standard WASM runtime, written in Rust |
| Config | TOML | Rust-native, no YAML indentation traps, comments allowed |
| Shell | TypeScript, Svelte 5 + Vite, static SPA (D28) | Fine-grained updates for many live values, small, readable |
| Widget kit | Lit web components (later) | Usable from any app framework |
| Kiosk | Cage + Chromium | Minimal Wayland kiosk, used by rpi-image-gen's examples |
| Image | rpi-image-gen, Debian Trixie | Official, keeps apt and Pi kernels |
| Updates | Rugix Ctrl on tryboot | Proven at Umbrel, permissive licence |
| Bridge firmware | Arduino C++ library (FaberLink) | Matches SENNEP's Mega; PlatformIO-friendly |
| Licence | Apache-2.0, with a `NOTICE` file (D19) | Open source, credit kept, patent protection |

## 17. Repository layout (planned, not created yet)

```
faber/                         # main monorepo
  core/                         # faber-core (Rust)
  shell/                        # web desktop (TypeScript)
  wit/                          # capability + host interfaces (the public API)
  sdk/                          # Rust/C SDKs, JS @faber-os/shell, package templates
  connectors/                   # first-party connectors (ld19, tmp102, faber-bridge, …)
  firmware/faberlink/            # Arduino library for bridges
  image/                        # rpi-image-gen config, Rugix setup
  docs/
faber-registry/                  # separate repo: the store index
```

## 18. SENNEP as the reference machine

SENNEP is the first real user and the field test. It is retrofitted onto Faber rather than Faber
being shaped around SENNEP's existing code (D22). If something in this table can't be done, Faber
is missing a feature, and that's how the roadmap is driven.

| SENNEP hardware (from its inventory) | Port | Faber part | Connector | Capability |
|---|---|---|---|---|
| LD19 LiDAR via CP2102 USB-UART | USB serial | `lidar` | `ldrobot-ld19` (wasm) | `laser-scan@1` |
| Arduino Mega (motors, IMU, safety stops, VL53L0X ×8 via TCA9548A, HC-SR04 ×4, NRF24 RF) | UART | `mega` → many parts | `faber-bridge` | `planar-base`, `motor`, `imu`, `distance`, `rc-input`, `estop` |
| Nano/Uno + WS2812B strip | USB serial | `leds` | `faber-bridge` | `light@1` |
| NEO-6M GPS | UART, 9600 baud | `gps` | `nmea-gps` (wasm, generic) | `position@1` |
| Raspberry Pi AI Camera | CSI | `camera` | `rpi-camera` (native) | `camera@1` |
| 3S6P battery (measured on the Mega) | via bridge | `battery` | `faber-bridge` | `battery@1` |
| Waveshare 7" HDMI touch screen | HDMI + USB touch | — | — | the kiosk |

The `sennep` app requires `laser-scan`, `planar-base` (with lease), `distance` (1..), `imu`,
`battery`, and optionally `rc-input`, `camera`, `position` and `light`. It publishes `sennep:pose`
as the first real test of derived signals.
