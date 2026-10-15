# Faber

> **Status: Phase 1.** The core and web desktop run on a PC with an LD19 LiDAR (or its built-in recording). Built for the Raspberry Pi 5,
> developed alongside a real robot.

**Faber turns a Raspberry Pi into a hardware workbench you control from a browser.**
Flash an SD card, boot, open `faber.local`, and you get a desktop with your sensors, motors and
microcontrollers in it. Add a part from the store, and you instantly see its live data and can
use it in your project, without writing drivers or touching Linux.

It's like Home Assistant OS, but for **maker and robotics projects** instead of smart homes.
Think of it as **the next step after Arduino, before bare Raspberry Pi**: easy enough to start with
in an afternoon, powerful enough for a robot.

## Why

Every hardware project starts with the same plumbing: find a library for the sensor, SSH into the
Pi, run `i2cdetect`, write a serial parser, build a web page to see what's going on, and pray the
SD card survives a power cut. Faber does that plumbing once, for everyone:

- **Parts become capabilities.** A LiDAR is a `laser-scan`, a ToF sensor is a `distance`, a motor
  controller is a `motor`. Apps use capabilities, so you can swap hardware without rewriting the app.
- **A store for hardware support.** Connectors for individual components are published by the
  community. The more people add, the less code the next project needs.
- **Bench tools built in:** serial monitor, I2C scanner, GPIO panel, live plots, Arduino flasher.
- **Safe for things that move:** e-stop, leases with timeouts, and microcontrollers that stop
  themselves if the Pi goes quiet.
- **Sandboxed third-party code** (WebAssembly). A connector from the store can't touch anything it
  didn't ask for.
- **One desktop, two screens:** the same UI in your browser and on a screen attached to the Pi.
- **Local-first:** no account, no cloud. Remote access through Tailscale if you want it.

## Try it (Phase 1, on a PC)

You need [Rust](https://rustup.rs) and [Node.js](https://nodejs.org). No hardware needed:
Faber has a built-in recording of a real LD19 LiDAR.

```bash
npm --prefix shell ci
npm --prefix shell run build
cargo run -p faber-core --features embed-shell
```

Open <http://127.0.0.1:7878>, click **Add the first part**, pick the port `replay:demo-ld19` and the
LD19 connector, and watch the scan. Have an LD19? Plug it in and pick its USB port instead.

## The four building blocks

| | What | Example |
|---|---|---|
| **Core** | The system itself: desktop, hardware access, safety, updates, network | always there |
| **Connector** | Makes one kind of hardware usable (from the store) | LD19 LiDAR, TMP102 sensor |
| **Tool** | Built-in utility for working with hardware | serial monitor, I2C scanner |
| **App** | Uses capabilities to do a job (from the store) | a robot, a weather station |

## First user: SENNEP V2

Faber is developed alongside [SENNEP V2](https://github.com/MikkelBlom/SENNEP-V2), a self-driving mecanum robot with
a LiDAR, a Raspberry Pi 5 and an Arduino Mega. SENNEP is the reference machine: whatever it needs
and Faber can't do yet is what gets built next.

## Under the hood

A single Rust core (`faber-core`) on Debian with read-only A/B updates. Third-party connectors and
apps run sandboxed as WebAssembly. A Svelte web desktop, served to your browser and to the Pi's own
screen. More in [docs/architecture.md](docs/architecture.md).

## Docs

| Doc | What's in it |
|---|---|
| [docs/architecture.md](docs/architecture.md) | How it's built: layers, capabilities, connectors, apps, safety, store, image |
| [docs/principles.md](docs/principles.md) | Easy first: the design rules every feature follows |
| [docs/capabilities.md](docs/capabilities.md) | All 41 capabilities, field by field, and the lease |
| [docs/glossary.md](docs/glossary.md) | Every term, with exactly one meaning |
| [docs/decisions.md](docs/decisions.md) | Why things are the way they are, plus open questions |
| [docs/roadmap.md](docs/roadmap.md) | Phases, from "runs on a PC" to "public store" |
| [docs/research/](docs/research/) | What HA, ESPHome, Viam, CasaOS, Umbrel, balena and others do, and what they miss |

## Licence

[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0). See `NOTICE` for credits.
