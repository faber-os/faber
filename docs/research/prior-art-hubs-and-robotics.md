# Prior art: Home Assistant, ESPHome, Viam, ROS 2, Node-RED

Researched 2026-10-15 from official docs (linked). Details marked *(from memory)* were not checked
against a live source. The companion file [prior-art-appliance-os.md](prior-art-appliance-os.md)
covers CasaOS, ZimaOS, Umbrel, balena and DietPi.

## 1. Home Assistant (HA)

**What:** an open-source home-automation hub in Python, from the Open Home Foundation.

**Layers** ([architecture](https://developers.home-assistant.io/docs/architecture_index)):
- **HA OS:** a Buildroot Linux with a read-only SquashFS root, systemd and Docker.
- **Supervisor:** a container that manages the OS, Core and apps (updates, backups, network, hardware).
- **Core:** the Python program that does the automation.

Inside Core, an **integration** (once called a "component") provides **platforms**, one per entity
domain (`light`, `sensor`). Platforms create **entities** (`sensor.kitchen_temp`: string state +
attributes), which are grouped under **devices**. A **config entry** is one configured integration
instance, made by a UI "config flow". On top of that come automations, scripts, scenes, dashboards
and Blueprints (automation templates).

**Add-ons → Apps:** release 2026.2 renamed add-ons (containers running beside HA) to **Apps**
([release](https://www.home-assistant.io/blog/2026/02/04/release-20262/),
[rationale](https://frenck.dev/renaming-home-assistant-add-ons-to-apps/)). **HACS** is a separate
community store for custom integrations and cards, installed from GitHub repos.

**Why the terms confuse people:**
- Hass.io was split into OS, Supervisor and "Supervised".
- "Component" became "integration", but "platform" still means both a Python file and an entity domain.
- Both "add-on" and "integration" sounded like "a thing you add", so users looked in the wrong place.
  Some products, like Z-Wave JS, exist as both.
- There are four install types, and apps exist on only two of them.

| Extension kind | Language | Isolation | Distribution |
|---|---|---|---|
| Integration | Python, inside Core | **none**: it can crash or block Core | core repo, HACS, `custom_components/` |
| App | anything (Docker) | container plus declared `uart`/`gpio`/`usb`/`privileged`, AppArmor | git "app repositories" |

**Config:** HA started fully in YAML. Since ADR-0010 (2020), device integrations are configured
through the UI only ([ADR-0010](https://github.com/home-assistant/architecture/blob/master/adr/0010-integration-configuration.md)).
The result is a lasting split between YAML users and UI users.

**Updates:** RAUC with A/B slots and rollback *(A/B and rollback from memory)*. Core, Supervisor and
apps are container images.

**Gaps for us:**
- custom code has no sandbox;
- no real-time path;
- state is a string, with no LaserScan or pose types and no high-rate streams;
- hardware access is just "let this container see `/dev/ttyUSB0`", with no arbitration between users of a bus.

## 2. ESPHome

**What:** YAML → firmware for ESP32, ESP8266 and RP2040, mainly to make sensors for HA.

**How:**
- A top-level key (`i2c:`) loads a **component**, and `sensor: - platform: bme280` loads a
  **platform** ([dev docs](https://developers.esphome.io/architecture/overview/)).
- Each one has a Python `CONFIG_SCHEMA` (validated) and a `to_code()` that emits C++, built with PlatformIO.
- At runtime, C++ classes implement `setup()`/`loop()`/`update()`.
- A sensor is pure YAML: platform, bus address, interval, plus **filters** (moving average, calibration, throttle).

**Extensions:** *external components* come from git or a local folder and have the same structure as
built-ins. They are compiled in, with no sandbox ([docs](https://esphome.io/components/external_components/)).

**Why it works:** one declarative file describes the whole device. Users never write C++, and the
schema catches mistakes early. It is the best "part → typed capability" UX in the hobby world.

**Pain points:**
- slow compiles;
- complex logic ends up as `lambda:` C++ inside YAML;
- microcontrollers only;
- tied to HA.

## 3. Viam (closest prior art)

**What:** a commercial robotics platform. The on-device server (RDK, Go) is open source under
**AGPL-3.0** ([GitHub](https://github.com/viamrobotics/rdk)). The cloud app (config, fleet, data, ML)
is proprietary and usage-priced, with the first $5/month free ([pricing](https://www.viam.com/pricing)).

**Terms** ([glossary](https://docs.viam.com/reference/glossary/)):
- a **resource** is a **component** (hardware: camera, motor, base, arm, board, sensor…) or a
  **service** (SLAM, vision, motion planning);
- each has a typed **API** in protobuf/gRPC (`rdk:component:motor`);
- a **model** is one implementation of an API;
- a **machine** is made of **parts** (one computer running `viam-server` plus its hardware);
- **fragments** are reusable config blocks.

**Modules** ([reference](https://docs.viam.com/build-modules/module-reference/)):
- Each module is a child process speaking gRPC over a Unix socket.
- Lifecycle: `Ready` → `Validate` (returns dependencies) → `AddResource`.
- A config change rebuilds the resource, and a crash restarts it every 5 s.
- Modules are written in Go, Python or C++ and built per platform.
- The registry offers public, private and unlisted modules.
- **No sandbox**: a module has the server's privileges.

**Config:** JSON, edited in the cloud app and pulled by the machine. It runs from a cached copy for
up to 60 days offline. A normal setup needs an account.

**Overlap with Faber:**
- typed capability APIs;
- out-of-process drivers;
- a registry;
- declared dependencies;
- an update agent.

**Differences (our opening):**
- Viam is cloud-first;
- it has no OS image, no maker tools and no hardware broker (modules open `/dev` themselves);
- it has no sandbox;
- AGPL plus a paid cloud.

## 4. ROS 2 (contrast)

- **Nodes** publish/subscribe on **topics**, or use **services** (request/reply) and **actions**
  (long-running goals with feedback).
- **Message types** live in `.msg` files (`sensor_msgs/LaserScan`, `sensor_msgs/Imu`,
  `geometry_msgs/Twist`), and they are the de facto robotics data vocabulary.
- Packages are built with colcon and shipped through apt.
- There is no sandbox, no device UI and no updater.
- Pain points: a heavy toolchain, Ubuntu-centric (awkward on Raspberry Pi OS), DDS networking
  trouble, and a steep learning curve ([concepts](https://docs.ros.org/en/rolling/Concepts.html)).

**Lesson:** copy the message *shapes*, not the toolchain.

## 5. Node-RED (contrast)

- **Nodes** wired into **flows** (`flows.json`), passing untyped `msg.payload` objects.
- Extra nodes are npm packages installed through the Palette Manager
  ([concepts](https://nodered.org/docs/user-guide/concepts)).
- Strengths: wiring is instant to learn, and the community is huge.
- Weaknesses: untyped messages, one process (one bad node takes everything down), version drift
  between packages, and flows get messy at scale.

## Lessons for Faber

**Do**
1. **Fix the vocabulary on day one.** One word, one meaning, everywhere: UI, docs and code. See [glossary.md](../glossary.md).
2. **Typed, versioned capabilities** (`laser-scan@1`), shaped after ROS `sensor_msgs` so a ROS bridge stays possible.
3. **ESPHome-style declarative parts:** a schema-validated config block per part. Validate before starting anything.
4. **Viam-style lifecycle:** validate → resolve dependencies → start, with restart and backoff, and visible health for each part. Reconfigure by rebuilding.
5. **Third-party code sandboxed, with hardware grants the core enforces** and arbitration between users of a bus. None of the five platforms does this.
6. **Files are the source of truth, and the UI edits those same files.** That avoids HA's YAML-vs-UI split.
7. **HAOS-style updates:** a read-only image, A/B slots with rollback, and packages updated separately from the OS.
8. **Local-first.** The cloud is optional, and remote access goes through Tailscale.

**Don't**
1. Run untrusted code inside the core process (HA custom integrations, Node-RED).
2. Use string-typed state for robot data.
3. Ship more than one install variant.
4. Let the store mean "any repo URL" with no review. Pin versions, use checksums, and show permissions before install.
5. Make logic-in-config the escape hatch (ESPHome lambdas, function nodes). Give apps a real SDK.
6. Use a licence that scares off connector authors (AGPL plus a proprietary cloud).
