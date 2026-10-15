# Phase 2 design: drive before you wire

Status: **design v1** (2026-10-15), from D12, D36, D37 and D46. This is the contract the Phase 2
code is built against. [roadmap.md](roadmap.md) says what "done" means; this says how.

## 1. What a user does

1. Add parts as usual, but tick **Simulated**. The part gets the same name and capability it would
   have for real (`lidar` stays `lidar/laser-scan`); only its source changes. Switching to the
   real part later is one line in `parts.toml`, and apps never notice (D36).
2. Pick a **world** (Settings is not the place: the world belongs to the machine config, so it is
   in `parts.toml`, and the World window has a picker).
3. Open **Drive**. Faber asks once, "Start driving sennep?" (D46). The on-screen joystick (or
   WASD/arrow keys, Q/E to turn) holds a **lease** on the base while it is used.
4. Open **World** to watch the machine, the room, and the rays of its simulated sensors.
5. Press **Stop** (the red button on the taskbar, or the Pause/Break key) to engage the
   **e-stop**: every lease ends, everything goes to its safe state, and nothing moves until the
   e-stop is released.
6. Write a program (any language, plain HTTP) that takes the lease and drives; see
   `examples/drive-square.py`.

## 2. Capabilities added in Phase 2

All follow [capabilities.md](capabilities.md); this phase implements four of them in
`faber-capabilities` (Rust types, TypeScript export, WIT sketches already exist):

| Capability | Signal | Commands (🔒 leased) | Safe state | Lease |
|---|---|---|---|---|
| `distance@1` | `meters`, `status` | — | — | — |
| `motor@1` | `velocityRadS?`, `positionRad?`, `currentA?` | `set-velocity`, `set-duty` 🔒 | velocity 0 | 250 ms |
| `planar-base@1` | `commanded`, `measured?` (twist2d `vx`, `vy`, `omega`) | `set-velocity` 🔒 | stop | 250 ms |
| `estop@1` | `engaged`, `source`, `reason` (**E**) | — (machine-level API) | — | — |

Commands are a tagged enum like readings: `{"capability": "planar-base@1", "command": {"kind":
"set-velocity", "vx": 0.2, "vy": 0, "omega": 0}}`. The core checks every command against the
part's info (limits) and **clamps** to it, reporting the clamp in the reply, rather than refusing a
joystick that overshoots.

## 3. Generic parts

Motors and bases have no USB connector until FaberLink (Phase 4). So the core gains built-in
**generic** connector definitions, usable simulated now and backed by a bridge later:

- `generic-planar-base`: settings `holonomic` (default true), `max-vx`, `max-vy`, `max-omega`,
  `max-accel`; footprint `length` × `width` (m).
- `generic-motor`: settings `max-velocity`, `kind`.
- `generic-distance`: settings `min-m`, `max-m`, `fov-rad`, `kind` (ultrasonic / laser-tof /
  infrared).
- Existing real connectors (`ldrobot-ld19`) can be simulated too: their manifest gives the
  capability and info (range limits), the world gives the values.

A generic part without `simulate` is refused with a hint ("needs a bridge, Phase 4; tick
Simulated for now").

## 4. Simulated parts in parts.toml

```toml
[world]
name = "apartment"            # one of the prebuilt worlds (section 7)
start = { x = 1.0, y = 1.0, yaw = 0.0 }

[parts.base]
connector = "generic-planar-base"
simulate = "world"            # instead of `port`
settings = { length = 0.8, width = 0.65, max-vx = 0.6, max-vy = 0.6, max-omega = 1.5 }

[parts.lidar]
connector = "ldrobot-ld19"
simulate = "world"
mount = { x = 0.2, z = 0.3 }

[parts.front]
connector = "generic-distance"
simulate = "manual"           # a value you set by hand, or a pattern
```

- `simulate = "world"`: values come from the world (raycasts from the part's `mount`; the base
  moves the machine's pose).
- `simulate = "manual"`: values come from a slider or a pattern (`steady`, `sine`, `noise`,
  `step`) set through the API; actuators accept commands and report them back as their state.
- `port` and `simulate` are mutually exclusive (config error with the line). A simulated part
  claims no port.
- At most one `planar-base` may be simulated in the world: it is the machine.

## 5. Leases

The core module `leases` owns them (see capabilities.md, "The lease"):

- `POST /api/v0/parts/{name}/lease` with `{"holder": "Drive window"}` → `201 {lease, deadlineUs}`,
  or `409 lease-held` with who holds it, or `423 estop-engaged`.
- Every accepted command renews the lease. `POST …/lease/renew` renews without a command.
- `DELETE /api/v0/parts/{name}/lease?lease=…` releases it.
- Ends: release, expiry (deadline passed), revoke (e-stop, an operator takeover with
  `{"takeOver": true}`, the part being removed or restarted), or the part failing.
- When a lease ends for any reason the core sends the part its **safe state** and announces
  `lease-ended {part, reason}` on the `leases` WebSocket topic, which also carries
  `lease-granted` so every screen shows who drives what.
- Built-from: a lease on a `planar-base` built from motors also locks those motors.
- Timing comes from the core's monotonic clock; a sweeper ends expired leases within 50 ms.

## 6. Commands and the e-stop

- `POST /api/v0/parts/{name}/command` with `{"lease": …, "command": …}` → `200 {applied}` (the
  command after clamping), `403 lease-required` / `lease-ended`, `423 estop-engaged`,
  `400 bad-command` (wrong capability or kind for this part).
- `POST /api/v0/estop` with `{"reason": …}` engages it: always allowed, from anyone. Every lease
  is revoked, every actuator gets its safe state, and new leases are refused (`423`).
- `POST /api/v0/estop/release` releases it. In Phase 6 only operators may; until then the shell
  asks to confirm.
- `GET /api/v0/estop` and the `estop` WebSocket topic (every-message) report its state.
- Real actuators get commands through the connector trait (`Connector::command`, default "not
  supported"); the first real use is the FaberLink bridge in Phase 4.

## 7. The world

2D and kinematic only (D36): no forces, no wheel slip.

- **Geometry:** walls and furniture as line segments and polygons (metres, world frame x east,
  y north), plus **moving obstacles** that follow a looping path at a walking speed.
- **The machine:** a rectangle (the base's footprint) at a pose `(x, y, yaw)`. The base integrates
  its commanded twist at 50 Hz, limited by `max-accel`. If the next step would overlap anything,
  the machine stops short and the base reports `bumped` in its state (the World window shows it).
- **Sensors:** `laser-scan` raycasts from its mount at the connector's resolution (LD19: 0.8°,
  10 Hz, range 0.02–12 m) with a little noise; `distance` casts a cone of rays and reports the
  nearest, with `too-far` / `too-close` from its info.
- **Prebuilt worlds**, built in as data files: `empty` (a 10 × 10 m floor), `apartment`,
  `office`, `warehouse`, `maze`, `obstacle-course` (with a person walking a loop). Each has a
  start pose and a description.
- **API:** `GET /api/v0/worlds` (the list), `GET /api/v0/world` (the current world's geometry),
  `PUT /api/v0/world` `{name, start?}` (writes `[world]` in parts.toml), `POST /api/v0/world/reset`
  (machine back to the start pose), and the `world` WebSocket topic: the machine's pose and the
  moving obstacles, 20 times a second.
- The pose is also published as a signal: `world/pose` (`pose@1`), the ground truth, so a
  program can compare its own estimate with it.

### Adding a world

A world is a TOML file in `core/src/sim/worlds/`, listed in `FILES` in that folder's `mod.rs`
(the binary includes it). Metres and radians, world frame (x east, y north):

```toml
name = "garage"                       # must match its FILES entry
title = "Garage"
description = "A 6 × 3 m garage: a workshop behind a doorway, a workbench and a stool."
start = { x = 1.0, y = 1.5, yaw = 0.0 }
bounds = { min = [0.0, 0.0], max = [6.0, 3.0] }
# open = true                         # only if rays may escape (an open floor)

[[outline]]                           # walls through these points, closed by default
points = [[0.0, 0.0], [6.0, 0.0], [6.0, 3.0], [0.0, 3.0]]

[[wall]]                              # a straight wall; doors = [[offset from `from`, width]]
from = [3.0, 0.0]
to = [3.0, 3.0]
doors = [[1.0, 0.9]]

[[box]]                               # a solid rectangle: centre, [length, width], yaw
label = "workbench"
at = [5.6, 1.5]
size = [0.6, 2.0]

[[legs]]                              # a table or chair: only its four legs are solid
label = "stool"
at = [4.8, 1.5]
size = [0.35, 0.35]
leg = 0.03

[[post]]                              # round: a bin, a pillar
at = [0.5, 2.5]
radius = 0.2

[[mover]]                             # walks its loop forever, positioned by time
label = "person"
radius = 0.25
speed = 1.0
path = [[3.6, 0.5], [4.8, 0.5]]
```

`[[polygon]]` (`points`) takes any other solid shape. The tests in `worlds/tests.rs` check a new
world like every other: it parses, rays from the start never escape (unless `open`), SENNEP's
0.8 × 0.65 m footprint is free at the start and can drive more than 3 m, and no mover walks
through anything. The world is 2D, so every sensor sees everything solid whatever its height:
model what a sensor at about 0.3 m would hit (table legs, not table tops).

## 8. Where it lives in the core

| Module | Layer | What |
|---|---|---|
| `leases` | 2 | lease table, deadlines, built-from locks (pure; clock passed in) |
| `estop` | 2 | e-stop state and its history |
| `sim::world` | 2 | geometry, raycasting, collision (pure, no threads) |
| `sim::worlds` | 2 | the prebuilt worlds (data) |
| `simparts` | 3 | the world engine: world parts as runtime sources: tick, publish, accept commands |
| `machine` | 4 | wires it all: config, leases, commands, e-stop, world (`[world]` in parts.toml) |
| `api` | 5 | the endpoints and topics above |

## 9. Shell

- **Add part:** a "Simulated" switch (and the generic connectors in the list, which can only be
  simulated for now).
- **Drive** (tool): joystick pad, keys, speed limit slider, lease status ("You're driving base"),
  the D46 confirm once per session, and a big Stop.
- **World** (tool): top view of the world, the machine, laser points, distance cones, moving
  obstacles; world picker and Reset position.
- **Taskbar:** a red **Stop** button whenever a part that can move exists; it shows "Stopped" and
  a Release button when the e-stop is engaged. Pause/Break engages it from any window.
- **Parts:** simulated parts show a "Simulated" badge; manual ones get a value slider and a
  pattern menu.
- **Distance viewer:** a bar and a plot.
