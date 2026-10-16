# Design: parts on a board's pins (guided and code mode)

Status: **design v1, accepted** (D57, 2026-10-16; Mikkel took every recommendation in §10). Being built. Builds on the Arduino
editor ([arduino-editor.md](arduino-editor.md)), FaberLink ([faberlink.md](faberlink.md)) and the
same-code check (D54).

## 1. The problem

A part wired to an Arduino (a sonar on pins 7 and 6) reaches Faber only through the sketch:
`FaberUltrasonic front("front", 7, 6);`. That is fine for someone who writes C++, and a wall for
everyone else. They have to know which pins can do what, which are already taken, how to wire the
part, and how to write and upload the sketch.

## 2. Two modes, chosen per board

| | **Code mode** | **Guided mode** |
|---|---|---|
| Who owns the sketch | You. You write it in the editor (or the Arduino IDE). | Faber. It writes the sketch from the board's *plan*. |
| How a part is added | Declare it in the sketch, upload. | Parts → **Add part → On a board's pins** → pick the part → pick pins → upload. |
| Pins | You choose; Faber shows what the board reports in use (§6). | Faber suggests free, suitable pins and refuses unsuitable ones. |
| Your own code | Anything. | In `my_code.h`, a tab Faber never overwrites (§5). |
| Leaving | — | **Take over the code** turns a guided board into a code-mode board (one way, §8). |

A board is in exactly one mode. The mode belongs to the board, so a Mega can be guided while an Uno
next to it runs your own sketch.

## 3. The guided flow (what the user sees)

1. **Add part → On a board's pins.** Pick the board (a connected FaberLink board, or "a new board":
   pick its type, plug it in when asked). Then pick the part type from a catalogue with pictures:
   ultrasonic distance, IR distance, DC motor with an H-bridge, battery voltage, and later servo,
   encoder, button, LED and I²C sensors.
2. **Name it** ("front"), with a sensible default.
3. **Pins.** A drawing of the board's headers shows every pin, coloured:
   - **suggested**: Faber's pick, which is free and suitable;
   - **suitable**: any other free pin that can do the job;
   - **unsuitable**, with the reason on hover ("Echo needs a digital pin; A6 on a Nano is analog
     only");
   - **in use**, with what by ("D7: front, Trig");
   - **reserved**, with the reason ("D0/D1: the USB link to Faber").

   Each role of the part (Trig, Echo, PWM, IN1, IN2, …) is filled by clicking a pin. Suggested
   pins are filled in from the start, so most users only press Next.
4. **Wiring.** A checklist with the drawing: "HC-SR04 VCC → 5V · GND → GND · Trig → D7 · Echo → D6",
   plus warnings where they matter ("Motors need their own supply: never power the motor from the
   board's 5V pin"; "This sensor is 3.3 V only: use a level shifter"). It can be printed or kept
   open in a window while wiring.
5. **Upload.** Faber writes the sketch, compiles it, shows the size, and uploads after the usual
   confirm. The same-code check (D54) records it.
6. **Check it works.** The new part appears under the board with a live value. For an actuator,
   a short guarded test ("Spin the left motor slowly for 1 s?") runs with the D46 confirm and the
   Stop bar.

Removing or moving a part works the same way: edit the plan, see the new wiring, upload.

## 4. What Faber must know

### Board pin maps (data, one file per board)
Every pin with its abilities: digital I/O, analog in (and analog-*only* pins like A6/A7 on a Nano),
PWM, external interrupt, I²C (SDA/SCL), SPI (MOSI/MISO/SCK/SS), hardware serial, the on-board LED,
and the voltage (5 V or 3.3 V). Also the **conflicts that bite beginners**:
- D0/D1 carry the USB link to Faber, so they are reserved (Serial1–3 on a Mega are free);
- the `Servo` library takes over PWM on D9/D10 on an Uno; on a Mega it takes Timer5 first, so
  the first servo stops PWM on D44–D46 (D11/D12 only go after 12 servos);
- `tone()` takes PWM on D3/D11 on an Uno;
- I²C pins can't be used as ordinary pins while an I²C part exists;
- D13 drives the on-board LED, which is fine for outputs but weak as an input with a pull-up;
- interrupt pins are scarce: an Uno has D2/D3, and of a Mega's six, D20/D21 are also I²C and
  D18/D19 also Serial1, which matters for wheel encoders;
- common modules share I²C addresses out of the box (0x68 MPU6050 vs DS3231, 0x40 INA219 vs
  PCA9685, 0x29 VL53L0X vs TCS34725, 0x27 LCD backpack vs MCP23017); the allocator checks
  addresses like pins;
- addressable LED strips turn interrupts off while they update (about 30 µs per pixel), which
  drops FaberLink bytes and encoder edges on a long strip: guided mode warns, and suggests a
  separate board for the strip.

**Pin providers.** Some parts add pins instead of measuring anything (PCA9685 PWM, MCP23017 GPIO,
ADS1115 analog in, 74HC595 outputs). The pin-map format lets a part contribute pins, so the
allocator can place a servo on a PCA9685 channel. On a Pi, which has no analog inputs, this is the
only way to use an analog part.

The first set is Uno, Nano (old and new bootloader), Mega 2560, i.e. the flasher's boards. The
values come from the ATmega datasheets and Arduino's published board pinouts. Each file is checked
by tests (every pin has a valid type, no ability claimed that its chip lacks).

### Part templates (data, one per FaberLink part class)
For each part: its **pin roles** with requirements (Trig: digital out; Echo: digital in; motor PWM:
PWM-capable; encoder A: interrupt-capable), its **settings** (range, a battery's divider resistor
values, the motor's max speed), its **power** (voltage, typical current, "needs its own supply"),
the **wiring lines**, and the **code** it adds to the sketch (the declaration, plus any `begin()`).

A part template exists only for a class the FaberLink library has. A test fails if a library class
has no template, or a template names a class or constructor the library doesn't have (the same
kind of drift test as the editor's completion catalogue).

### The allocator (pure logic, heavily tested)
Given the board's pin map, the parts already in the plan and the new part's roles, it returns, per
role, the suggested pin, the suitable pins and every unsuitable pin with its reason. It also
re-checks the whole plan for conflicts (two parts on one pin, a Servo plus PWM on D9, I²C pins
used as GPIO).

## 5. The plan and the generated sketch

A guided board is a folder like any sketch, so it can be backed up, opened in Files and taken over
later:

```
home:/Arduino/mega/
  board.toml      # the plan: board type, parts, their pins and settings
  mega.ino        # generated from board.toml; marked "generated, don't edit"
  my_code.h       # yours: userSetup() and userLoop(), never overwritten
```

```toml
# board.toml
faber-board = 1
board = "mega2560"
name = "mega"

[parts.front]
template = "ultrasonic"
pins = { trig = 7, echo = 6 }
settings = { max-m = 4.0 }
```

The editor opens `mega.ino` read-only, with a bar reading "Faber writes this file from the board's
plan" and the buttons *Edit the plan* and *Take over the code*. `my_code.h` opens normally, so a
guided board can still blink an LED or read a button without leaving guided mode. The generated
sketch calls `userSetup()` and `userLoop()` (and checks they don't block; `delay()` in
`userLoop()` is flagged by Verify as a warning, because it starves the link).

## 6. "What's already connected", in both modes

- **Guided boards**: from `board.toml`, before anything is uploaded.
- **Code-mode boards**: the library knows each part's pins, so the board can tell Faber. This
  needs a small protocol addition: an optional **`PINS`** message after each `PART`
  (`part u8 · (role u8 · pin u8)*`), sent only by a library that has it. An older core ignores
  the message type, and an older library just doesn't send it. Faber then shows the pins a
  code-mode board uses in Parts and on the drawing, and the editor's completion could warn when a
  new declaration takes a pin already in use.
- **Boards without FaberLink**: Faber can't know, and says "Unknown: this board doesn't run
  FaberLink".

## 7. Where it lives

| Piece | Where |
|---|---|
| Pin maps, part templates, allocator, plan parse/write, sketch generator | `core/src/arduino/wiring/` (layer 2, pure; data files beside it) |
| Plan storage, upload, the live check | `core/src/machine/arduino.rs` (layer 4) |
| API | `GET /api/v0/arduino/boards/{type}/pins`, `GET /api/v0/arduino/parts` (templates), `POST /api/v0/arduino/plans/{board}/suggest` (allocator), `PUT /api/v0/arduino/plans/{board}` (save plan + generate), then the existing compile/upload |
| `PINS` message | `sdk/rust/faberlink` + `firmware/faberlink` + interop tests + faberlink.md |
| Shell | "Add part → On a board's pins" wizard, the pin drawing (an SVG drawn by Faber from the pin map, not Arduino's artwork), the wiring checklist window, the editor's read-only bar |

Access: reading pin maps and plans is for anyone; editing a plan is an **operator**, and uploading
is an **admin** (D53, as for the editor).

## 8. Switching modes

- **Guided → code**: "Take over the code" removes the generated mark and `board.toml` becomes a
  read-only note in the folder. From then on the board is code mode. This is one way, with a
  confirm; the point is that your edits are never overwritten.
- **Code → guided**: no conversion. Faber offers "Start a guided plan for this board", which
  begins empty or, for a FaberLink sketch whose board reports `PINS`, pre-filled from what the
  board reports. You review it, and nothing is uploaded until you confirm.

## 9. Building it (order)

1. Pin maps for the four boards, plus tests.
2. Part templates for the existing library classes (motor + H-bridge, IR distance, ultrasonic,
   battery), plus the drift test.
3. The allocator and its conflict rules, plus tests (the bulk of the logic).
4. Plan file and sketch generator: the generated sketch must compile with `--warnings all` for
   every template on every board (the fake toolchain in unit tests, real `arduino-cli` in the
   Arduino CI job).
5. API and the shell wizard, the pin drawing and the wiring checklist.
6. The `PINS` message (library, core, interop), and code-mode pin display.
7. New part classes in the library (servo, encoder, button, LED, an I²C sensor), each with a template.

## 10. Questions Mikkel answered (D57: all as recommended)

1. **`my_code.h` in guided mode** (recommended), or should guided boards be generated with no
   code of yours at all?
2. **The `PINS` message** so code-mode boards report their pins (recommended), or should only
   guided boards know their pins?
3. **Take over the code is one way** (recommended), or should Faber try to keep a hand-edited
   sketch in sync with the plan? Keeping them in sync means parsing C++, which is fragile.
4. **First part catalogue.** [research/common-parts.md](research/common-parts.md) ranks about 60
   parts. Its recommended first wave of 12: HC-SR04, Sharp IR, battery divider, DC motor with a
   driver, quadrature encoder, IMU (MPU6050 first), VL53L0X/L1X time of flight, button / switch /
   bumper, LED / relay / MOSFET output, servo, WS2812B strip, and potentiometer / joystick. That
   means eight new library classes, and two new capabilities, `line-sensor@1` and `obstacle@1`.
   Also: `FaberMotor` (PWM + DIR) fits none of the most common drivers (L298N, L293D and TB6612
   need EN + IN1 + IN2; L9110, DRV8833 and BTS7960 need two PWM pins), so motor templates need
   driver shapes. And `FaberUltrasonic` blocks for up to about 24 ms per reading, so several sonars
   should move to interrupt timing.
5. **Pi GPIO** (Phase 6b) could later use the same wizard, pin drawing and allocator, with the Pi's
   40-pin header as one more pin map. Should the design aim for that now? Recommended: yes, in the
   data format only.

## 11. After setup: configuring parts

Every part keeps its settings editable after it is set up: the part's page in Parts has a
**Settings** tab with a form built from the part template (a sonar's min and max range, a motor's
max speed and direction, a battery's divider resistors and chemistry, a button's debounce).

Two kinds of setting, and the form says which:
- **Live settings** change without an upload: they are sent to the board as a FaberLink command,
  or applied by the core (smoothing, a display unit). Most range limits and smoothing are live.
- **Build settings** are compiled into the sketch (pins, a sensor's hardware mode). Changing one
  shows the new wiring if it moved, and offers an upload.

For a code-mode board, only settings the board itself accepts at runtime are shown, because Faber
can't edit your sketch.

## 12. Next: automations (a separate design)

Mikkel wants a no-code way to make parts react, like Home Assistant's automations: **when** something
happens (a trigger: a distance drops below 30 cm, a button is pressed, the battery goes low, a
time, an app's signal), **if** conditions hold (the e-stop is released, it is daytime), **then**
do actions (stop the base, set an LED colour, notify, start an app, wait).

This is big enough for its own design (`docs/automations.md`, to come). Points that design must
settle:
- **Where it runs.** In the core, over capabilities, so it works with any part, real or simulated,
  and with apps. Actions that move things take a lease like any program and respect the e-stop and
  D46. A later option: compile simple, safety-relevant rules ("bumper pressed → stop motors") into
  a guided board's sketch as **board reflexes**, so they still act within milliseconds when the Pi
  or the link is down.
- **How it relates to apps.** An automation is a small, declarative program with no code. It should
  probably run as an app (its own leases, logs and on/off switch), so there is one way to run
  things.
- **The editor.** A visual trigger/condition/action builder with the same pickers as the rest of
  Faber (parts, signals, values with units), and a YAML/TOML view for those who want text.
- **Safety.** Automations that move things are off until turned on and confirmed, and they show
  in the Stop bar while they drive.

## 13. As built: the core (steps 1–4 of §9, and the API of step 5)

What exists, and where it differs in detail from the design above. The decisions stand.

- **Where.** `core/src/arduino/wiring/` (layer 2): `pins.rs` and `boards/*.toml` (pin maps),
  `parts.rs` and `parts/*.toml` (templates), `allocator.rs`, `plan.rs` (`board.toml`) and
  `generate.rs` (the sketch and `my_code.h`). Plans on disk and the API: `core/src/machine/wiring.rs`
  and `core/src/api/arduino.rs`; the endpoints are in [api.md](api.md#guided-boards).
- **Pin maps** for `uno`, `nano` (A6/A7 analog only; `nano-old` uses the same file) and `mega2560`,
  with every value cited to the Arduino AVR core 1.8.8 (`variants/*/pins_arduino.h`), the Servo
  library's `ServoTimers.h` and the core's `Tone.cpp`; tests check the files against those
  headers' macros, and against the installed headers' timer tables when the Arduino IDE is on the
  computer. The **Pi header** is a pin map too (`pi-header`), in the data format only: its
  physical pins come from the port manager's header table (the one `GET /api/v0/ports/header`
  serves) and `boards/pi-header.toml` adds what each line can do; no plan can be saved for it yet.
  The Mega's on-board I²C pull-ups (research §5, "verify") are left out until checked.
- **The API path** for a pin map is `GET /api/v0/arduino/pins/{board}`, not
  `/arduino/boards/{type}/pins`: the router can't hold that next to `/arduino/boards/{port}/code`.
- **Templates** exist for the four library classes. The infrared one is the Sharp GP2Y0A21 with
  the curve from the library's example; other Sharp models need their own curve and template.
  The motor has all three driver shapes, written against `FaberMotor`'s driver constructor
  (`faberlink::DRIVER_EN_IN1_IN2` with an optional STBY, `faberlink::DRIVER_TWO_PWM` with an
  optional enable, and `setMaxVelocity()` in `setup()`); the two new ones stay marked `todo`, and
  their drift test ignored, until that library change is merged into this branch. A role can be
  `optional` (STBY, enable): it is left out of the call when it has no pin, so it comes last. Role
  ids are the `PINS` message's role names (`pwm`, `dir`, `en`, `in1`, `in2`, `stby`, `trig`,
  `echo`, `signal`, `sense`), so a plan and a code-mode board's report speak of pins the same way. Every setting of these four is a **build** setting: the library takes
  them in constructors and has no runtime settings yet, so nothing is `live`.
- **Pin providers**: a template's `provides` adds pins named `<part>.<pin>` (`servos.ch3`) while
  that part is in the plan; only a role marked `provided` may take one. No provider template
  exists yet (tested with a test PCA9685).
- **The allocator** suggests the cheapest free pin per role: pins with abilities the role doesn't
  need (an interrupt, PWM, an analog input, a serial port or a bus) cost more, as do the pins a
  first servo or `tone()` would take, pins noted to avoid (D13 as an input), and distance from the
  part's other pins. Roles with the fewest choices go first. Its checks and their codes are in
  [api.md](api.md#guided-boards). Pins in use from outside the plan come in as `ExternalUse`: the
  hook for the `PINS` message (§6), empty until boards report their pins.
- **`board.toml`**: a pin is written as its number for a plain digital pin (`trig = 7`) and as its
  name otherwise (`out = "A0"`); both are read. Every setting is written out, so a later change
  of a template's default never changes a saved board. Faber also keeps `sketch-sha256`, the hash
  of the sketch it last wrote: a sketch changed outside Faber is reported (`edited`) and only
  overwritten when asked (`409 sketch-edited`). Taking over adds `taken-over` and a comment.
- **The sketch**: `<name>.ino` starts with the generated mark and the wiring, then the includes,
  `FaberLink link(Serial);`, each part's declaration (and helper, like the Sharp curve), then
  `#include "my_code.h"` (so your code can use the parts), `setup()` with
  `link.begin(115200, "<name>", "guided")` and `userSetup()`, and `loop()` with `link.poll()` and
  `userLoop()`. Golden files for every template on every board are in `fixtures/wiring/`; the
  real `arduino-cli` compiles them with `--warnings all` in a test when the computer has one (it
  did, with no warnings, on 2026-10-16) and in CI's Arduino job.
- **Not built yet**: the shell (wizard, pin drawing, wiring checklist, the editor's read-only bar),
  the `PINS` message (§6), Verify's warning for `delay()` in `userLoop()` (§5), the live check
  after upload (§3 step 6), and the new library classes of §9 step 7.
