# NMEA GPS: a GPS module as `position@1`, without writing a connector

A **declarative connector** for GPS modules that speak NMEA 0183, the text format nearly every
GPS module sends (u-blox NEO-6M, NEO-M8N, BN-220, …). The whole connector is
[`faber.toml`](faber.toml): `line = { format = "nmea" }` hands the lines to Faber's built-in NMEA
reader, which gives `position@1` readings: latitude, longitude, altitude, fix, satellites, and
speed and course over ground.

How to write your own: [docs/connectors-declarative.md](../../../docs/connectors-declarative.md).

## What you need

- A GPS module with a serial (UART) output, such as a NEO-6M breakout, and a USB serial adapter
  (3.3 V logic), or a module with USB built in.
- A view of the sky. Indoors most modules never get a fix.
- A Faber core. No hardware? Use the recording in [`captures/`](captures/) (below).

## Wiring

| GPS module | USB serial adapter | Why |
|---|---|---|
| VCC | 3.3V or 5V (see the module's board) | Most breakouts have a regulator and take 5 V. |
| GND | GND | A shared ground. |
| TX | RX | The module talks, Faber listens. |
| RX | (not needed) | Faber doesn't configure the module. |

## Steps

1. **Install the connector.** Copy this folder to `connectors/nmea-gps/` in Faber's data folder
   (`./data` when you run `faber-core` from a checkout) and restart the core. Until packages
   arrive (Phase 3), installing is copying the folder.
2. **Add the part.** Plug in the adapter, open **Add part**, pick *NMEA GPS module* and its port.
3. Put the antenna where it sees the sky and wait. A cold start can take a minute or more.

**Without the hardware:** open Files and upload `captures/nmea-gps.fbcap` into **Recordings**.
It becomes the port `replay:nmea-gps`; pick that port in step 2.

## What you should see

The part turns *running* as soon as the first good sentence arrives, even before a fix. Until
the module has a fix, readings say `fix: none` (ignore their latitude and longitude). Then the
fix becomes `fix3d` (or `fix2d`, `dgps`, …), with the position, the number of satellites, and
the speed and course while moving.

The recording is ten seconds: two without a fix, then a walk east across Copenhagen's City Hall
Square at about 0.6 m/s.

## If it doesn't work

- **"Nothing has arrived from the sensor for 5 s"**: check that the module's TX goes to the
  adapter's RX, and that the module has power (most have an LED that blinks once a second when
  they have a fix).
- **"The last 10 lines … couldn't be read"**: usually the baud rate. Try 9600, then 38400 or
  115200, in `faber.toml`. A line ending in "it was damaged on the way" means its checksum
  didn't add up: check the wiring and the ground.
- **`fix: none` forever**: the module can't see enough satellites. Go outside or near a window.

## The connector, explained

```toml
[ports]
serial = { baud = 9600 }      # the module's baud rate

[lines]
max-length = 100              # NMEA sentences are at most 82 characters
timeout-s = 5                 # silence for 5 s marks the part as failed

[[provides]]
capability = "position@1"
line = { format = "nmea" }    # Faber's built-in NMEA reader
```

The reader checks every sentence's checksum (`*hh` at the end). It reads three sentences:

| Sentence | Gives |
|---|---|
| GGA | latitude, longitude, altitude, fix quality, satellites. One reading per GGA. |
| RMC | speed and course over ground, used by the GGA of the same second. |
| GSA | whether a plain GPS fix is 2D or 3D. |

Other sentences (GSV, VTG, GLL, TXT) are fine and simply not used. A module that sends no GGA at
all (two RMCs in a row) gets one reading per RMC instead. The time of a reading is when its last
byte arrived at the core, not the GPS time in the sentence.

## Tested

`core/tests/declarative.rs` loads this `faber.toml` and plays `captures/nmea-gps.fbcap` through
Faber's engine, then checks the readings, on every build. The recording is **generated**, not
recorded from a real module: the test writes realistic NEO-6M sentences (RMC, VTG, GGA, GSA,
GSV, GLL each second) with correct checksums.
