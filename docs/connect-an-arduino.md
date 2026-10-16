# Connect an Arduino

Turn an Arduino into a **bridge**: a board that runs the FaberLink library, so its motors and
sensors show up in Faber as parts you can view and drive. You write a short sketch that says what
is wired where; the library does the talking, and it stops the motors on its own if Faber goes
quiet. The protocol and the safety rules are in [faberlink.md](faberlink.md).

This guide uses the library's example, `MotorAndDistance`: one DC motor and one analog distance
sensor. For an HC-SR04 ultrasonic sensor, use the example `Ultrasonic` instead (wiring in the
sketch: Trig to pin 7, Echo to pin 6).

**Tried on a real board** (2026-10-16): an Arduino Mega 2560 on COM6 with `MotorAndDistance` and
nothing wired. "Check for a FaberLink board" found it, its parts appeared and ran, and after the
board was reset it showed Failed for a moment and came back by itself.

**Tested or not.** Every step marked *(tested)* is checked on every CI run against the library's
real C++ code behind a fake serial port (`core/tests/bridge.rs`, `tools/faberlink-interop`) or by
compiling the example for an Uno and a Mega. Steps marked *(needs hardware)* have not yet been
checked with a real board on a real USB port. Faber's own [Arduino flasher](#flash-from-faber) is
tested against simulated bootloaders only: it still needs a run on a real Uno and a real Mega.

## What you need

- An Arduino: Uno, Nano, Mega or similar, and its USB cable.
- A DC motor on an H-bridge (motor driver) with a PWM input for speed and an input for direction,
  e.g. an L298N or a TB6612, with its own motor power supply.
- An analog distance sensor, here a Sharp GP2Y0A21 infrared sensor (10–80 cm).
- The Arduino IDE, once, to compile the sketch and upload it. Or compile it there and put it on
  the board with Faber's own [Arduino flasher](#flash-from-faber).
- A computer running Faber.

## Wiring

| From | To |
|---|---|
| H-bridge PWM (speed) | pin 9 |
| H-bridge DIR (direction) | pin 8 |
| H-bridge GND | GND (shared with the motor supply's GND) |
| Sensor output | A0 |
| Sensor 5 V and GND | 5V and GND |

The motor's own power goes to the H-bridge, never through the Arduino.

## Steps

1. **Install the library.** Copy the folder `firmware/faberlink` from the Faber repository into
   your Arduino libraries folder (`Documents/Arduino/libraries/`) and rename it `FaberLink`.
   Restart the Arduino IDE. *(needs hardware: the IDE side; the folder is a complete Arduino
   library, and CI compiles its examples)*
2. **Open the example**: File → Examples → FaberLink → MotorAndDistance. If your motor's top speed
   is known, put it in rad/s in `FaberMotor left("left", 9, 8, 15.0)`; then Faber can drive it by
   speed as well as by duty (the Drive window drives by speed, so it needs this number). *(tested: compiles for an Uno and a Mega)*
3. **Upload it** to the board (Sketch → Upload). Then **close the Arduino IDE's Serial Monitor**:
   only one program can use the port at a time. *(needs hardware)* Uploading again later, once the
   board is a part in Faber? **Pause** it in Parts first, or the IDE says "Access denied"; see
   [If it doesn't work](#if-it-doesnt-work). Or **flash it from Faber**
   instead, see [Flash from Faber](#flash-from-faber). *(tested on simulated bootloaders only;
   needs hardware)*
4. **Find the board in Faber.** Open Parts → Add part. Under the board's port, press **Check for
   a FaberLink board**. Faber opens the port, says hello and listens for a few seconds; it only
   does this when you ask, because opening a port restarts most Arduinos. It answers "FaberLink
   board: motor-and-distance (faberlink-arduino 0.1.0)" and picks the port, the **FaberLink board**
   connector and a name. *(tested with a fake board; the real USB port needs hardware)*
5. **Add it.** Change the name if you like (say `arduino`) and press Add part. *(tested)*
6. **Drive the motor.** In Parts, the board's parts appear under it: `arduino.left` (the motor) and
   `arduino.front` (the sensor). Press **Drive** on `arduino.left`. Faber asks once whether to
   start driving; then the pad (or the arrow keys) drives the motor while you hold it: up is
   forward, down is backward, and the Speed slider sets how fast. *(tested through the API with
   a fake board; the Drive window itself is tested with a stand-in core)*
7. **Stop it.** Let go, close the Drive window, or press the red **Stop** on the taskbar: the motor
   stops. *(tested)*

## What you should see

- In Parts: `arduino` with "Connected" and the board's name and firmware, and below it
  `arduino.left` and `arduino.front`, each Running.
- Opening `arduino.front` shows the distance as a bar and a plot, about 20 times a second.
- A program can drive the motor too: take the lease with
  `POST /api/v0/parts/arduino.left/lease` and send commands
  (`{"capability": "motor@1", "command": {"kind": "set-duty", "duty": 0.5}}`); see
  [api.md](api.md#driving-leases-commands-and-the-e-stop). If the program stops sending, the motor
  stops within a quarter of a second. *(tested)*

## If it doesn't work

| What you see | What to try |
|---|---|
| "No FaberLink answer from COM5" | Is the FaberLink sketch uploaded? Is the baud 115200 (`link.begin(115200, …)`)? Faber keeps asking, so fixing it is enough. |
| "Could not open COM5" | Close the Arduino IDE's Serial Monitor (or any other program using the port), then wait: Faber retries. |
| The Arduino IDE can't upload: "Access denied" (on Danish Windows "Adgang nægtet"), or "can't open device" | Faber holds the port while the board is a part. In Parts, press **Pause** on the board: it stops, its motors stop, and the port is free. Upload, then press **Resume**. A paused board stays paused (also when you unplug it or restart Faber) until you resume it. *(tested with a fake board; the IDE side needs hardware)* |
| The Serial monitor says the port "is used by the part …" | The same: pause the board from the message's **Pause** button (or in Parts), and resume it when you're done. |
| "speaks FaberLink version …, but this Faber speaks …" | Use the FaberLink library from the same Faber version, and upload again. |
| "The board stopped answering" | Check the USB cable and the board's power. Keep `loop()` free of `delay()`: `link.poll()` must run often. |
| "The board stopped its motors: Faber's heartbeat didn't reach it in time" | A loose cable, or a very busy computer. The motor starts again with the next command. |
| A part shows "Not on the board" | parts.toml names a part the sketch doesn't have: check the name in the sketch, or remove that part in Parts. |
| The e-stop won't release: "The stop button on the board … is still pressed" | The board's own stop button is latched. Reset it on the board, then release the e-stop in Faber. |
| The motor turns the wrong way | Swap the motor's two wires on the H-bridge. |

Don't print to `Serial` in the sketch: the port belongs to FaberLink. The library's `link.log()`
sends a line to Faber's log instead.

## Flash from Faber

Faber can put the compiled sketch on the board itself, over the board's USB cable, so the board can
be plugged into the robot's computer and never into the PC with the Arduino IDE. It talks to the
bootloader every Arduino comes with: Optiboot on the Uno and Nano, and the Mega 2560's own. It works
for any sketch, not only FaberLink ones.

**Tested or not.** The flasher is tested on every CI run against simulated bootloaders, written
from Atmel's protocol descriptions (AVR061 for the Uno and Nano, AVR068 for the Mega), through the
API with a fake serial port: write, read back and compare, a board that doesn't answer, a wrong
chip, a board that stops in the middle, a bad flash cell and damaged answers. **It has not yet been
tried on a real board** (*needs hardware*): until it has, the Arduino IDE's Upload is the safe way.

1. **Compile the sketch to a file.** In the Arduino IDE: Sketch → Export Compiled Binary. The IDE
   writes two `.hex` files next to the sketch; take the one **without** `with_bootloader` in its
   name, e.g. `MotorAndDistance.ino.hex`. Copy it into Faber's Files (drag it onto Files in the
   browser), e.g. into Documents. A connector package may also bring its own firmware; then there
   is nothing to compile.
2. **Open the flasher**: Start → Arduino flasher.
3. **Pick the port.** An official Uno or Mega is recognised by its USB ids, and the board is
   picked for you. A clone with a CH340 or FT232 USB chip can't be told apart from other boards:
   pick the board yourself (an old Nano clone is often "Arduino Nano (old bootloader)").
4. **Pick the program**: "A .hex file from Files" and Choose…, or "Firmware from a package".
5. **Flash.** Faber warns that this replaces the program on the board and asks once more. Then it
   resets the board, waits for the bootloader, checks that the chip is the board you picked,
   writes the program and reads it back to compare. The bar shows writing, then checking. Keep the
   board plugged in until it says done.
6. **Find the board.** If the board was already added as a FaberLink board, it starts again by
   itself on the new firmware. If not, press **Check for a FaberLink board** (after a package's
   firmware), or go on with step 4 above in Parts → Add part.

Faber never flashes on its own, and it never flashes a port a part is using. A port that a
FaberLink board in Parts is using is refused first; **Stop the board and flash** stops it (its
motors stop), flashes, and lets it start again on the new firmware.

| What you see | What to try |
|---|---|
| "The board didn't answer the bootloader." | Press the reset button on the board just as you press Flash (some clones don't reset by themselves). Check that you picked the right board: an Uno and a Mega speak different bootloader languages. Nothing was written. |
| "This board has an ATmega168 …, but an Arduino Uno has an ATmega328P" | The board isn't the one you picked (here an old Nano). Pick the right board, with a sketch compiled for it. Nothing was written. |
| "The program is … bytes, but the Arduino Uno has room for 32256." | The sketch was compiled for a bigger board, or is too big. Compile it for this board. |
| "The firmware file can't be used: line 12 is damaged …" | The `.hex` file is broken or cut off. Export it again. |
| "The board stopped answering while writing the program." | A loose cable. Flash again: the bootloader is still there, so a half-written program is fixed by flashing again. |
| "Reading the program back found a difference at address …" | Flash again. If it keeps failing at the same address, the chip's flash may be worn out. |
| "… is open in the serial monitor" or "… is used by the part …" | Close the serial monitor, or remove the part (or change its port), then flash. |

## The code, explained

```cpp
#include <FaberLink.h>

FaberLink link(Serial);   // FaberLink talks over the USB serial port

// A DC motor on an H-bridge: PWM on pin 9, direction on pin 8, 15 rad/s at full duty. The name
// "left" becomes the part arduino.left in Faber.
FaberMotor left("left", 9, 8, 15.0);

// The Sharp sensor on A0, 0.1–0.8 m. sharpToMeters (in the example) turns the reading into metres.
FaberDistance front("front", A0, 0.1, 0.8, faberlink::DISTANCE_INFRARED, sharpToMeters);

void setup() {
  // The speed, the board's name and the sketch's version: Faber shows the name and version.
  link.begin(115200, "motor-and-distance", "0.1.0");
}

void loop() {
  link.poll();   // reads Faber's messages, sends readings, and runs the safety checks
}
```

Why there is so little: the library already does the safety work, on the board, where it still
works if the computer freezes. The motor starts stopped. It stops by itself 250 ms after its last
command, and when Faber's heartbeat stops for 250 ms. It never starts again on its own: only a new
command does. While Faber's e-stop is engaged, every command is refused.

Where the board's parts sit on the robot is set in parts.toml, under the board:

```toml
[parts.arduino]
connector = "faber-bridge"
port = "usb:2341:0043"

[parts.arduino.parts.front]
mount = { x = 0.35, z = 0.1 }   # metres from the robot's centre: forward, left, up
```
