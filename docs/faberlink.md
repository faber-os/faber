# Phase 4 design: FaberLink

Status: **design v1, proposed** (2026-10-15). Builds on D11, D12 and D22 (designed fresh, not
from SENNEP's early protocol). Nothing here is built yet.

FaberLink is the serial protocol between the core and a microcontroller (a **bridge**). The
microcontroller declares the parts it hosts; the `faber-bridge` connector turns them into normal
parts with normal capabilities; leases and the e-stop reach the motors; and the microcontroller
stops its own motors if the Pi goes quiet.

## 1. Goals

1. **Plug and play.** A board running FaberLink says what it has. Faber suggests adding it, and
   its motors and sensors appear as parts without any typing.
2. **Safe without Linux.** The microcontroller's own watchdog stops every actuator when the
   heartbeat stops, even if the Pi has frozen. That is the last line of defence (D12).
3. **Small.** Fits an Arduino Uno's 2 KB RAM for a few parts; no dynamic allocation.
4. **Robust on a noisy cable.** Every frame is checked; a bad frame is dropped, never acted on.

## 2. Framing

- **COBS** encoding with a `0x00` delimiter: a frame can be found again after any garbage.
- Frame: `type (u8) · seq (u8) · payload (≤ 250 B) · CRC-16/CCITT-FALSE (u16 LE)`.
- Little-endian, fixed-size integers, floats as IEEE-754 `f32`.
- Default 115 200 baud (works on every Arduino); the hello can negotiate 1 Mbaud on boards that
  support it.

## 3. Messages

| Type | Direction | Payload |
|---|---|---|
| `HELLO` | core → board | protocol version, the core's max frame size |
| `HELLO_ACK` | board → core | protocol version, board name, firmware name and version, heartbeat timeout (ms) |
| `DESCRIBE` / `PART` | core → board / board → core | one `PART` per hosted part: local id (u8), capability id (`motor@1`), name (`front-left`), info (limits) |
| `HEARTBEAT` | core → board | every 50 ms; carries the e-stop state |
| `STATUS` | board → core | every 100 ms: health, uptime, latched hardware e-stop, last fault |
| `READING` | board → core | local part id + the capability's reading (fixed layout per capability) |
| `COMMAND` | core → board | local part id + command (e.g. `set-velocity`); only sent while the core holds a lease |
| `APPLIED` | board → core | what the board actually did (after its own limits) |
| `SAFE` | core → board | put one part (or all) in its safe state now |
| `ESTOP` | either way | engage; the board may also raise it from a physical button |
| `LOG` | board → core | a short text line, shown in Logs under the bridge part |

## 4. Safety rules on the board (the library enforces them, not the sketch)

1. No `HEARTBEAT` for the timeout (default 250 ms): every actuator goes to its safe state.
   Recovery needs a new `COMMAND`, not just a heartbeat: the board never resumes motion by itself.
2. An actuator that gets no `COMMAND` for its lease time (250 ms for motors) stops by itself, so
   a lost lease on the Pi stops the motor even if `SAFE` is lost on the wire.
3. A latched hardware e-stop wins over everything until it is reset on the board **and** released
   in Faber.
4. Ramp-down happens on the board (planar-base safe state, capabilities.md).

## 5. The Arduino library

```cpp
#include <FaberLink.h>

FaberLink link(Serial);
FaberMotor left("left", 9, 8);           // PWM pin, direction pin
FaberDistance front("front", A0);        // e.g. an analog IR sensor

void setup() { link.begin(115200, "sennep-mega", "0.1.0"); }
void loop()  { link.poll(); }            // everything else is done by the library
```

- Built-in part types cover the common cases (DC motor with H-bridge, servo, analog/digital in,
  ultrasonic HC-SR04, VL53L0X, encoder); a custom part implements a small interface.
- Lives in its own repo (`faber-os/faberlink-arduino`) so the Arduino Library Manager can list it.
- Tests: the protocol core is plain C++ compiled and tested on the PC too (no hardware needed),
  and a Rust test drives the `faber-bridge` connector against that compiled library.

## 6. In the core

- `faber-bridge` connector: built-in Rust first (like the LD19), wasm later.
- A board's parts appear as **sub-parts** of the bridge part: `mega` → `mega/left`, `mega/front`,
  each with its own viewer and lease. They are listed in parts.toml under the bridge so their
  names and mounts can be set.
- The Arduino flasher tool (avrdude, bundled) installs the right firmware, so most people never
  open the Arduino IDE.

## 7. Open questions for Mikkel

1. **Sub-parts in parts.toml**: `[parts.mega.parts.left] mount = …` (nested), or flat parts that
   say `bridge = "mega"`? Recommendation: nested; the bridge owns them.
2. **Default heartbeat timeout** 250 ms: right for SENNEP's speed (0.6 m/s → stops within ~15 cm
   plus ramp)? Recommendation: yes, configurable per board within 100–1000 ms.
3. **Retrofit order**: SENNEP's Mega firmware is rewritten on FaberLink once the library exists
   (D22). Fine to do that inside the SENNEP V2 repo?
