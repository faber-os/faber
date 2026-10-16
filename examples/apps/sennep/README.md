# SENNEP: drive SENNEP V2 by hand, safely

The Faber **app** for SENNEP V2, a holonomic (mecanum) robot of 800 × 650 mm with an LD19 LiDAR,
a distance sensor at each corner and a 3S Li-ion pack. Its window shows what the LiDAR sees and
drives SENNEP with a joystick or the keyboard. Its logic runs in the background and stops SENNEP
before it drives into something it can see.

> **Where it lives.** Here, in Faber's examples, for now, so Faber's CI tests it. It moves to the
> SENNEP V2 repo once Mikkel OKs it (docs/apps.md §7, question 2).

## Roles

| Role | Capability | Parts | What it does with it |
|---|---|---|---|
| `lidar` | `laser-scan@1` | one | The top view, and what the safety layer looks out for. |
| `base` | `planar-base@1`, `command = true` | one | Drives it, holding its lease only while you drive. |
| `proximity` | `distance@1` | any number, optional | More eyes for the safety layer (low or close things the LiDAR misses); the ring in the window. |
| `battery` | `battery@1` | one, optional | The gauge; a critical battery caps the speed. |

It works without proximity sensors or a battery. It reads where each part sits on SENNEP from its
**mount** in Parts (x forward, y left, yaw), so set those right: the LiDAR's position decides where
the walls are, and a proximity sensor's yaw decides which way it looks.

It publishes two derived signals, which you can watch like any sensor:

- `app:sennep/pose` (`pose@1`): where SENNEP is, by **dead reckoning** from the base's measured
  velocity (its commanded one when it doesn't measure), in a frame called `odom` that starts where
  SENNEP was when the app started. **It drifts:** wheel slip, bumps and small speed errors add up
  and are never corrected. In the simulated world, `world/pose` is the true pose to compare it with.
  The window's Reset pose puts it back to 0.
- `app:sennep/nearest-obstacle` (`distance@1`): the gap from SENNEP's leading edge to the nearest
  thing in the direction it drives (or drove last), from the LiDAR and the proximity sensors.
  `no-target` when nothing is within 12 m.

## The safety layer, and its limits

Every command the app sends goes through it, whether the window is open or not:

- **Stop distance** (default 0.35 m, the window's slider): it never drives SENNEP's leading edge
  closer than this to anything the LiDAR or a proximity sensor sees. It slows down on the way
  (at most 1.5 m/s per metre left), so it stops smoothly. Each direction counts on its own: with a
  wall on the left, it may still drive ahead along it, just not further left.
- **Turning** on the spot is refused while something is within 5 cm of the circle SENNEP's corners
  sweep.
- **Battery**: when the battery's level is critical (below 9.9 V for a 3S Li-ion pack, D48), it
  drives at 30 % of the top speed and says so.
- **No LiDAR, no driving**: without a new scan in the last second it won't move.
- **The window's dead man's switch**: the window repeats what you ask every 100 ms, and its page
  beats every 100 ms while it drives; if either stops (closed, crashed, the browser hung), the base
  stops within 300 ms. Faber itself ends the app's lease when the beat stops, when you press its
  Stop bar or close the window: the base is declared `drive = "window"`, so the app may only drive
  it while its window does.

It **only limits this app's own commands.** It doesn't touch the core's e-stop, which always
works, nor anything else driving the base (the Drive window, another app, a program). And it only
sees what its sensors see: something lower than the LiDAR's plane and outside every proximity
sensor's cone, a glass door, or a drop (stairs) is invisible to it. It is a helper for driving by
hand, not a certified safety system: keep the e-stop in reach.

## Try it in the simulated apartment

No robot needed.

1. **Build and pack it:** in this folder, `faber pack`. It builds `src/lib.rs` for WebAssembly
   and writes `sennep-0.1.0.faberpkg`.
2. **Simulate SENNEP:** pick the apartment in the World window, then add these parts (Parts → Add
   part, tick Simulated, or `POST /api/v0/parts`):
   - `base`: `generic-planar-base`, simulated in the world, `length` 0.8, `width` 0.65;
   - `lidar`: `ldrobot-ld19`, in the world, mount x = 0.2;
   - four `generic-distance` sensors in the world at the corners: `front_left` (x 0.4, y 0.325,
     yaw 45°), `front_right` (0.4, −0.325, −45°), `rear_left` (−0.4, 0.325, 135°), `rear_right`
     (−0.4, −0.325, −135°);
   - `pack`: `generic-battery` (3S Li-ion by default), in the world: it drains as SENNEP drives.
3. **Install it:** double-click the package in Files. Pick the parts for each role (the window
   suggests the only one that fits; tick the four sensors for `proximity`), tick "Turn it on now"
   and Install.
4. **Open its window** (Apps → SENNEP → Open, or the start menu) and drive: **W A S D** or the
   arrows (or drag the joystick) move it, **Q E** turn, **Space** or Stop stops. Click into the
   window first, so it gets the keys. The first time, Faber asks whether SENNEP may drive the base;
   while it drives, Faber shows "SENNEP is driving base." above the window, with a Stop.

Drive at a wall: SENNEP slows down and stops 0.35 m short of it, the joystick's edge and the stop
zone turn red, and the window says "Stopped: something 0.36 m ahead." To see the speed capped, add
a manual `generic-battery`, set it below 9.9 V (its Manual pane), and pick it for the `battery`
role in Apps.

## Its settings and its messages

The window and the logic share the app's **state**, its saved settings
(`PUT /api/v0/apps/sennep/state`):

| Key | Default | What |
|---|---|---|
| `max-speed` | `0.3` | Top speed (m/s, 0.05–0.6). The window's slider. |
| `stop-distance` | `0.35` | Leading edge to obstacle (m, 0.1–1.5). The window's slider. |
| `max-turn` | `0.8` | Top turn rate (rad/s). |
| `length`, `width` | `0.8`, `0.65` | SENNEP's footprint (m), for the stop distance and the view. |

What changes all the time goes as live **messages** instead (docs/apps.md §12), never saved:

| Kind | From → to | What |
|---|---|---|
| `drive` | window → logic | What the driver asks for: `"<x> <y> <turn>"`, each −1..1, every 100 ms while a control is held. Only while the window drives the base through the logic (`faber.driveVia("base")`). |
| `reset-pose` | window → logic | Puts the pose back to 0. |
| `hello` | window → logic | A window opened: the logic answers with the limits. |
| `limits` | logic → windows | What limits the driving now (JSON), whenever it changes. |

## The code, explained

- `src/geometry.rs`: points in the machine frame (a scan's returns from where the LiDAR sits, a
  distance sensor's along its yaw) and how far SENNEP's outline can move in a direction before it
  touches a point.
- `src/safety.rs`: the safety layer, a plain function of the request, the points, the settings and
  the battery, so `cargo test` checks every rule without wasm or a core.
- `src/odometry.rs`: dead reckoning.
- `src/lib.rs`: the app. Each tick (20 a second) it reads the settings and its messages, integrates
  the pose, takes the newest scan and the proximity readings, limits the window's request, and
  sends it under the base's lease. It lets go of the lease when you stop driving, so the Drive window or
  another program can take over.
- `ui/`: the window, plain HTML and JavaScript. `view.js` holds its pure parts (tested by the
  SDK's tests, `sdk/js/shell/src/sennep-view.test.js`), `app.js` wires them to the
  [`@faber-os/shell`](../../../sdk/js/shell/README.md) SDK (`faber-shell.js`, copied in; don't edit
  it). Its text is plain English: Faber's own catalogue doesn't translate apps.

**Why the window drives through the logic** instead of with the SDK's `faber.drive()`: leases are
exclusive, so if the window held the base's lease the logic couldn't limit what it sends. Keeping
the stop in the logic means it works the same whoever asks to drive, and keeps working if the
window's page misbehaves. `faber.driveVia("base")` keeps what `faber.drive()` gives the user: Faber
asks before the first drive (D46) and shows its own "SENNEP is driving base." bar with a Stop the
app can't hide, and Faber, not the app, ends the drive when the window stops.

## What needs the real robot

- The LD19 on SENNEP, and its mount measured on the frame.
- The Mega's FaberLink firmware exposing a `planar-base` (with its own ramp-down) and the corner
  sensors; until then, the base can't be driven from Faber (docs/faberlink.md).
- The stop distance and slow-down rate checked on the floor: braking on mecanum wheels, the
  LD19's 10 Hz scan and the Pi's latency all add distance.
- The pose's drift measured on the real floor.
