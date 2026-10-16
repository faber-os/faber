# Writing a declarative connector

Status: serial devices that send text lines (Phase 3c, 2026-10-15) and I²C sensors read
register by register (Phase 6b, section 10). Design:
[packages.md §6](packages.md#6-the-declarative-engine-first-cut).

A **declarative connector** is a connector without code: one `faber.toml` file that says which
port the part uses, what its lines look like, and which capability they become. Faber's engine
reads the lines and publishes the readings, exactly like a built-in connector, so viewers and
apps can't tell the difference. Nothing in the file can run code, so it is safe to install
(docs/architecture.md §4).

It is the easiest kind of connector, the next step after printing values from an Arduino. Move
up to a **wasm** connector when the device speaks binary packets, needs commands sent to it, or
needs logic a pattern can't express (packages.md §2).

Three complete, tested examples:

- [`examples/connectors/line-distance`](../examples/connectors/line-distance/): an Arduino with an
  HC-SR04 printing `D:123`. Start here.
- [`examples/connectors/nmea-gps`](../examples/connectors/nmea-gps/): a GPS module, through the
  built-in NMEA reader.
- [`examples/connectors/ti-tmp102`](../examples/connectors/ti-tmp102/): a TMP102 temperature
  sensor on the Pi's I²C bus, read register by register (section 10).

## 1. A first connector

An Arduino prints `D:123` (centimetres) ten times a second. The whole connector:

```toml
faber-manifest = 1

[package]
id = "my-ranger"
type = "connector"
kind = "declarative"
version = "0.1.0"
name = "My ranger"

[ports]
serial = { baud = 9600 }

[[provides]]
capability = "distance@1"
line = { pattern = "D:{meters:u32}", scale = 0.01 }
```

**Install it** by putting the file in Faber's data folder as
`connectors/my-ranger/faber.toml` (the folder is named after the `id`) and restarting the core.
It then shows up in Add part, and in `GET /api/v0/connectors` with `"kind": "declarative"`.
Until the package installer arrives (packages.md §5), installing is copying the folder.

## 2. The file, key by key

Every key is checked. A key Faber doesn't know is an error, not ignored, so a typo can't
silently do nothing.

### `faber-manifest`

The version of this file format. Write `faber-manifest = 1` as the first line. A newer Faber
still reads format 1; an older Faber refuses a newer format and says so.

### `[package]`

| Key | Meaning |
|---|---|
| `id` | Lowercase words joined by `-`, starting with a letter: `acme-ranger`. Also the folder's name. Can't be the id of a built-in connector. |
| `type` | Always `"connector"`. |
| `kind` | `"declarative"`. (`"wasm"` files are read by the wasm engine instead.) |
| `version` | Three numbers: `"0.1.0"`. |
| `name` | What people see in Add part. |
| `publisher` | Optional: who made it, e.g. `"Ada Lovelace"`. Shown in the install dialog. |

### `[ports]`

`serial = { baud = 9600 }`: the part uses one serial port (UART or USB serial) at this baud rate,
the same as `Serial.begin(9600)` on an Arduino. Or `i2c = { addresses = [0x48, 0x49], default = 0x48 }`:
the part is one device on an I²C bus (section 10). SPI is not declarative yet.

### `[lines]` (optional)

| Key | Default | Meaning |
|---|---|---|
| `ending` | `"\n"` | What ends a line: `"\n"`, `"\r\n"` or `"\r"`. With `"\n"`, a `\r` before it is dropped too, so `Serial.println` (`\r\n`) works either way. |
| `max-length` | `256` | Longest line in bytes. A longer one is a bad line, and its bytes are dropped up to its ending. |
| `warn-after-bad-lines` | `10` | This many bad lines in a row mark the part as degraded (section 6). |
| `timeout-s` | `5` | No good line for this long marks the part as failed. |

Spaces at the start and end of a line are ignored, and empty lines are skipped.

### `[[settings]]` (optional)

Settings a part of this kind takes, shown in Add part. The same declarations as built-in
connectors (architecture.md §4, "Settings"):

```toml
[[settings]]
key = "max-m"
label = "Farthest it can measure"
description = "Anything farther reads as too far."
kind = "number"          # number, bool, choice, string or part-list
unit = "m"
default = 4.0
min = 0.0
min-exclusive = true     # also: max, suggested, advanced = true
```

A `choice` has `choices = [{ value = "a", label = "A" }, …]`. Use a setting anywhere a value is
allowed in `info`, `set`, `scale` or `offset` by writing `{ setting = "max-m" }`.

### `[[provides]]`

One table per signal the part publishes. Several may read the same stream.

| Key | Meaning |
|---|---|
| `capability` | The capability and its version: `"distance@1"`. |
| `name` | The signal's name, when there is more than one of a capability (`"front"`, `"rear"`). Default: the capability's name. |
| `info` | What the sensor can do, once (docs/capabilities.md rule 4), from its datasheet. Optional. |
| `line` | How to read it: a `pattern`, or `format = "nmea"`. For a serial sensor. |
| `read`, `poll` | Which register to read, and how often. For an I²C sensor (section 10). |

## 3. Patterns

A pattern is the line as the device prints it, with the values replaced by **fields** in braces:

```toml
line = { pattern = "T={celsius:f32} C" }       # T=21.5 C
line = { pattern = "R,{meters:u32},{_},{status-code:u32}" }
```

| Field | Reads | Example text |
|---|---|---|
| `{name:f32}` | a decimal number | `-1.25`, `3e2` |
| `{name:i32}` | a whole number | `-12` |
| `{name:u32}` | a whole number of 0 or more | `40000` |
| `{name:hex}` | a whole number in hexadecimal, `0x` optional | `1F`, `0x1f` |
| `{_}` | anything, skipped | |

- Everything outside braces must appear exactly. `{{` and `}}` are literal braces.
- A pattern matches the **whole** line.
- A field reads everything up to the next piece of text in the pattern (or the end of the
  line), so two fields need text between them: `{a:f32},{b:f32}`, never `{a:f32}{b:f32}`.
- Spaces around a number are fine: `D: 12` matches `D:{meters:f32}`.
- A pattern needs at least one typed field.

### Scale and offset

The value published is `raw × scale + offset`. For a pattern with one field, write them on the
line; with several, per field:

```toml
line = { pattern = "D:{meters:u32}", scale = 0.01 }            # cm → m
line = { pattern = "{a:i32},{b:i32}", fields = { b = { scale = 0.1, offset = -40 } } }
```

A whole-number field that isn't scaled stays a whole number (a satellite count); everything else
becomes a decimal.

## 4. From fields to a reading

A field's **name is the reading field it fills**, written as in docs/capabilities.md in
kebab-case: `meters`, `latitude-deg`. Nested fields use dots: `position.x`. Fields the line
doesn't carry, but the reading needs, get a fixed value under `set`:

```toml
[[provides]]
capability = "pose@1"
line = { pattern = "P {position.x:f32} {position.y:f32}", set = { frame = "odom", position.z = 0, orientation = { x = 0, y = 0, z = 0, w = 1 } } }
```

When Faber loads the file it builds one reading from stand-in values, so a misspelt or missing
field is reported with its line straight away, not as bad lines on a running part. `t-us` is
filled in by the core (section 5).

What each capability supports:

| Capability | Declarative | Notes |
|---|---|---|
| `distance@1` | yes | Name a field `meters`. Without a `status` field, Faber works it out from `info`: below `min-m` is `too-close`, above `max-m` is `too-far` (and `meters` is held at that edge); without `info` it is `ok`. |
| `position@1` | yes | Use `format = "nmea"` (section 5). A pattern works too: `fix` goes under `set` (it is a word), `satellites` can be a `u32` field. |
| `pose@1` | yes | Nested fields and `set`, as above. |
| `battery@1` | yes | Name a field `volts` (and `amps`, `charge`, `celsius` if the monitor sends them; current is negative while discharging, so a monitor that sends discharge milliamps takes `scale = -0.001`). Without a `level` field, Faber works it out from `volts` and `info` (`chemistry` and `cells-in-series`, which it then needs): the same thresholds and hysteresis as every battery (docs/capabilities.md). |
| `estop@1` | not usefully | `engaged` is true/false, which no field type reads. |
| `laser-scan@1` | no | A scan doesn't fit on one line: write a wasm connector. |
| `motor@1`, `planar-base@1` | no | They take commands, and a declarative connector only reads. |

Other capabilities in docs/capabilities.md become available as Faber implements them; the same
rules then apply to them.

## 5. NMEA (GPS)

```toml
[[provides]]
capability = "position@1"
line = { format = "nmea" }
```

The built-in reader takes NMEA 0183 sentences from any talker (`$GP…`, `$GN…`, `$GL…`) and checks
each one's checksum (`*hh`); a sentence whose checksum doesn't add up is a bad line. It uses:

| Sentence | For |
|---|---|
| GGA | latitude, longitude, altitude, fix quality, satellites. **One reading per GGA.** |
| RMC | speed and course over ground, taken by the GGA of the same second (same UTC time). |
| GSA | whether a plain GPS fix is `fix2d` or `fix3d`. Without GSA a GGA fix counts as `fix3d`. |

GGA's fix quality becomes `fix`: 1 GPS (and 3, PPS) → `fix3d`/`fix2d`, 2 → `dgps`, 4 → `rtk-fixed`,
5 → `rtk-float`; 0, and 6–8 (estimated, manual, simulated), are `none`. While `fix` is `none`,
latitude and longitude are 0: ignore them. A module that sends no GGA at all (two RMCs in a row)
gets one reading per RMC instead, as `fix2d` unless GSA says 3D. Other sentences (GSV, VTG, GLL,
TXT) are accepted and not used. Accuracy fields stay empty: HDOP is not a distance.

## 6. Time, bad lines and health

**Time.** A reading's `t-us` is the core's receive time of the line's **last byte** (the chunk
that ended it), on the core's clock, never a time from the device.

**Bad lines** are lines no pattern reads: the wrong shape, a value that isn't a number, a line
over `max-length`, bytes that aren't text, or an NMEA sentence with a bad checksum. They never
stop the part. Every bad line is counted, and:

| Situation | Part state | Shown |
|---|---|---|
| no good line yet | starting | |
| good lines arriving | running | |
| `warn-after-bad-lines` bad lines in a row | degraded | how many, how many so far, the latest bad line and why ("meters: \"abc\" is not a number"), and to check the baud rate and the serial monitor |
| nothing for `timeout-s` | failed | "Nothing has arrived from the sensor": check power, TX → RX and the port |
| data, but no good line for `timeout-s` | failed | "Data arrives, but no line could be read" |

One good line puts it back to running.

## 7. Errors in faber.toml

Every mistake names its line and says what to write instead. A few examples:

```
line 15: The pattern has no fields, so it can't read a value. `pattern` needs at least one field like {meters:f32}.
line 15: "float" in {meters:float} is not a field type. Use one of f32, i32, u32, hex, like {meters:f32}.
line 15: This line can't make a distance@1 reading: missing field `meters`. Name the pattern's fields after the reading's fields, …
line 14: motor@1 parts take commands, and a declarative connector can only read lines. Write a wasm connector for hardware that is commanded.
line 1: This faber.toml doesn't say which manifest format it is written in. Add faber-manifest = 1 as its first line.
```

A connector with a mistake isn't loaded; the core starts without it and writes the message to
the core's log, as `connectors/<id>/faber.toml line N: …`. Installing a package with a mistake is
refused before anything changes, naming the package file and the file inside it:
`my-sensor-0.1.0.faberpkg: faber.toml line N: …`.

## 8. Testing with a capture and a replay port

You don't need the device on your desk to work on a connector:

1. **Record it once.** Open the device's port in the shell's serial monitor and press Record. The
   recording lands in Files → Recordings as a `.fbcap` (D35).
2. **Replay it.** Every recording in Recordings is a port, `replay:<file name>`, that plays the
   bytes back with their original timing, in a loop. Add a part with your connector on that
   port, and watch its viewer and state. Edit `faber.toml`, restart the core, and try again.
3. **Ship it.** Put the recording in your connector's `captures/` folder (packages.md §5), so
   others can try it without the hardware and `faber check` can run it.

Faber's own examples are tested this way on every build: `core/tests/declarative.rs` loads each
example's `faber.toml`, plays its capture through the engine, and compares every reading. Their
captures are written by that test from known lines (so a change to the lines is a change to the
test), not recorded from hardware.

## 9. Not yet

- SPI register maps, and writing a register (a sensor that must be configured before it measures).
- Several registers combined into one value (a BME280's calibration): write a wasm connector once
  wasm connectors can reach a bus.
- Text values (a field that reads `ok` or `ERR`) and true/false fields.
- Binary protocols and commands to the device: write a wasm connector.
- Installing from a `.faberpkg` file: the package installer (packages.md §5).

## 10. I²C registers

An I²C sensor is a small set of registers at an address on the bus. Its datasheet has a
**register map**: which register holds the value, how many bytes it is, and what one bit is worth.
That is all a declarative connector needs. The TMP102 (the whole connector,
[`examples/connectors/ti-tmp102`](../examples/connectors/ti-tmp102/faber.toml)):

```toml
[ports]
i2c = { addresses = [0x48, 0x49, 0x4A, 0x4B], default = 0x48 }

[[provides]]
capability = "temperature@1"
poll = { default-hz = 1 }
read = { register = 0x00, bytes = 2, format = "i16be", shift = 4, scale = 0.0625, unit = "celsius" }
```

**`[ports] i2c`**: `addresses` are the addresses the sensor can have (most have a pin that picks
one), from 0x08 to 0x77; `default` (optional, the first one otherwise) is where a new part starts,
usually what the sensor has out of the box. A part is one device: two parts share a bus at
different addresses, never at the same one.

**`read`**, one register per provide:

| Key | Meaning |
|---|---|
| `register` | The register's number, 0x00–0xff. |
| `format` | How its bytes make a number: `u8`, `i8`, `u16be`, `i16be`, `u16le`, `i16le`, `u32be`, `i32be`, `u32le`, `i32le` (`u` unsigned, `i` signed two's complement, `be` most significant byte first, `le` least). |
| `bytes` | Optional, a check: how many bytes the format reads. |
| `shift` | Optional: bits to drop on the right first, for a value left-justified in its bytes. A signed value keeps its sign. |
| `scale`, `offset` | Optional: `value = (raw >> shift) × scale + offset`. A number, or `{ setting = "…" }`. |
| `unit` | The reading field the value fills, named after its unit as in the reading: `celsius`, `meters`, `volts`, `fraction`. |
| `set` | Optional: fixed reading fields, as for lines (section 4). |

**`poll = { default-hz = 1 }`**: how many times a second the register is read, more than 0 and at
most 100; once a second when it is left out.

Every read is one bus transaction: Faber writes the register's number, then reads its bytes (a
repeated start, `I2C_RDWR` on Linux). It never writes anything else, so a connector can't change
a sensor's settings; a sensor that must be configured first isn't declarative yet (section 9).

**Health:** the part is `running` while reads succeed. A read nobody answers makes it `degraded`
with what went wrong; three in a row make it `failed` ("The sensor didn't answer"), with a hint to
check the wiring and the address and to run the I2C scanner.

**Testing without the sensor:** start the core with `--fake-hardware`. Its `i2c-1` has a TMP102
at 0x48 that encodes temperatures as the datasheet says, so the example runs as it would on a Pi.
`core/tests/declarative.rs` runs the example against it on every build.
