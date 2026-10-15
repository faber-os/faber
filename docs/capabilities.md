# Capabilities

Status: **v1 draft** (2026-10-15). All 41 capabilities are specified here. Each one gets
implemented when its first connector is written. The WIT sketches are in
[`wit/capabilities/`](../wit/capabilities/). The terms are defined in [glossary.md](glossary.md).

## Common rules

1. **Timestamps.** Every signal carries `t-us`: microseconds on the core's monotonic clock.
   Connectors and bridges stamp in their own clock, and the core converts. The core also publishes
   the mapping to wall-clock time.
2. **Units.** SI and radians everywhere, and the UI shows friendly units (degrees, km/h, L/min,
   kg). The exceptions follow ROS: temperature in **°C**, relative humidity as a **fraction 0–1**.
3. **Frames.** Machine frame as in ROS REP-103: **x forward, y left, z up**. Every spatial part has
   a `mount` setting (x, y, z in m; roll, pitch, yaw in rad) relative to the machine. The shell's
   **top view** places every spatial part where it really sits.
4. **Info.** Facts a part reports about itself once (range limits, field of view, pixel count,
   supported modes) are its **info**, not repeated in every reading.
5. **Missing values** use `option<…>`. The one exception is inside large arrays, where `+inf`
   means "no return" (ROS convention, used in `laser-scan`).
6. **Uncertainty** is a simple variance per axis, not a full covariance matrix.
7. **Delivery.** **L** = latest-value: a slow subscriber gets the newest reading and skips the
   rest. **E** = every-message: events, where none may be lost.
8. **Named instances.** A part may provide the same capability more than once (a PMS5003 gives three
   `concentration`s). Each one is named in the connector manifest (`[[provides]] name = "pm2p5"`).
   The address is `part/name`, and the name defaults to the capability's name.
9. **Command kinds.** 🔒 **leased**: anything that can hurt something (see [the lease](#the-lease)).
   **Shared**: harmless commands where the last writer wins (lights, displays, tare, reset counters).
   **Always allowed**: `estop.engage`.
10. **Built from.** A part's info may say it is *built from* other parts (SENNEP's `planar-base` is
    built from its 4 `motor`s). A lease on it locks those parts too.

## The lease

A **lease** is an exclusive, time-limited right for **one holder** to command **one actuator
part**, plus any parts it is built from. It works as both a *talking stick* (only one holder at a
time) and a *dead man's switch* (if the holder goes quiet, the part makes itself safe).

```
request ──► granted (lease id + deadline)        or  denied ("held by sennep")
   │
   ├─ every command renews it          (a joystick at 20 Hz keeps it alive by itself)
   ├─ renew()                          (hold a position without sending commands)
   │
   └─ ends in one of four ways ──► the part goes to its SAFE STATE
        release()  – the holder is done
        expire     – the deadline passed without renewal
        revoke     – e-stop, an update, or an operator takes over
        part fails – the connector dies, or the bridge loses its heartbeat
```

- **Holders:** apps that declared `command = true` for the role, and the shell's manual-control
  windows (users with the *operator* role).
- **Duration:** each capability has a default (below). A part setting may change it within the
  capability's limits. Every lease has a deadline. There are no permanent leases.
- **Enforced by the core.** The router drops any command without a valid lease id.
- **Operators can always take over** an app's lease. The app receives a `revoked` event.
- **A second layer on the bridge:** the microcontroller stops its own actuators when commands or
  the heartbeat stop arriving, even if the Pi is completely frozen.

## Overview

| Group | Capabilities |
|---|---|
| GPIO level | `digital-in`, `digital-out` 🔒, `analog-in`, `pwm-out` 🔒, `servo` 🔒 |
| Environment | `temperature`, `humidity`, `pressure` |
| Ranging & motion | `distance`, `imu`, `laser-scan`, `position`, `pose` |
| Actuation | `motor` 🔒, `planar-base` 🔒, `light` |
| Power & control | `battery`, `rc-input`, `estop` |
| Media | `camera` |
| General inputs | `button`, `encoder`, `keypad`, `presence`, `illuminance`, `color-sensor`, `sound-level`, `concentration`, `soil-moisture`, `force`, `flow-rate`, `power-meter`, `magnetic-field`, `tag-reader`, `wind`, `counter` |
| General outputs | `tone`, `text-display`, `pixel-display`, `ir-remote` |
| Fallback | `measurement` |

All are version `@1`.

## GPIO level

| Capability | Signal | Commands | Settings / info | Safe state · lease | Viewer |
|---|---|---|---|---|---|
| `digital-in` | `level: bool` (**E**) | — | pull (none/up/down), debounce-ms, active-low | — | LED + edge log |
| `digital-out` | `level: bool` (L) | `set(level)` 🔒 | active-low, safe-level (default *inactive*) | safe-level · 1 s (max 60 s) | switch |
| `analog-in` | `fraction` 0–1, `volts?` (L) | — | rate-hz, reference-volts | — | plot |
| `pwm-out` | `duty` 0–1 (L) | `set-duty` 🔒 | frequency-hz | duty 0 · 1 s | slider |
| `servo` | `angle-rad` as commanded (L) | `set-angle` 🔒 | min/max angle, pulse min/max µs, max speed, safe-state (*hold* / release, default hold) | hold or release · 1 s | dial |

## Environment

| Capability | Signal (L) | Settings | Based on |
|---|---|---|---|
| `temperature` | `celsius`, `variance?` | rate-hz | ROS Temperature |
| `humidity` | `relative` 0–1, `variance?` | rate-hz | ROS RelativeHumidity |
| `pressure` | `pascal`, `variance?` | rate-hz | ROS FluidPressure |

Viewer for all three: the value plus a plot.

## Ranging and motion

**`distance`** (ROS Range)
- **Signal (L):** `meters`, `status` (ok / too-close / too-far / no-target / error).
- **Info:** min-m, max-m, field-of-view-rad, kind (ultrasonic / laser-tof / infrared / radar).
- **Settings:** rate-hz, mount.
- **Viewer:** a bar, a plot, and a cone in the top view.

**`imu`** (ROS Imu)
- **Signal (L):** `orientation?` (quaternion, absent for IMUs without fusion),
  `angular-velocity` rad/s (vec3), `linear-acceleration` m/s² (vec3), and an optional variance
  (vec3) for each.
- **Settings:** mount, rate-hz.
- **Viewer:** a 3D orientation plus plots.

**`laser-scan`** (ROS LaserScan + raw points)
- **Signal (L):** `angle-min`, `angle-increment`, `angles?`, `time-increment?`, `scan-time` s,
  `ranges[]` m (`+inf` = no return), `intensities[]?` (0–1).
- **Even bins or raw points:** without `angles`, point *i* lies at
  `angle-min + i × angle-increment` (even bins). With `angles`, each point has its own angle (raw
  points, as the LD19 sends them).
- **Info:** range-min, range-max.
- **Settings:** mount, rate-hz.
- **Viewer:** a polar plot inside the top view.

**`position`** (ROS NavSatFix)
- **Signal (L):** `latitude-deg`, `longitude-deg`, `altitude-m?`,
  `fix` (none / fix2d / fix3d / dgps / rtk-float / rtk-fixed, as the WIT names them), `satellites`,
  `horizontal-accuracy-m?`,
  `vertical-accuracy-m?`, `speed-m-s?`, `course-rad?`.
- **Viewer:** coordinates and fix quality, plus a map when the machine is online.

**`pose`** (ROS PoseWithCovariance)
- **Signal (L):** `frame` ("map" / "odom" / …), `position` (vec3), `orientation` (quaternion),
  `position-variance?` (vec3), `yaw-variance?`.
- Usually published as a **derived signal** (e.g. `sennep:pose`).
- **Viewer:** the position and a trail in the top view.

## Actuation

**`motor`** 🔒 (lease 250 ms)
- **Signal (L):** `velocity-rad-s?`, `position-rad?`, `current-a?`.
- **Commands:** `set-velocity`, `set-position`, `set-duty` (open loop, −1 to 1). The info says
  which ones the part supports.
- **Info:** kind (dc / stepper / bldc / continuous-servo), modes, max-velocity.
- **Settings:** stop-mode (brake / coast).
- **Safe state:** velocity 0 / duty 0.

**`planar-base`** 🔒 (lease 250 ms)
- **Signal (L):** `commanded` and `measured?` velocity (twist2d: vx, vy m/s, ω rad/s).
- **Command:** `set-velocity(twist2d)` in the machine frame.
- **Info:** holonomic, max vx/vy/ω, max acceleration, built-from.
- **Safe state:** stop, with the ramp-down done on the microcontroller.

**`light`** (shared)
- **Signal (L):** current colours.
- **Commands:** `set-all(rgb, brightness)`, `set-pixels(start, [rgb])`, `set-effect(name)`.
- **Info:** pixel-count (1 = a single RGB LED), effects (those the microcontroller supports),
  max-brightness.
- **Safe state:** none (keeps its state).

## Power and control

**`battery`** (ROS BatteryState)
- **Signal (L):** `volts`, `amps?` (negative = discharging), `charge?` 0–1, `celsius?`,
  `cell-volts[]?`, `level` (ok / low / critical).
- The core raises a notification whenever `level` changes.
- **Info:** chemistry, cells-in-series, capacity-ah.

**`rc-input`** (ROS Joy)
- **Signal (L):** `axes[]` −1 to 1, `buttons[]`, `connected`, `link-quality?` 0–1.
- **Info:** axis and button labels.
- Informational on the Pi. Radio overrides for safety stay on the microcontroller.

**`estop`**
- **Signal (E):** `engaged`, `source` (part / app / shell), `reason`.
- **Commands:** `engage()` (always allowed) and `release()` (operator role only, and not while a
  hardware e-stop is latched).
- **Info:** hardware-latching.
- When engaged, the core revokes every lease.

## Media

**`camera`** (minimal in v1)
- **Signal (L, about 1 Hz):** `width`, `height`, `format`, `fps`.
- The video itself never goes through the router. The core serves it as a stream to the shell.
- **Settings:** resolution, fps, exposure (auto / manual).
- Detections from the AI camera become their own capability later.

## General inputs

| Capability | Signal | Commands (shared) | Settings / info | Typical parts |
|---|---|---|---|---|
| `button` | `event` (press / release / long-press / double-press), `pressed` (**E**) | — | long-press-ms, double-press-ms, debounce-ms | push buttons, TTP223, MPR121 |
| `encoder` | `count`, `delta`, `velocity` (counts/s, or rad/s when counts-per-rev is known) (L) | `reset-count` | info: counts-per-rev | rotary knobs, wheel encoders |
| `keypad` | `key`, `pressed` (**E**) | — | info: layout | 4×4 matrix keypads |
| `presence` | `detected`, `distance-m?` (**E** on change) | — | hold-time-ms | PIR, mmWave radar |
| `illuminance` | `lux` (L) | — | rate-hz | LDR, BH1750, TSL2561 |
| `color-sensor` | `r`, `g`, `b`, `clear` 0–1, `color-temperature-k?` (L) | — | gain, integration time | TCS34725 |
| `sound-level` | `decibels` (L) | — | info: calibrated (dB SPL) or relative | microphone modules |
| `concentration` | `value` (L) | — | info: substance (co2 / co / ch4 / lpg / alcohol / voc-index / pm1 / pm2p5 / pm10 / o3 / no2 / nh3 / …), unit (ppm / ppb / ug-per-m3 / index), calibrated | SCD40, MQ-x, SGP30, PMS5003 |
| `soil-moisture` | `fraction` 0–1 (L) | — | calibration (dry / wet raw values) | capacitive soil sensors |
| `force` | `newtons` (L) | `tare` | calibration factor | HX711 load cells, FSRs |
| `flow-rate` | `m3-per-s`, `total-m3` (L) | `reset-total` | pulses per litre | YF-S201 |
| `power-meter` | `volts`, `amps`, `watts`, `watt-hours` (L) | `reset-energy` | shunt ohms | INA219, INA226, ACS712 |
| `magnetic-field` | `tesla` (vec3), `heading-rad?` (L) | — | mount, declination | HMC5883L, QMC5883L |
| `tag-reader` | `tag-id`, `kind` (**E**) | — | info: kinds (rfid / nfc / barcode / qr) | RC522, PN532, scanners |
| `wind` | `speed-m-s`, `direction-rad?` (L) | — | calibration | anemometer + wind vane |
| `counter` | `count`, `rate-per-s` (L) | `reset` | — | rain gauges, Geiger counters, pulse outputs |

## General outputs (all shared)

| Capability | Signal | Commands | Info | Typical parts |
|---|---|---|---|---|
| `tone` | `playing`, `frequency-hz?` (L) | `play(frequency-hz, duration-ms)`, `stop` | — | buzzers, small speakers |
| `text-display` | current lines (L) | `set-line(row, text)`, `clear`, `set-backlight` | rows, columns | 16×2 / 20×4 LCDs, 7-segment |
| `pixel-display` | — | `clear`, `draw([ops])` (text / line / rect / image), `flush` | width, height, colour depth | SSD1306, e-paper, LED matrices |
| `ir-remote` | received `protocol`, `code` (**E**) | `send(protocol, code)` | can-send, can-receive | IR receivers and LEDs |

Outputs that are already covered elsewhere:
- relays, solenoids, valves → `digital-out`
- vibration motors, fans, single LEDs → `pwm-out`
- RGB LEDs → `light` with 1 pixel
- steppers → `motor` with kind `stepper`

## Fallback

**`measurement`**
- **Signal (L):** `value`.
- **Info:** `quantity` (a label) and `unit` (a [UCUM](https://ucum.org) string such as `"mV"` or `"Hz"`).
- Viewers and plots work automatically, but apps can't rely on what the number means.
- **Registry rule:** use the proper capability when one exists. `measurement` is for the long tail.

## Not capabilities (on purpose)

- **Real-time clock:** system time, which belongs to the core.
- **Bluetooth, LoRa, Wi-Fi modules:** ways to connect, not things you measure. They become ports
  or connectors.
- **Fingerprint and other biometric sensors:** sensitive data. Later, if ever.

## How SENNEP maps onto these

| SENNEP hardware | Capability |
|---|---|
| LD19 LiDAR | `laser-scan` (raw points with `angles`) |
| 4 mecanum motors | 4 × `motor` |
| Drive base (kinematics on the Mega) | `planar-base`, built from the 4 motors |
| 8 × VL53L0X | 8 × `distance` (laser-tof) |
| 4 × HC-SR04 | 4 × `distance` (ultrasonic) |
| IMU | `imu` |
| NRF24 radio joystick | `rc-input` |
| 3S6P battery | `battery` |
| WS2812B strip | `light` |
| 3 W headlights, IR dock beacon | `pwm-out` |
| NEO-6M GPS | `position` |
| AI Camera | `camera` |
| Arm button | `estop` (released = armed) |
| Localisation | `sennep:pose` (derived signal) |
