# TI TMP102: an I²C temperature sensor, without writing a connector

A **declarative connector** for an I²C sensor: the whole connector is
[`faber.toml`](faber.toml). Faber reads the sensor's temperature register once a second and
publishes `temperature@1` readings in °C, so the Temperature viewer, the Signal plotter and every
app work with it.

How to write your own: [docs/connectors-declarative.md](../../../docs/connectors-declarative.md),
section "I²C registers".

## What you need

- A Raspberry Pi with I²C switched on (`sudo raspi-config` → Interface Options → I2C).
- A TMP102 breakout board (SparkFun and others sell them) and four jumper wires.
- No Pi at hand? Start the core with `--fake-hardware`: it has a simulated TMP102 at 0x48 on
  `i2c-1`, and everything below works the same.

## Wiring

```
TMP102 breakout       Raspberry Pi header
  VCC  ─────────────── pin 1  (3.3 V)
  GND  ─────────────── pin 6  (GND)
  SDA  ─────────────── pin 3  (GPIO2, I²C 1 SDA)
  SCL  ─────────────── pin 5  (GPIO3, I²C 1 SCL)
  ADD0 ─────────────── GND    (address 0x48)
```

The TMP102 runs on 1.4–3.6 V: use the Pi's **3.3 V**, never 5 V. Most breakouts already tie ADD0
to ground and have pull-up resistors on SDA and SCL.

## Steps

1. Install the connector: copy this folder to Faber's data folder as `connectors/ti-tmp102/`
   and restart the core (or install it as a package).
2. Open **I2C scanner** and press **Scan**. The sensor answers at `0x48`, and the scanner suggests
   the TMP102 for it.
3. Press **Add part**, give it a name (`cabin_temp`), and add it. Its temperature appears in Parts;
   open it in the Temperature viewer or the Signal plotter.

## What the file says, and where it comes from

Every number is from the TI datasheet, **SBOS397** ("TMP102 Low-Power Digital Temperature Sensor
With SMBus and Two-Wire Serial Interface"):

| Key | Value | From the datasheet |
|---|---|---|
| `addresses` | 0x48, 0x49, 0x4A, 0x4B | The address pin ADD0 wired to GND, V+, SDA or SCL. |
| `default` | 0x48 | ADD0 to GND, what most breakouts do. |
| `register` | 0x00 | The pointer register selects the temperature register with 0x00 (0x01 is the configuration register, 0x02 T(LOW), 0x03 T(HIGH)). |
| `bytes`, `format` | 2, `i16be` | The temperature comes as two bytes, the most significant first, in two's complement. |
| `shift` | 4 | In the normal 12-bit mode the value is left-justified: the lowest 4 bits are 0. |
| `scale` | 0.0625 | One bit is 0.0625 °C. |
| `unit` | `celsius` | The reading field it fills: `temperature@1`'s `celsius`. |
| `default-hz` | 1 | Read once a second. At power-up the sensor converts 4 times a second (configuration `0x60A0`), so each read is fresh. |

So `0x19 0x00` is 0x1900 = 6400, shifted by 4 is 400, times 0.0625 is **25.0 °C**; `0xE7 0x00` is
−6400 → −400 → **−25.0 °C**.

The connector never writes to the sensor: it leaves the configuration at its power-on values. In
**extended mode** (13 bits, up to 150 °C) the format changes; this connector doesn't switch it on.

## Tested on every build

`core/tests/declarative.rs` runs this file against the fake I²C bus's TMP102 model, which encodes
temperatures exactly as the datasheet's tables do, and checks that the part reads the right °C.
