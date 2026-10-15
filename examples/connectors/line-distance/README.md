# Line distance: an Arduino and an HC-SR04, without writing a connector

A **declarative connector**: the whole connector is [`faber.toml`](faber.toml). An Arduino
measures the distance with an HC-SR04 ultrasonic sensor and prints one line per measurement,
like `D:123` for 123 cm. Faber reads those lines and turns them into `distance@1` readings, the
same readings any other distance sensor gives, so the Distance viewer and every app work with it.

How to write your own: [docs/connectors-declarative.md](../../../docs/connectors-declarative.md).

## What you need

- An Arduino Uno (or any board that prints over USB serial).
- An HC-SR04 ultrasonic distance sensor and four jumper wires.
- A Faber core. No hardware at all? Use the recording in [`captures/`](captures/) (below).

## Wiring

```
HC-SR04          Arduino Uno
  VCC  ────────── 5V
  Trig ────────── pin 9
  Echo ────────── pin 10
  GND  ────────── GND
```

| HC-SR04 | Arduino | Why |
|---|---|---|
| VCC | 5V | The HC-SR04 needs 5 V. |
| Trig | 9 | The Arduino starts a measurement here. |
| Echo | 10 | The sensor answers with a pulse as long as the sound took. |
| GND | GND | A shared ground. |

## Steps

1. **Upload this sketch** to the Arduino:

   ```cpp
   const int TRIG = 9;
   const int ECHO = 10;

   void setup() {
     Serial.begin(9600);      // faber.toml: serial = { baud = 9600 }
     pinMode(TRIG, OUTPUT);
     pinMode(ECHO, INPUT);
   }

   void loop() {
     // A 10 µs pulse on Trig starts one measurement.
     digitalWrite(TRIG, LOW);
     delayMicroseconds(2);
     digitalWrite(TRIG, HIGH);
     delayMicroseconds(10);
     digitalWrite(TRIG, LOW);

     // Echo stays high for as long as the sound took there and back: 58 µs per cm.
     // No echo within 30 ms (about 5 m) gives 0; print 999 then, which Faber shows as too far.
     unsigned long us = pulseIn(ECHO, HIGH, 30000);
     Serial.print("D:");
     Serial.println(us == 0 ? 999 : us / 58);   // println ends the line with \r\n

     delay(100);              // 10 measurements a second
   }
   ```

2. **Install the connector.** Copy this folder to `connectors/line-distance/` in Faber's data
   folder (`./data` when you run `faber-core` from a checkout), so the file is
   `data/connectors/line-distance/faber.toml`, and restart the core. Until packages arrive
   (Phase 3), installing is copying the folder.
3. **Add the part.** Plug in the Arduino, open **Add part**, pick *Arduino distance printer
   (HC-SR04)* and the Arduino's port.

**Without the hardware:** open Files and upload `captures/line-distance.fbcap` into
**Recordings**. It becomes the port `replay:line-distance`; pick that port in step 3.

## What you should see

The part turns *running* as soon as the first line arrives. Its Distance viewer shows the
distance in metres. Hold your hand closer than 2 cm and it says *too close*; point it at the
ceiling far away and it says *too far*.

The recording plays a hand moving towards the sensor and away again: 1.20 m down to 0.30 m and
back, one too-close moment, and one too-far one.

## If it doesn't work

- **"Nothing has arrived from the sensor for 5 s"**: wrong port, or the Arduino isn't running
  the sketch. Open the port in the serial monitor: you should see `D:` lines.
- **"The last 10 lines from the sensor couldn't be read"**: the lines don't look like `D:123`.
  Check the baud rate (9600 on both sides) and that the sketch prints exactly `D:` and a number.
- **The connector isn't in Add part**: the core's log says why (a mistake in `faber.toml` is
  reported with its line). Check that the folder is called `line-distance`, like the `id`.

## The connector, explained

```toml
faber-manifest = 1                       # the faber.toml format this is written in
```

`[package]` says what it is: its `id` (also the folder's name), that it is a `declarative`
connector, and the name people see.

```toml
[ports]
serial = { baud = 9600 }                 # the same as Serial.begin(9600)

[lines]
ending = "\r\n"                          # what Serial.println ends a line with
max-length = 32                          # longer lines are noise, counted as bad lines
```

```toml
[[settings]]
key = "max-m"                            # a setting each part can change in Add part
...

[[provides]]
capability = "distance@1"
info = { min-m = 0.02, max-m = { setting = "max-m" }, field-of-view-rad = 0.26, kind = "ultrasonic" }
line = { pattern = "D:{meters:u32}", scale = 0.01 }
```

- `info` is what the sensor can do, from its datasheet. `{ setting = "max-m" }` takes the value
  from the part's own setting.
- The **pattern** matches `D:` and then reads a whole number into the reading's `meters` field.
  `scale = 0.01` turns centimetres into metres.
- A `distance@1` reading also has a `status`. The pattern doesn't fill it, so Faber works it out
  from `info`: below 2 cm is *too close*, above `max-m` is *too far*.
- Lines that don't match (the Arduino's start-up noise, a cut-off line) are counted as bad lines.
  A few are fine; ten in a row mark the part as degraded, with the latest bad line shown.

## Tested

`core/tests/declarative.rs` loads this `faber.toml` and plays `captures/line-distance.fbcap`
through Faber's engine, then checks every reading, on every build. The recording is written by
that test from a list of known lines (`D:120` … with a little noise), not recorded from a real
sensor.
