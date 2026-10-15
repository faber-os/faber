# Glossary

**Rule: one word, one meaning, everywhere.** The UI, docs, code, manifests and error messages
use exactly these words. If something new does not fit, add a term here first, then build it.
Home Assistant spent years untangling "component / integration / platform / add-on". This page
exists so we never have to.

## The five things you can have

| Term | One-line definition | Installable? | Example |
|---|---|---|---|
| **Core** | The Faber program and image: everything that is always there. | no (it *is* the system) | `faber-core`, the shell, the router |
| **Connector** | A package that knows how to talk to one kind of hardware and turns it into capabilities. | yes, from the store | `ldrobot-ld19`, `ti-tmp102`, `faber-bridge` |
| **Tool** | A built-in utility in the shell for working with hardware directly. | no, ships with the core | serial monitor, I2C scanner, GPIO panel |
| **App** | A package with logic and/or UI that *uses* capabilities to do a job, and may publish derived signals. | yes, from the store | `sennep`, `weather-station` |
| **Accessory** | A built-in program in the shell for everyday work that isn't about hardware. | no, ships with the core | Files, Notes, Settings |

Simple tests: does it talk to hardware? Then it's a **connector**. Does it use capabilities to do
something? Then it's an **app**. Is it built in and works with hardware directly? Then it's a
**tool**. Is it built in and has nothing to do with hardware? Then it's an **accessory**. Nothing is
ever two of these.

## All terms

| Term | Meaning |
|---|---|
| **Faber** | The whole project (Latin "maker"). **Faber OS** is its Raspberry Pi image. |
| **Machine** | The computer running Faber, normally a Raspberry Pi. One machine runs one core. |
| **Core** | `faber-core`, the single Rust program that runs everything: port manager, router, runtime, shell server, store client, updater. |
| **Part** | One physical component *you added* to your machine, with a name you chose, e.g. `front_lidar`, handled by one connector. Parts live in your machine config. |
| **Connector** | A package that implements one or more capabilities for a kind of hardware. It has a *kind*: **declarative**, **wasm** or **native** (see [architecture.md](architecture.md#connectors)). |
| **Capability** | A standard, versioned, typed interface such as `distance@1`, `imu@1`, `laser-scan@1` or `motor@1`. It defines the part's **signals**, **commands** and **settings**. Apps depend on capabilities, never on connectors. |
| **Signal** | A stream of typed readings from one capability of one part, e.g. `front_lidar/laser-scan`. |
| **Derived signal** | A stream of typed data an *app* computes and publishes under its own name, e.g. `sennep:pose`. It uses a capability's data shape, but it's never hardware: it can't claim ports or accept commands. |
| **Command** | A request sent to a part, e.g. `left_motor/motor.set-velocity`. Commands that move things need a **lease**. |
| **Info** | Facts a part reports about itself once, not in every reading: range limits, field of view, pixel count, supported modes. |
| **Built from** | A part's info can say it is built from other parts (SENNEP's `planar-base` from its 4 `motor`s). A lease on it locks those parts too. |
| **Leased / shared command** | A *leased* command needs a lease (anything that can hurt something). A *shared* command is harmless, and the last writer wins (lights, displays, tare). See [capabilities.md](capabilities.md). |
| **Setting** | A configuration value of a part or app, validated against its manifest schema. |
| **Lease** | An exclusive, time-limited right for one holder (an app or an operator in the shell) to command one actuator part and the parts it is built from. Every command renews it. If it is released, expires, is revoked (e-stop, update, operator takeover) or the part fails, the part goes to its **safe state**. Full definition in [capabilities.md](capabilities.md#the-lease). |
| **Safe state** | What each actuator capability does when nobody is in control (e.g. a motor stops). Defined by the capability, not the connector. |
| **Port** | A physical hardware interface on the machine: `i2c-1`, `spi0.0`, `uart0`, `gpio17`, `usb:1a86:7523`. Only the port manager opens ports. |
| **Port manager** | The part of the core that owns all ports, grants them to connectors, and detects conflicts (two parts on one UART, two I2C devices at one address). |
| **Router** | The part of the core that moves signals and commands between parts, apps, the shell and outside bridges (MQTT, ROS). |
| **Runtime** | The part of the core that loads, sandboxes, starts, restarts and stops connectors and apps. |
| **Bridge** | A microcontroller (Arduino, Pico, ESP32) running the **FaberLink** firmware library, connected over USB/UART. The `faber-bridge` connector turns it into parts. This is where real-time work lives. |
| **FaberLink** | The small, versioned serial protocol and Arduino/C++ library that a bridge runs. |
| **Package** | The unit the store distributes. There are exactly two package types: **connector** and **app**. |
| **Manifest** | The `faber.toml` file at the root of every package. It declares identity, version, capabilities, permissions and settings. |
| **Permission** | Something a package must declare before it gets it: port claims, network access, which capabilities it may command. It is shown to the user at install. |
| **Store** | The place in the shell where you browse and install packages. |
| **Registry** | The data behind the store: a GitHub repo (`faber-registry`) that indexes packages. Anyone can host an extra registry, and its package ids are prefixed with the registry id. |
| **Machine config** | The files in `/data/config/` that describe *your* machine: its parts, installed apps and settings. The UI edits these same files. |
| **Shell** | The web desktop: windows, taskbar, launcher. The same shell is served to remote browsers and shown on the attached screen. |
| **Kiosk** | The shell running full-screen on a display attached to the machine. |
| **Window** | Anything open in the shell: a tool, an app's UI, a part's viewer. |
| **Viewer** | The default live view the core has for every capability (a plot for `distance`, a 2D scan for `laser-scan`, a 3D orientation for `imu`). Any part gets one for free. |
| **Home folder** | Faber's own files, inside the data folder: Desktop, Documents, Recordings, System and Trash. Files and Notes work here. |
| **System folder** | The folder in the home folder that shows the machine config (`parts.toml`). Saving a file there is checked first: a broken config is refused with its line number. |
| **Trash** | Where deleted files go. Emptying it deletes them for good. |
| **Recording** | A capture (`.fbcap`) of a port's bytes, made with the serial monitor's Record button and kept in Recordings. Any recording can be replayed as a port. |
| **Developer settings** | Settings for people building Faber or connectors, behind a warning. Includes full-machine file access. |
| **Simulated part** | A part with no hardware behind it: the core simulates its capability, either by hand (`manual`: values you set, or a pattern) or in the world. Same name and signals as the real part, so apps can't tell the difference. |
| **World** | The core's 2D simulation of a space (floor plan, obstacles) with the machine in it. Simulated parts with a `mount` measure and move in it. Faber ships prebuilt worlds. |
| **SDK** | The libraries, templates and WIT files for writing connectors and apps. |
| **WIT** | The interface language used to define capabilities for WASM packages ([Component Model guide](https://component-model.bytecodealliance.org)). |

## Words we deliberately do not use

| Avoid | Why | Say instead |
|---|---|---|
| component | It means a WASM component, an ESPHome component *and* a Viam component | **part** (the hardware) or **connector** (the driver) |
| device | Ambiguous: Linux `/dev`, HA devices, the Pi itself | **machine** (the computer), **part** (the hardware), **port** (the interface) |
| integration, add-on, plugin, module, driver | Each is used differently by HA, Viam, Node-RED and others | **connector** or **app** |
| bus (for internal messaging) | Clashes with I2C/SPI buses | **router**, **signal** |
| entity | HA's string-state model, which we deliberately don't copy | **signal** |
