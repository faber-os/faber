# My sensor

A Faber connector for a sensor that prints one line per measurement over serial, like `D:123`
for 123 cm. Faber turns each line into a `distance@1` reading, so the Distance viewer and every
app work with it.

This text is shown in Faber's install dialog. Describe your device here.

## What you need

- The sensor, and a USB serial adapter or an Arduino that prints its lines.
- A Faber machine.

## Wiring

| Sensor | Adapter | Why |
|---|---|---|
| TX | RX | The sensor talks, Faber listens. |
| GND | GND | A shared ground. |
| VCC | 5V or 3.3V | Check your sensor's datasheet. |

## Steps

1. Install `my-sensor-0.1.0.faberpkg`: double-click it in Files.
2. Open **Parts**, click **Add part**, pick **My sensor** and its port.

## What you should see

The part turns green and its Distance viewer shows the distance.

## If it doesn't work

- **The part says lines can't be read:** the baud rate or the line format is wrong. Check
  `serial` and `line` in `faber.toml` against your device.
- **Nothing arrives:** check the wiring (TX to RX), and that you picked the right port.
