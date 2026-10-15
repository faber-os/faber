# Connect an Arduino

Turn an Arduino into a **bridge**: a board that runs the FaberLink library, so its motors and
sensors show up in Faber as parts you can view and drive. You write a short sketch that says what
is wired where; the library does the talking, and it stops the motors on its own if Faber goes
quiet. The protocol and the safety rules are in [faberlink.md](faberlink.md).

This guide uses the library's example, `MotorAndDistance`: one DC motor and one analog distance
sensor.

**Tested or not.** Every step marked *(tested)* is checked on every CI run against the library's
real C++ code behind a fake serial port (`core/tests/bridge.rs`, `tools/faberlink-interop`) or by
compiling the example for an Uno and a Mega. Steps marked *(needs hardware)* have not yet been
checked with a real board on a real USB port.

## What you need

- An Arduino: Uno, Nano, Mega or similar, and its USB cable.
- A DC motor on an H-bridge (motor driver) with a PWM input for speed and an input for direction,
  e.g. an L298N or a TB6612, with its own motor power supply.
- An analog distance sensor, here a Sharp GP2Y0A21 infrared sensor (10–80 cm).
- The Arduino IDE (only once, to upload the sketch).
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
   only one program can use the port at a time. *(needs hardware)*
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
| "speaks FaberLink version …, but this Faber speaks …" | Use the FaberLink library from the same Faber version, and upload again. |
| "The board stopped answering" | Check the USB cable and the board's power. Keep `loop()` free of `delay()`: `link.poll()` must run often. |
| "The board stopped its motors: Faber's heartbeat didn't reach it in time" | A loose cable, or a very busy computer. The motor starts again with the next command. |
| A part shows "Not on the board" | parts.toml names a part the sketch doesn't have: check the name in the sketch, or remove that part in Parts. |
| The e-stop won't release: "The stop button on the board … is still pressed" | The board's own stop button is latched. Reset it on the board, then release the e-stop in Faber. |
| The motor turns the wrong way | Swap the motor's two wires on the H-bridge. |

Don't print to `Serial` in the sketch: the port belongs to FaberLink. The library's `link.log()`
sends a line to Faber's log instead.

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
