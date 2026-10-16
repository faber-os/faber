# FaberLink

Status: **protocol v1 and the Arduino library built** (Phase 4a, 2026-10-15), and the
`faber-bridge` connector that brings a board's parts into the core (Phase 4b, §9). Builds on D11,
D12 and D22 (designed fresh, not from SENNEP's early protocol).

FaberLink is the serial protocol between the core and a microcontroller (a **bridge**). The
microcontroller declares the parts it hosts; the `faber-bridge` connector turns them into normal
parts with normal capabilities; leases and the e-stop reach the motors; and the microcontroller
stops its own motors if the Pi goes quiet.

| Piece | Where | What |
|---|---|---|
| Core side (Rust) | `sdk/rust/faberlink` | framing, every message, the capability layouts, and the session state machine |
| Board side (C++) | `firmware/faberlink` | the Arduino library: a portable protocol core plus thin Arduino glue |
| Interop tests | `tools/faberlink-interop` | the Rust session against the C++ core, compiled on the PC |
| The connector | `core/src/bridge` | `faber-bridge`: runs a board on its port, its parts as sub-parts (§9) |

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
`HELLO_ACK` is the one layout that may grow: it can end in **optional fields** (below), and a
receiver ignores bytes after the optional fields it knows, because it reads `HELLO_ACK` before any
version is agreed.

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
heartbeat_timeout_ms u16 · part_count u8 · board str8 · firmware str8 · firmware_version str8`,
then the optional `build_id u64`
- `version`: the highest version both sides speak, or **0** if the ranges don't overlap. With 0 the
  board stays unconnected (it sends nothing and applies nothing) and the core reports a version
  mismatch.
- `min_version`/`max_version`: the versions the board speaks.
- `max_payload`: the largest payload the board accepts; the core never sends more. The board in
  turn sends at most `min(its own, the core's)` once connected.
- `heartbeat_timeout_ms`: no `HEARTBEAT` for this long and the board stops every actuator
  (100–1000 ms, default 250).
- `build_id` (optional, added 2026-10-16): which build of the sketch the board runs. Faber's
  Arduino editor compiles every sketch with `-DFABER_BUILD_ID=0x…`, the first 8 bytes of a SHA-256
  over the sketch's files and board ([arduino-editor.md](arduino-editor.md#the-same-code-check)),
  and the library sends it here. A sketch compiled anywhere else (the Arduino IDE) has none and
  sends exactly the bytes it always did. A receiver takes 0 bytes after the strings as "no build
  id", 8 or more as the build id (ignoring what follows: a later optional field), and refuses 1–7
  (a cut-off frame). The board leaves room for it when it shortens its strings to fit
  `max_payload`.
- **Compatibility:** a board without a build id works with every core. A board *with* one needs a
  core from 2026-10-16 or later: an older core refuses the longer `HELLO_ACK` ("No FaberLink
  answer"). That only happens if a sketch compiled by a newer Faber's editor meets an older core;
  the editor always compiles with the library copy built into the same core.
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
| 5 | `battery@1` |

Other capabilities have no FaberLink layout yet; one is added (with a new code) when a board needs
it. A `twist` below is `vx f32 · vy f32 · omega f32`.

| Capability | Reading | Command | Info |
|---|---|---|---|
| `distance@1` | `meters f32 · status u8` (5 B). `status`: 0 ok, 1 too-close, 2 too-far, 3 no-target, 4 error. `meters` is always finite (too-close/too-far report the range's edge, no-target its max). | — | `min_m f32 · max_m f32 · field_of_view_rad f32 · kind u8` (13 B). `kind`: 0 ultrasonic, 1 laser-tof, 2 infrared, 3 radar. |
| `motor@1` | `present u8 · velocity_rad_s f32 · position_rad f32 · current_a f32` (13 B). `present` bits 0/1/2 say which of the three are measured; the others are sent as 0. | `kind u8 · value f32` (5 B). `kind`: 0 set-velocity (rad/s), 1 set-position (rad), 2 set-duty (−1..1). The safe state is set-velocity 0. | `kind u8 · modes u8 · max_velocity_rad_s f32` (6 B). `kind`: 0 dc, 1 stepper, 2 bldc, 3 continuous-servo. `modes` bits 0/1/2: velocity, position, duty. A NaN maximum means "unknown". |
| `planar-base@1` | `flags u8 · commanded twist · measured twist` (25 B). `flags` bit 0: `measured` is known; bit 1: bumped. | `kind u8 (0 = set-velocity) · twist` (13 B) | `holonomic u8 · max_velocity twist · max_acceleration twist · n u8 · n × str8` (the parts it is built from, by their names on the board) |
| `estop@1` | `engaged u8 · source u8 · reason str8` | — | `hardware_latching u8` (1 B) |
| `battery@1` | `present u8 · volts f32 · amps f32 · charge f32 · celsius f32 · level u8 · n u8 · n × cell_volts f32` (19 B + 4n). `present` bits 0/1/2/3: amps, charge, celsius, cell volts; the others are sent as 0, and `n` is 0 without bit 3. `level`: 0 ok, 1 low, 2 critical. `volts` is always finite. | — | `chemistry u8 · cells_in_series u8 · capacity_ah f32` (6 B). `chemistry`: 0 li-ion, 1 lipo, 2 lifepo4, 3 nimh, 4 lead-acid. `cells_in_series` is at least 1. A NaN capacity means "unknown". |

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
- `FaberBattery` measures a battery through a voltage divider on an analog pin:
  `FaberBattery battery("battery", A1, 3.0, 3);` is a 3S Li-ion pack behind a 20 kΩ / 10 kΩ
  divider (ratio `(R1 + R2) / R2` = 3.0). Further arguments: the chemistry, the analog reference
  (5 V on an Uno or Mega) and the capacity in Ah. It averages 8 reads twice a second and sends the
  voltage and the level; `faberlink::Battery` computes the level from the same per-chemistry
  thresholds and hysteresis as `faber-capabilities` (Li-ion 3S: low below 10.5 V, critical below
  9.9 V). The library can't link Rust, so the numbers are copied; an interop test checks that both
  give the same level at every voltage, for every chemistry, from every previous level.
- `FaberUltrasonic` reads a trigger/echo range finder such as the HC-SR04:
  `FaberUltrasonic front("front", 7, 6);` (trigger, echo; 2 cm to 4 m by default, the HC-SR04
  datasheet's range). One measurement per 60 ms, as the datasheet asks; each waits at most about
  24 ms for the echo (for 4 m), and the speed of sound is taken at about 20 °C (343 m/s).
- Examples: `examples/MotorAndDistance`, `examples/Ultrasonic` and `examples/BatteryMonitor`. CI
  compiles every example for an Uno and a Mega.
- **Build id:** when the sketch is compiled with `FABER_BUILD_ID` defined (Faber's Arduino editor
  does that, [arduino-editor.md](arduino-editor.md)), `link.begin()` hands it to the board, which
  sends it at the end of every `HELLO_ACK` (§3). Faber then shows which sketch the board runs.
- Not built yet: `FaberServo`, digital in/out, VL53L0X, encoders, a planar-base part with
  on-board ramp-down (the interop harness has a plain test one, for SENNEP's fake Mega:
  `FakeBoard::sennep`), and negotiating 1 Mbaud.
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
| Each `FaberMotor` / `FaberDistance` / `FaberBattery` | 33 B / 42 B / about 34 B |
| Virtual tables, part names and other strings | ~100 B, depending on the sketch |
| Stack at its deepest (receive → command → apply → stop → send), from `-fstack-usage` | ~150 B |

`MotorAndDistance` on an Uno (avr-gcc 7.3, Arduino AVR core 1.8.8): **9 298 B flash (28 %)** and
**639 B of globals (31 %)**, of which the Arduino core's `Serial` is 157 B, leaving 1 409 B for the
stack. On a Mega (250-byte payloads): 10 422 B flash and 1 011 B of globals.

Battery support (2026-10-16) adds 12 B of flash and 12 B of RAM to every sketch (the
`"battery@1"` id string). `BatteryMonitor` alone on an Uno: 6 896 B flash and 590 B of globals.

The build id (2026-10-16) adds 52 B of flash and 9 B of RAM to every sketch, and another 52 B of
flash when the sketch is compiled with one. Measured with `arduino-cli` 1.5.1 and AVR core 1.8.8
after that change, without a build id:

| Example | Uno flash | Uno globals | Mega flash | Mega globals |
|---|---|---|---|---|
| `MotorAndDistance` | 9 382 B | 662 B | 10 500 B | 1 034 B |
| `BatteryMonitor` | 6 948 B | 599 B | 7 348 B | 971 B |
| `Ultrasonic` | 6 874 B | 598 B | 7 654 B | 970 B |
These numbers were measured on a PC without `arduino-cli`, compiling sketch, library and AVR core
1.8.8 with the Arduino IDE's avr-gcc 7.3 (`-Os`, LTO, `--gc-sections`, `-Wall -Wextra -Werror`);
that build gives `MotorAndDistance` 20 B more flash and 2 B more globals than the figures above,
which came from `arduino-cli`.

## 8. Interop tests (`tools/faberlink-interop`)

The C++ core is compiled for the PC with the `cc` crate (MSVC on Windows, g++/clang elsewhere,
warnings as errors) together with a small C ABI harness: a fake board with a simulated motor
(part 0) and distance sensor (part 1), fed bytes and a fake `millis()`; `FakeBoard::mega` hosts two
motors and two distance sensors, and the core's own tests put it behind a fake serial port (§9).
The tests drive it from the Rust session over an in-memory pipe and prove, on every CI run:

- the handshake and `DESCRIBE` give the right parts, infos and heartbeat timeout (and the timeout is
  clamped to 100–1000 ms);
- readings decode to the right capability types (ok, too-close, too-far, no-target), and a
  battery reports its voltage and level (`FakeBoard::with_battery`); the library and
  `faber-capabilities` give the same battery level at every voltage, for every chemistry;
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

## 9. In the core: the `faber-bridge` connector

Built in Phase 4b, in Rust inside the core (`core/src/bridge/`; wasm later). Add a board as one part
on its serial port, `connector = "faber-bridge"`; its motors and sensors appear by themselves.

### Sub-parts and their names

Each part the board describes becomes a **sub-part** named `<bridge>.<name>`: the board `mega`
with the sketch's `FaberMotor left(…)` gives `mega.left`. Its one signal is named after its
capability: `mega.left/motor`, `mega.front/distance`. The bridge's own signal is `mega/estop`, the
board's stop button (`estop@1`).

**Why `.` and not `/`** (decided in Phase 4b): a signal address is `part/name`, and a part's named
instances already use the `/` (`mega/left` could be the `left` signal of the part `mega`). With
`mega/left/motor`, every reader of an address would have to know which parts are bridges to split
it, and `mega/left` would mean two things. A name the user picks may not contain a `.`, and a
signal name never does, so `mega.left/motor` splits at its one `/` and never collides with an
address that exists today. It also fits in URLs as it is (`/api/v0/parts/mega.left/lease`).

A board's name for a part that isn't a valid part name is lowercased and every run of other
characters becomes `_` (`Left Motor` → `left_motor`); one with nothing usable left, or a name two
parts share, becomes `part<id>`. A drive base's built-from list names the board's parts; the core
turns it into sub-part names, so a lease on `mega.base` locks `mega.left` and `mega.right`.

### parts.toml

The board decides which parts exist; parts.toml only says where they sit, nested under the bridge:

```toml
[parts.mega]
connector = "faber-bridge"
port = "usb:2341:0042"

[parts.mega.parts.left]       # the board's part "left"
mount = { x = 0.2, y = 0.3 }
```

- `mount` is the only key (anything else is an error with its line), and only a `faber-bridge`
  part may have `parts`.
- Changing a sub-part's mount doesn't restart the bridge, so leases on its parts carry on.
- A name listed here that the board doesn't have shows as **Not on the board** (degraded), with a
  hint to check the sketch or remove the lines. Such a stale entry can be removed in Parts; a part
  the board has can't (the board's sketch decides).
- Editing keeps comments: removing the bridge removes its nested tables and the comments directly
  above them, and nothing else.

### The link

The bridge owns the port (claimed like any part's) and runs a `faberlink::Session` on its own
thread: `HELLO` until the board answers, `DESCRIBE` until every `PART` arrived, a heartbeat every
50 ms carrying Faber's e-stop, and the board's `STATUS` every 100 ms. It never waits for the
machine's lock; what the machine must act on goes through the lease sweeper's inbox.

| The bridge's state | When | What it says to do |
|---|---|---|
| starting | port open, saying hello or asking for parts | |
| running | parts known, STATUS arriving | |
| failed: *No FaberLink answer from COM5* | no `HELLO_ACK` for 2.5 s | Is the board running a FaberLink sketch, and is the baud right (115200, as in `link.begin(115200, …)`)? Faber keeps asking. |
| failed: *The board "mega" (sennep 2.0.0) speaks FaberLink versions 2–3, but this Faber speaks version 1. Update Faber…* | `HELLO_ACK` version 0 | Faber asks again every few seconds, so a reflashed board connects by itself. |
| failed: *The board "mega" stopped answering.* | no frame for 500 ms | Check the cable and power; `link.poll()` must run often. |
| failed: *COM5 stopped working: …* / *Could not open COM5* | the port closed (unplugged) or can't be opened | Faber reconnects as soon as it is back. |
| failed: *The board reports a fault: …* | `STATUS` health 2 | Check wiring and power, reset the board. |
| degraded: *The board stopped sending its status.* | no `STATUS` for 500 ms while other frames arrive | Old firmware, or a busy `loop()`. |
| degraded: *The stop button on the board is pressed.* | `STATUS` bit 0 | Reset it on the board, then release the e-stop in Faber. |
| degraded: *The board stopped its motors: Faber's heartbeat didn't reach it in time.* | `STATUS` failsafe | Check the cable; motors start again on the next command. |
| degraded: *The board has thrown away n damaged messages.* | `rx_bad` grew in the last 5 s | Check the cable. |

While the board is out of reach every sub-part is **failed** ("Stopped: …"), and the lease
sweeper ends their leases (`part-failed`). The port is reopened with backoff (1 s doubling to 5 s),
and the board is described again on every connect: new firmware with other parts is picked up,
signals of parts that are gone are removed, and leases on parts that went away or changed end.

### Leases, commands and safety

- A sub-part that moves takes a lease like any part (`POST /api/v0/parts/mega.left/lease`, 250 ms).
  The core checks the lease and clamps the command to the board's own info **before** anything is
  sent; `COMMAND` frames are only ever written for a command that passed.
- Every lease end (release, expiry, takeover, e-stop, the part failing, removal) sends `SAFE` for
  that part, and for the parts a base is built from.
- If `SAFE` is lost on the wire, the board's own lease (250 ms for motors) stops the motor anyway;
  if the core freezes, the board's heartbeat timeout does.
- Faber's e-stop goes to every board as an `ESTOP` frame plus `SAFE` for everything, and as the
  level in every heartbeat (a board that connects while it is engaged hears it in its first
  heartbeat). Releasing restarts nothing.
- The board's **own stop button** engages Faber's e-stop with source `part`. It stays latched:
  releasing in Faber is refused (`409 estop-latched`) until the button is reset on the board, and
  the release that follows clears the board's latch (§5, rule 3). A board that goes away no longer
  blocks the release; one that comes back still latched engages the e-stop again.

### Detect, then suggest

A port that answers `HELLO` is suggested as "A FaberLink board: sennep-mega (sennep 0.1.0)" in
Add part. The check (`POST /api/v0/ports/check-board`) runs **only when someone asks** ("Check for
a FaberLink board" on a port in Add part), never by itself, and only on a port nobody uses. That is
on purpose: opening a serial port toggles DTR, which restarts most Arduino-based devices (a 3D
printer mid-print is one), and bytes a device doesn't expect may confuse it. The check sends only
`HELLO` (no commands, no heartbeat), listens for up to 2.5 s because an Arduino takes about a
second to start after the reset, then closes the port.

### Tests

- `core/src/bridge/tests.rs`: the bridge against the C++ board (`FakeBoard::mega`) and a scripted
  Rust board behind a fake serial port: sub-parts and readings, commands and `SAFE`, the e-stop both
  ways, no answer, another version, unplug and reconnect, new firmware, a board fault, status
  silence, a base's built-from, and the probe.
- `core/src/machine/bridge_tests.rs`: leases, commands, Not on the board, removing, mounts.
- `core/tests/arduino.rs`: a sketch uploaded from the Arduino editor, then run as the C++ board
  with that build id: Parts and `GET /api/v0/arduino/boards/{port}/code` prove the code live; the
  same sketch without a build id is "different" ([arduino-editor.md](arduino-editor.md)).
- `core/tests/bridge.rs`: **the roadmap's proof**, through the HTTP API with the C++ board.
  The Mega shows two motors and two distance sensors; a program with a lease drives a motor;
  killing it stops the motor within the lease time through `SAFE` (with the board's own lease set
  to 1 s, so only `SAFE` explains it), and with every `SAFE` lost on the cable through the board's
  own lease; the e-stop stops everything; the stop button latches until reset on the board;
  unplugging fails the bridge, stops its parts and ends every lease.

### The Arduino flasher

Built in Phase 4c: Start → Arduino flasher puts a compiled sketch (Intel HEX) on an Uno, Nano or
Mega over its USB port, from Files or from a package's `firmware/<board>.hex`, so a bridge's
firmware can be installed without the Arduino IDE ([connect-an-arduino.md](connect-an-arduino.md#flash-from-faber),
[api.md](api.md#the-arduino-flasher)). It is written in Rust (`sdk/rust/flash`, crate
`faber-flash`) instead of bundling avrdude: one binary for every OS the core runs on, no external
program to ship, and the protocol code testable against simulated bootloaders. A FaberLink board
on the port is stopped only when the user says so, and starts again on the new firmware. Tested on
simulated bootloaders only so far; it needs a run on a real board.

### Not built yet

- A bridge as a wasm connector.
- Other speeds than 115 200 baud.
- Using the board's `t_ms` to map its clock (readings are stamped on arrival, §10 decision 5).

## 10. Decisions and open questions

Decided while building Phase 4a (Mikkel's recommendations from the design, taken as given):

1. **Sub-parts** are nested in parts.toml under the bridge (built that way in 4b, §9).
2. **Heartbeat timeout** 250 ms by default, configurable per board within 100–1000 ms (at SENNEP's
   0.6 m/s that is about 15 cm plus the ramp).
3. **The library stays in the Faber repo** until Mikkel agrees to a separate
   `faber-os/faberlink-arduino` repo.
4. Small boards take 64-byte payloads; the board says so in `HELLO_ACK` and the core respects it.
5. Readings are stamped with the core's clock on arrival; the board's `t_ms` is kept on the wire for
   a later clock mapping.

Decided while building Phase 4b:

6. **Sub-part names use `.`**: `mega.left`, with the signal `mega.left/motor` (§9 says why not `/`).
7. **Checking a port for a board is opt-in**, per port, never automatic (§9).
8. **A board's stop button latches in Faber too**: release is refused until it is reset on the
   board.

Still open:

1. **Retrofit order**: SENNEP's Mega firmware is rewritten on FaberLink now that the library exists
   (D22). Fine to do that inside the SENNEP V2 repo?
