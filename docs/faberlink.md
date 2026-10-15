# FaberLink

Status: **protocol v1 and the Arduino library built** (Phase 4a, 2026-10-15); the `faber-bridge`
connector that brings a board's parts into the core is Phase 4b. Builds on D11, D12 and D22
(designed fresh, not from SENNEP's early protocol).

FaberLink is the serial protocol between the core and a microcontroller (a **bridge**). The
microcontroller declares the parts it hosts; the `faber-bridge` connector turns them into normal
parts with normal capabilities; leases and the e-stop reach the motors; and the microcontroller
stops its own motors if the Pi goes quiet.

| Piece | Where | What |
|---|---|---|
| Core side (Rust) | `sdk/rust/faberlink` | framing, every message, the capability layouts, and the session state machine |
| Board side (C++) | `firmware/faberlink` | the Arduino library: a portable protocol core plus thin Arduino glue |
| Interop tests | `tools/faberlink-interop` | the Rust session against the C++ core, compiled on the PC |

This page is exact enough to write a third implementation: §2–§4 are the wire format, §5 the
rules a board must follow.

## 1. Goals

1. **Plug and play.** A board running FaberLink says what it has. Faber suggests adding it, and
   its motors and sensors appear as parts without any typing.
2. **Safe without Linux.** The microcontroller's own watchdog stops every actuator when the
   heartbeat stops, even if the Pi has frozen. That is the last line of defence (D12).
3. **Small.** Fits an Arduino Uno's 2 KB RAM for a few parts; no dynamic allocation (§7).
4. **Robust on a noisy cable.** Every frame is checked; a bad frame is dropped, never acted on.

## 2. Framing

```
on the wire:  COBS( type · seq · payload · crc_lo · crc_hi ) · 0x00
```

| Field | Size | Meaning |
|---|---|---|
| `type` | u8 | the message type (§3) |
| `seq` | u8 | per sender, +1 for every frame it sends, wrapping at 255 → 0 |
| `payload` | 0–250 B | the message's layout (§3, §4) |
| `crc` | u16 LE | **CRC-16/CCITT-FALSE** over type, seq and payload: polynomial `0x1021`, initial value `0xFFFF`, not reflected, no final XOR. The check value of ASCII `123456789` is `0x29B1`. |

- **COBS** (Consistent Overhead Byte Stuffing) removes every `0x00`, and one `0x00` ends the frame.
  A frame is at most 254 bytes before encoding, so its encoding is exactly one byte longer, and the
  longest frame on the wire is 256 bytes with the delimiter. A full 254-byte group at the very end
  is **not** followed by an empty group (standard COBS: `01 02 … FE` encodes as `FF 01 02 … FE`).
- A receiver drops (and counts) anything between two `0x00`s that isn't valid COBS, is shorter than
  4 bytes, is longer than the longest frame, or fails the CRC. It then looks for the next frame
  after the next `0x00`. Two `0x00`s in a row are an empty frame and are ignored; the core sends one
  `0x00` before its first `HELLO` to end any half frame the board may hold.
- **Missed frames** are counted from gaps in `seq`: a frame whose `seq` is `n` more than expected
  means `n` frames were lost. Duplicate or out-of-order frames don't happen on a serial line.
- **Numbers** are little-endian, fixed size; floats are IEEE-754 `f32`. **Strings** are `str8`: a
  length byte, then that many bytes of UTF-8 (no terminator). Booleans are one byte, `0` or `1`.
- A receiver refuses a payload that is shorter or longer than its layout, or that has a value the
  version doesn't define (an unknown enum value, a flag bit that isn't listed, a boolean that isn't
  0 or 1). Within one protocol version every layout is exact.
- **Speed:** 115 200 baud by default (works on every Arduino), 8N1.

## 3. Messages

`HELLO` and `HELLO_ACK` have the same layout in every protocol version, so two sides can always
agree on one. Every other layout belongs to the negotiated version; this page is **version 1**.

| Type | Name | Direction | When |
|---|---|---|---|
| `0x01` | `HELLO` | core → board | until the board answers, every 500 ms |
| `0x02` | `HELLO_ACK` | board → core | the answer to every `HELLO` |
| `0x03` | `DESCRIBE` | core → board | after `HELLO_ACK`, again every 500 ms until every `PART` arrived |
| `0x04` | `PART` | board → core | one per hosted part, in answer to `DESCRIBE` |
| `0x10` | `HEARTBEAT` | core → board | every 50 ms once the board has answered |
| `0x11` | `STATUS` | board → core | every 100 ms once connected |
| `0x20` | `READING` | board → core | per part, at the part's own rate |
| `0x21` | `COMMAND` | core → board | only while the core holds a lease on the part |
| `0x22` | `APPLIED` | board → core | after every `COMMAND`, and whenever the board itself stops an actuator |
| `0x23` | `SAFE` | core → board | put one part, or every part, in its safe state now |
| `0x30` | `ESTOP` | either way | engage or release the e-stop |
| `0x40` | `LOG` | board → core | a line of text for the bridge part's log |

### Payloads

**`HELLO`** (3 B): `min_version u8 · max_version u8 · max_payload u8` — the versions the core speaks
and the largest payload it accepts.

**`HELLO_ACK`**: `version u8 · min_version u8 · max_version u8 · max_payload u8 ·
heartbeat_timeout_ms u16 · part_count u8 · board str8 · firmware str8 · firmware_version str8`
- `version`: the highest version both sides speak, or **0** if the ranges don't overlap. With 0 the
  board stays unconnected (it sends nothing and applies nothing) and the core reports a version
  mismatch.
- `min_version`/`max_version`: the versions the board speaks.
- `max_payload`: the largest payload the board accepts; the core never sends more. The board in
  turn sends at most `min(its own, the core's)` once connected.
- `heartbeat_timeout_ms`: no `HEARTBEAT` for this long and the board stops every actuator
  (100–1000 ms, default 250).

**`DESCRIBE`** (0 B).

**`PART`**: `id u8 · count u8 · lease_ms u16 · capability str8 · name str8 · info`
- `id`: the board's local id for the part (0, 1, 2, … in the order the sketch declared them). Every
  later message about the part uses it.
- `count`: how many parts the board has.
- `lease_ms`: for actuators, no `COMMAND` for this long and the board stops it (250 for motors);
  `0` for sensors.
- `capability`: the versioned capability id, e.g. `motor@1`. A core that doesn't know it keeps the
  info bytes raw and can say which capability it was.
- `info`: the rest of the payload, the capability's info layout (§4).

**`HEARTBEAT`** (1 B): `flags u8` — bit 0: the e-stop is engaged in Faber. The e-stop travels as a
level in every heartbeat, so a lost `ESTOP` frame is corrected within 50 ms.

**`STATUS`** (11 B): `health u8 · flags u8 · uptime_ms u32 · last_fault u8 · rx_bad u16 · rx_gaps u16`
- `health`: 0 ok, 1 degraded (failsafe, e-stop or bad frames seen), 2 fault.
- `flags`: bit 0 a hardware e-stop is latched on the board; bit 1 the board holds its actuators
  stopped for an e-stop (from either side); bit 2 failsafe: the heartbeat timed out.
- `uptime_ms`: the board's clock (`millis()`).
- `last_fault`: 0 none, 1 heartbeat lost, 2 lease expired, 3 bad frames, 4 refused command, 5 e-stop.
- `rx_bad`, `rx_gaps`: frames the board dropped, and frames it missed (from `seq`), saturating.

**`READING`**: `part u8 · capability_code u8 · t_ms u32 · reading` — `t_ms` is the board's clock in
ms (it wraps after 49.7 days). The core stamps the reading with its own clock when it arrives.

**`COMMAND`**: `part u8 · capability_code u8 · command`

**`APPLIED`**: `part u8 · capability_code u8 · cause u8 · clamped u8 · command` — what the board does
*now*, after its own limits. `cause`: 0 a command was applied, 1 a command was refused (unsupported,
not a number, or the wrong capability; refusing stops the actuator), 2 the heartbeat was lost, 3 the
part's own lease expired, 4 `SAFE`, 5 the e-stop, 6 a command came without a live heartbeat.
`clamped` is 1 when a limit changed the command.

**`SAFE`** (1 B): `part u8`, or `0xFF` for every part.

**`ESTOP`**: `engaged u8 · source u8 · reason str8` — `source`: 0 part (e.g. the board's own button),
1 app, 2 shell. The core cuts `reason` to fit the board's `max_payload` (and to 60 bytes).

**`LOG`**: `level u8 · text` — `level` 0 info, 1 warn, 2 error; `text` is the rest of the payload
as UTF-8 (a receiver shows invalid bytes as `�` rather than dropping the line).

## 4. Capability layouts

`READING`, `COMMAND` and `APPLIED` carry a one-byte **capability code**; `PART` carries the id. Each
layout maps 1:1 onto the types in `sdk/rust/capabilities` (and `wit/capabilities`).

| Code | Capability |
|---|---|
| 1 | `distance@1` |
| 2 | `motor@1` |
| 3 | `planar-base@1` |
| 4 | `estop@1` |

Other capabilities have no FaberLink layout yet; one is added (with a new code) when a board needs
it. A `twist` below is `vx f32 · vy f32 · omega f32`.

| Capability | Reading | Command | Info |
|---|---|---|---|
| `distance@1` | `meters f32 · status u8` (5 B). `status`: 0 ok, 1 too-close, 2 too-far, 3 no-target, 4 error. `meters` is always finite (too-close/too-far report the range's edge, no-target its max). | — | `min_m f32 · max_m f32 · field_of_view_rad f32 · kind u8` (13 B). `kind`: 0 ultrasonic, 1 laser-tof, 2 infrared, 3 radar. |
| `motor@1` | `present u8 · velocity_rad_s f32 · position_rad f32 · current_a f32` (13 B). `present` bits 0/1/2 say which of the three are measured; the others are sent as 0. | `kind u8 · value f32` (5 B). `kind`: 0 set-velocity (rad/s), 1 set-position (rad), 2 set-duty (−1..1). The safe state is set-velocity 0. | `kind u8 · modes u8 · max_velocity_rad_s f32` (6 B). `kind`: 0 dc, 1 stepper, 2 bldc, 3 continuous-servo. `modes` bits 0/1/2: velocity, position, duty. A NaN maximum means "unknown". |
| `planar-base@1` | `flags u8 · commanded twist · measured twist` (25 B). `flags` bit 0: `measured` is known; bit 1: bumped. | `kind u8 (0 = set-velocity) · twist` (13 B) | `holonomic u8 · max_velocity twist · max_acceleration twist · n u8 · n × str8` (the parts it is built from, by their names on the board) |
| `estop@1` | `engaged u8 · source u8 · reason str8` | — | `hardware_latching u8` (1 B) |

## 5. Rules a board follows (the library enforces them, not the sketch)

A board is **connected** after a `HELLO` with a version in common. Before that it sends nothing but
`HELLO_ACK` and ignores every other message. A new `HELLO` starts a new session: every running
actuator stops (without an `APPLIED`) and the heartbeat must start again.

1. **No heartbeat, no motion.** When no `HEARTBEAT` has arrived for the heartbeat timeout
   (default 250 ms, configurable 100–1000 ms per board), every running actuator goes to its safe
   state (`APPLIED` cause 2) and `STATUS` shows failsafe. A `COMMAND` without a live heartbeat is
   refused (cause 6). **Recovery needs a new `COMMAND`**: a heartbeat alone never restarts anything.
2. **An actuator's own lease.** An actuator that gets no `COMMAND` for its `lease_ms` (250 ms for
   motors) stops by itself (cause 3), even while heartbeats arrive. So a lost lease on the Pi stops
   the motor even if `SAFE` is lost on the wire.
3. **The e-stop wins.** While an e-stop is engaged, every actuator is stopped and every command is
   refused (cause 5). Faber's e-stop comes as `ESTOP` and as the heartbeat's level. A **hardware
   e-stop** on the board latches: it is cleared only when it has been **reset on the board** (in the
   sketch, `link.resetEstop()`) **and then released in Faber** (an `ESTOP` release that arrives
   after the reset). Releasing never restarts anything; a new command does.
4. **The safety checks run before every received message and on every `poll()`**, so a heartbeat
   that arrives late can't hide that it timed out.
5. **Things that move start stopped**: every actuator is put in its safe state at `begin()`.
6. Ramp-down for `planar-base` happens on the board (capabilities.md); no built-in part type does it
   yet.

## 6. The core side (`sdk/rust/faberlink`)

- `cobs`, `crc` and `frame` need neither `std` nor an allocator; `message` and `layout` turn payloads
  into the capability types and back; `session` is the connection itself.
- `Session` is a pure state machine with the clock passed in: `receive(bytes, now)`, `tick(now)`,
  `take_output()`, `take_events()`. It sends `HELLO` until the board answers, `DESCRIBE` until every
  `PART` has arrived, heartbeats every 50 ms carrying the e-stop; it routes readings to their part
  (and drops a reading that doesn't fit its part's capability), sends `COMMAND` only when told the
  core holds a lease, and counts bad, undecodable and missed frames. A board silent for 500 ms is
  lost and the session starts over. A version mismatch ends in `Failed` with an error that says what
  to update.
- Property tests (proptest) show encode → decode is the identity for frames, every message and every
  layout, and that random garbage never panics and resyncs at the next `0x00`; an exhaustive test
  drops every single flipped bit of a frame.

## 7. The Arduino library (`firmware/faberlink`)

```cpp
#include <FaberLink.h>

FaberLink link(Serial);
FaberMotor left("left", 9, 8);           // PWM pin, direction pin
FaberDistance front("front", A0);        // an analog distance sensor

void setup() { link.begin(115200, "sennep-mega", "0.1.0"); }
void loop()  { link.poll(); }            // everything else is done by the library
```

- `src/FaberLinkCore.h/.cpp` is the protocol and the safety rules in plain, portable C++11: no
  Arduino headers, no dynamic allocation, no exceptions, no RTTI (a test reads the source to keep it
  that way). It is what the interop tests compile.
- `src/FaberLink.h/.cpp` is the Arduino glue: a `Stream` adapter, `millis()`, and the built-in part
  types `FaberMotor` (an H-bridge with a PWM and a direction pin; set-duty, and set-velocity open
  loop when the motor's full-duty speed is given) and `FaberDistance` (analog; linear, or a curve
  function). A custom part derives from `faberlink::Part` (or `Motor`/`Distance`) and is added with
  `link.add(part)`. Globally declared parts register themselves.
- Example: `examples/MotorAndDistance`. CI compiles every example for an Uno and a Mega.
- Not built yet: `FaberServo`, digital in/out, HC-SR04, VL53L0X, encoders, a planar-base part with
  on-board ramp-down, and negotiating 1 Mbaud.
- **Where it lives:** inside the Faber repo for now. Moving it to its own repo
  (`faber-os/faberlink-arduino`, so the Arduino Library Manager can list it) needs Mikkel's OK; the
  folder is already a complete Arduino library, so the move is a copy.

### RAM and flash on an Uno

On boards with 2–2.5 KB of RAM (Uno, Nano, Leonardo) the library takes payloads of up to
**64 bytes** instead of 250 (`FABERLINK_MAX_PAYLOAD`, set per board in `FaberLinkCore.h`; only
change it through build flags so every file agrees). Every message a few parts need fits in 64.

| What | RAM |
|---|---|
| The board object (two 68-byte frame buffers and its state) and the `FaberLink` glue | 189 B |
| The table that collects globally declared parts | 33 B |
| Each `FaberMotor` / `FaberDistance` | 33 B / 42 B |
| Virtual tables, part names and other strings | ~100 B, depending on the sketch |
| Stack at its deepest (receive → command → apply → stop → send), from `-fstack-usage` | ~150 B |

`MotorAndDistance` on an Uno (avr-gcc 7.3, Arduino AVR core 1.8.8): **9 298 B flash (28 %)** and
**639 B of globals (31 %)**, of which the Arduino core's `Serial` is 157 B, leaving 1 409 B for the
stack. On a Mega (250-byte payloads): 10 422 B flash and 1 011 B of globals.

## 8. Interop tests (`tools/faberlink-interop`)

The C++ core is compiled for the PC with the `cc` crate (MSVC on Windows, g++/clang elsewhere,
warnings as errors) together with a small C ABI harness: a fake board with a simulated motor
(part 0) and distance sensor (part 1), fed bytes and a fake `millis()`. The tests drive it from the
Rust session over an in-memory pipe and prove, on every CI run:

- the handshake and `DESCRIBE` give the right parts, infos and heartbeat timeout (and the timeout is
  clamped to 100–1000 ms);
- readings decode to the right capability types (ok, too-close, too-far, no-target);
- commands apply, are clamped to the board's limits, and refused commands stop the motor;
- no heartbeat for the timeout stops the motor, and it doesn't resume on heartbeats alone, only on a
  new command; a command without a heartbeat is refused;
- an actuator stops on its own lease timeout (exactly 250 ms) even with heartbeats arriving;
- a hardware e-stop latches until it is reset on the board **and then** released in Faber; Faber's
  e-stop reaches the board even when the `ESTOP` frame is lost;
- every single flipped bit of a command is dropped; bit flips, truncation, inserted garbage and
  lost bytes in both directions are dropped and the link recovers; random bytes never break the
  board;
- a version mismatch gives a clear error and the board stays unconnected.

## 9. In the core (Phase 4b)

- `faber-bridge` connector: built-in Rust first (like the LD19), wasm later. It owns the port and
  runs a `faberlink::Session`.
- A board's parts appear as **sub-parts** of the bridge part: `mega` → `mega/left`, `mega/front`,
  each with its own viewer and lease. They are **nested** in parts.toml under the bridge so their
  names and mounts can be set: `[parts.mega.parts.left] mount = …` (decided: the bridge owns them).
- The Arduino flasher tool (avrdude, bundled) installs the right firmware, so most people never
  open the Arduino IDE.

## 10. Decisions and open questions

Decided while building Phase 4a (Mikkel's recommendations from the design, taken as given):

1. **Sub-parts** are nested in parts.toml under the bridge.
2. **Heartbeat timeout** 250 ms by default, configurable per board within 100–1000 ms (at SENNEP's
   0.6 m/s that is about 15 cm plus the ramp).
3. **The library stays in the Faber repo** until Mikkel agrees to a separate
   `faber-os/faberlink-arduino` repo.
4. Small boards take 64-byte payloads; the board says so in `HELLO_ACK` and the core respects it.
5. Readings are stamped with the core's clock on arrival; the board's `t_ms` is kept on the wire for
   a later clock mapping.

Still open:

1. **Retrofit order**: SENNEP's Mega firmware is rewritten on FaberLink now that the library exists
   (D22). Fine to do that inside the SENNEP V2 repo?
