//! Board pin maps (docs/wiring.md §4): every pin of a board, what it can do, which pins are
//! reserved and why, and the board's conflict rules (which timers the Servo library and `tone()`
//! take). One data file per board in `boards/`, built into the core like the simulator's worlds.
//!
//! The Raspberry Pi's header is a pin map too ([`pi_header`]), but its physical pins come from the
//! port manager's header table, so they are passed in rather than repeated here.
//!
//! A part can add pins (a PCA9685's PWM channels, an ADS1115's analog inputs): [`PinMap::with_provided`]
//! appends them, named `<part>.<pin>`, and the allocator treats them like the board's own.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The Arduino boards with a pin map, by the flasher's board id, and their files. `nano-old` is
/// the Nano with its old bootloader: the same pins.
const FILES: &[(&str, &str)] = &[
    ("uno", include_str!("boards/uno.toml")),
    ("nano", include_str!("boards/nano.toml")),
    ("nano-old", include_str!("boards/nano.toml")),
    ("mega2560", include_str!("boards/mega2560.toml")),
];

const PI_HEADER: &str = include_str!("boards/pi-header.toml");

/// The id of the Raspberry Pi header's pin map.
pub const PI_HEADER_ID: &str = "pi-header";

/// What kind of thing the pins are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "PinMapKind")]
pub enum BoardKind {
    /// An Arduino: Faber writes its sketch from a plan.
    Arduino,
    /// The Raspberry Pi's own header (data only, for now).
    Pi,
}

/// What a bus pin belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum BusKind {
    I2c,
    Spi,
    Serial,
}

/// One bus a pin is part of, e.g. `i2c` `sda`, `serial1` `rx`, `spi-0` `ce0`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PinBus")]
pub struct BusPin {
    pub kind: BusKind,
    /// The bus: `i2c`, `spi`, `serial`, `serial1` … on an Arduino; `i2c-1`, `spi-0`, `uart0` on a
    /// Pi (as its ports are named).
    pub bus: String,
    /// The pin's line on it: `sda`, `scl`, `mosi`, `miso`, `sck`, `ss`, `ce0`, `rx`, `tx`.
    pub line: String,
}

/// One pin and what it can do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "BoardPin")]
pub struct Pin {
    /// As printed on the board: `D7`, `A0`, `GPIO17`; `servos.ch3` for a pin a part provides.
    pub name: String,
    /// The Arduino pin number (`A0` is 14 on an Uno), or the GPIO line on a Pi. None for a pin a
    /// part provides.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u16>,
    /// The physical pin on a Pi's header (1–40).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical: Option<u8>,
    /// Digital in and out (`digitalRead`/`digitalWrite`). False for an analog-only pin (A6/A7 on
    /// a Nano).
    pub digital: bool,
    /// Its analog input channel (`A3` is 3), if it has one.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analog: Option<u8>,
    /// The hardware PWM behind it: `timer2b` (the timer and its channel), `pwm0-ch2` on a Pi.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pwm: Option<String>,
    /// Its external interrupt (`attachInterrupt`), e.g. 0 for INT0.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<u8>,
    /// It has a pin-change interrupt.
    pub pin_change: bool,
    /// The buses it is part of.
    pub buses: Vec<BusPin>,
    /// It drives the on-board LED.
    pub led: bool,
    /// Its logic level.
    pub volts: f32,
    /// The part in the plan that provides this pin, for a pin that isn't the board's own.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provided_by: Option<String>,
}

impl Pin {
    /// The timer behind its PWM (`timer2` for `timer2b`), on an Arduino.
    pub fn timer(&self) -> Option<&str> {
        let pwm = self.pwm.as_deref()?;
        let digits = pwm.strip_prefix("timer")?.chars().take_while(char::is_ascii_digit).count();
        (digits > 0).then(|| &pwm[.."timer".len() + digits])
    }

    /// Its line on a bus of this kind, if it is on one.
    pub fn bus(&self, kind: BusKind) -> Option<&BusPin> {
        self.buses.iter().find(|b| b.kind == kind)
    }

    /// How the sketch names it: `7` for `D7`, `A0` for `A0`.
    pub fn code(&self) -> String {
        match (self.name.strip_prefix('D'), self.number) {
            (Some(n), Some(num)) if n == num.to_string() => n.to_string(),
            _ => self.name.clone(),
        }
    }
}

/// Pins that are never offered, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ReservedPins")]
pub struct Reserved {
    pub pins: Vec<String>,
    pub reason: String,
}

/// Something worth knowing about some pins; `avoid` lists the needs (`digital-in`, …) for which
/// the allocator picks other pins first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PinNote {
    pub pins: Vec<String>,
    pub note: String,
    pub avoid: Vec<String>,
}

/// A physical header position with no GPIO (3V3, 5V, GND), for drawing a Pi's header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PowerPin {
    pub physical: u8,
    pub name: String,
}

/// A board's pins and its rules (`GET /api/v0/arduino/pins/{board}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PinMap {
    /// `uno`, `nano`, `nano-old`, `mega2560`, `pi-header`.
    pub id: String,
    pub name: String,
    pub chip: String,
    pub kind: BoardKind,
    /// The board's logic level.
    pub logic_volts: f32,
    /// What `analogRead` measures against (its full scale). None: no analog inputs (a Pi).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analog_reference_volts: Option<f32>,
    /// Every pin, in the board's order.
    pub pins: Vec<Pin>,
    pub reserved: Vec<Reserved>,
    pub notes: Vec<PinNote>,
    /// The timers the Servo library takes, in the order it takes them.
    pub servo_timers: Vec<String>,
    /// How many servos one of those timers drives.
    pub servos_per_timer: u8,
    /// The timer `tone()` takes.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone_timer: Option<String>,
    /// A Pi header's power pins, by position (empty for an Arduino).
    pub power: Vec<PowerPin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawMap {
    faber_pins: u32,
    id: String,
    name: String,
    chip: String,
    kind: BoardKind,
    logic_volts: f32,
    analog_reference_volts: Option<f32>,
    #[serde(default)]
    servo_timers: Vec<String>,
    #[serde(default)]
    servos_per_timer: u8,
    tone_timer: Option<String>,
    #[serde(default)]
    pins: Vec<RawPin>,
    #[serde(default)]
    gpio: Vec<RawGpio>,
    #[serde(default)]
    reserved: Vec<Reserved>,
    #[serde(default)]
    notes: Vec<RawNote>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawPin {
    name: String,
    number: u16,
    #[serde(default = "yes")]
    digital: bool,
    analog: Option<u8>,
    pwm: Option<String>,
    interrupt: Option<u8>,
    #[serde(default)]
    pin_change: bool,
    #[serde(default)]
    buses: Vec<String>,
    #[serde(default)]
    led: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawGpio {
    line: u16,
    pwm: Option<String>,
    #[serde(default)]
    buses: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNote {
    pins: Vec<String>,
    note: String,
    #[serde(default)]
    avoid: Vec<String>,
}

fn yes() -> bool {
    true
}

/// `i2c:sda` → the I²C bus's SDA line.
pub(super) fn parse_bus(text: &str) -> Result<BusPin, String> {
    let (bus, line) = text.split_once(':').ok_or_else(|| format!("\"{text}\" isn't bus:line"))?;
    let kind = if bus.starts_with("i2c") {
        BusKind::I2c
    } else if bus.starts_with("spi") {
        BusKind::Spi
    } else if bus.starts_with("serial") || bus.starts_with("uart") {
        BusKind::Serial
    } else {
        return Err(format!("\"{bus}\" isn't an I²C, SPI or serial bus"));
    };
    Ok(BusPin { kind, bus: bus.into(), line: line.into() })
}

fn raw(text: &str) -> Result<RawMap, String> {
    let raw: RawMap = toml::from_str(text).map_err(|e| e.to_string())?;
    if raw.faber_pins != 1 {
        return Err(format!("faber-pins = {} isn't a version this Faber reads", raw.faber_pins));
    }
    Ok(raw)
}

fn notes(raw: Vec<RawNote>) -> Vec<PinNote> {
    raw.into_iter().map(|n| PinNote { pins: n.pins, note: n.note, avoid: n.avoid }).collect()
}

/// Reads a pin-map file of an Arduino board.
fn parse(text: &str) -> Result<PinMap, String> {
    let raw = raw(text)?;
    let volts = raw.logic_volts;
    let pins = raw
        .pins
        .into_iter()
        .map(|p| {
            Ok(Pin {
                buses: p.buses.iter().map(|b| parse_bus(b)).collect::<Result<_, _>>()?,
                name: p.name,
                number: Some(p.number),
                physical: None,
                digital: p.digital,
                analog: p.analog,
                pwm: p.pwm,
                interrupt: p.interrupt,
                pin_change: p.pin_change,
                led: p.led,
                volts,
                provided_by: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PinMap {
        id: raw.id,
        name: raw.name,
        chip: raw.chip,
        kind: raw.kind,
        logic_volts: volts,
        analog_reference_volts: raw.analog_reference_volts,
        pins,
        reserved: raw.reserved,
        notes: notes(raw.notes),
        servo_timers: raw.servo_timers,
        servos_per_timer: raw.servos_per_timer,
        tone_timer: raw.tone_timer,
        power: Vec::new(),
    })
}

/// The pin map of an Arduino board, by the flasher's board id (`uno`, `nano`, `nano-old`,
/// `mega2560`).
pub fn arduino(board: &str) -> Option<PinMap> {
    let (_, text) = FILES.iter().find(|(id, _)| *id == board)?;
    // The files are part of the binary and the tests parse every one of them.
    let mut map = parse(text).unwrap_or_else(|e| panic!("the pin map of {board} is invalid: {e}"));
    if map.id != board {
        // nano-old: the Nano's pins under the flasher's name for the board.
        let flasher = faber_flash::Board::by_id(board).expect("every pin map is a flasher board");
        map.id = board.into();
        map.name = flasher.name.into();
    }
    Some(map)
}

/// The Arduino boards with a pin map, in the flasher's order.
pub fn arduino_boards() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|(id, _)| *id)
}

/// One position of the Pi's header, as the port manager's table has it.
#[derive(Clone, Copy, Debug)]
pub struct HeaderPosition<'a> {
    pub physical: u8,
    /// `GPIO17`, `3V3`, `5V`, `GND`.
    pub name: &'a str,
    pub line: Option<u32>,
}

/// The Raspberry Pi header's pin map, from the header table (`ports::header::HEADER`, passed in
/// by the machine) and what `boards/pi-header.toml` says each line can do.
pub fn pi_header<'a>(header: impl IntoIterator<Item = HeaderPosition<'a>>) -> PinMap {
    let raw = raw(PI_HEADER).unwrap_or_else(|e| panic!("the Pi header's pin map is invalid: {e}"));
    let mut pins = Vec::new();
    let mut power = Vec::new();
    for pos in header {
        let Some(line) = pos.line else {
            power.push(PowerPin { physical: pos.physical, name: pos.name.into() });
            continue;
        };
        let extra = raw.gpio.iter().find(|g| u32::from(g.line) == line);
        pins.push(Pin {
            name: pos.name.into(),
            number: u16::try_from(line).ok(),
            physical: Some(pos.physical),
            digital: true,
            analog: None,
            pwm: extra.and_then(|g| g.pwm.clone()),
            interrupt: None,
            pin_change: false,
            buses: extra
                .map(|g| g.buses.iter().map(|b| parse_bus(b).expect("the tests parse every bus")).collect())
                .unwrap_or_default(),
            led: false,
            volts: raw.logic_volts,
            provided_by: None,
        });
    }
    PinMap {
        id: raw.id,
        name: raw.name,
        chip: raw.chip,
        kind: raw.kind,
        logic_volts: raw.logic_volts,
        analog_reference_volts: raw.analog_reference_volts,
        pins,
        reserved: raw.reserved,
        notes: notes(raw.notes),
        servo_timers: raw.servo_timers,
        servos_per_timer: raw.servos_per_timer,
        tone_timer: raw.tone_timer,
        power,
    }
}

impl PinMap {
    pub fn pin(&self, name: &str) -> Option<&Pin> {
        self.pins.iter().find(|p| p.name == name)
    }

    /// The pin a plan or a person means: its name (`D7`, `a0`, `GPIO17`, `servos.ch3`), or its
    /// number (`7`, `14`).
    pub fn resolve(&self, text: &str) -> Option<&Pin> {
        let text = text.trim();
        if let Ok(n) = text.parse::<u16>() {
            return self.pins.iter().find(|p| p.number == Some(n) && p.provided_by.is_none());
        }
        self.pins.iter().find(|p| p.name.eq_ignore_ascii_case(text))
    }

    /// Why a pin is reserved, if it is.
    pub fn reserved_reason(&self, name: &str) -> Option<&str> {
        self.reserved.iter().find(|r| r.pins.iter().any(|p| p == name)).map(|r| r.reason.as_str())
    }

    /// Notes about a pin.
    pub fn notes_for(&self, name: &str) -> impl Iterator<Item = &PinNote> {
        self.notes.iter().filter(move |n| n.pins.iter().any(|p| p == name))
    }

    /// The pins on a bus of this kind (`i2c`'s SDA and SCL …).
    pub fn bus_pins(&self, kind: BusKind) -> impl Iterator<Item = &Pin> {
        self.pins.iter().filter(move |p| p.provided_by.is_none() && p.bus(kind).is_some())
    }

    /// The map with pins a part provides appended (a PCA9685's channels …).
    pub fn with_provided(mut self, pins: Vec<Pin>) -> PinMap {
        self.pins.extend(pins);
        self
    }

    /// Names of the board's pins with this ability, for messages: "D3, D5, D6, D9, D10 and D11".
    pub fn names_where(&self, f: impl Fn(&Pin) -> bool) -> Vec<&str> {
        self.pins.iter().filter(|p| p.provided_by.is_none() && f(p)).map(|p| p.name.as_str()).collect()
    }
}

/// "D2, D3 and D18": a list for a sentence.
pub fn and_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn all() -> Vec<PinMap> {
        arduino_boards().map(|b| arduino(b).unwrap()).collect()
    }

    // The ATmega328P variant (Arduino AVR core 1.8.8, variants/standard/pins_arduino.h), written
    // out as the header's macros and tables say it, so the data file is checked against them.
    mod standard {
        pub const NUM_DIGITAL_PINS: u16 = 20;
        pub fn has_pwm(p: u16) -> bool {
            matches!(p, 3 | 5 | 6 | 9 | 10 | 11)
        }
        pub fn interrupt(p: u16) -> Option<u8> {
            match p {
                2 => Some(0),
                3 => Some(1),
                _ => None,
            }
        }
        /// digital_pin_to_timer_PGM (the non-ATmega8 branch).
        pub const TIMERS: [Option<&str>; 20] = [
            None,
            None,
            None,
            Some("timer2b"),
            None,
            Some("timer0b"),
            Some("timer0a"),
            None,
            None,
            Some("timer1a"),
            Some("timer1b"),
            Some("timer2a"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
    }

    mod mega {
        pub const NUM_DIGITAL_PINS: u16 = 70;
        pub fn has_pwm(p: u16) -> bool {
            (2..=13).contains(&p) || (44..=46).contains(&p)
        }
        pub fn interrupt(p: u16) -> Option<u8> {
            match p {
                2 => Some(0),
                3 => Some(1),
                18..=21 => Some((23 - p) as u8),
                _ => None,
            }
        }
        pub fn pin_change(p: u16) -> bool {
            (10..=13).contains(&p) || (50..=53).contains(&p) || (62..=69).contains(&p)
        }
        pub fn timer(p: u16) -> Option<&'static str> {
            Some(match p {
                2 => "timer3b",
                3 => "timer3c",
                4 => "timer0b",
                5 => "timer3a",
                6 => "timer4a",
                7 => "timer4b",
                8 => "timer4c",
                9 => "timer2b",
                10 => "timer2a",
                11 => "timer1a",
                12 => "timer1b",
                13 => "timer0a",
                44 => "timer5c",
                45 => "timer5b",
                46 => "timer5a",
                _ => return None,
            })
        }
    }

    #[test]
    fn every_file_parses_and_every_pin_is_valid() {
        for map in all() {
            assert_eq!(map.kind, BoardKind::Arduino);
            let flasher = faber_flash::Board::by_id(&map.id).unwrap();
            assert_eq!(map.name, flasher.name, "{}", map.id);
            assert_eq!(map.chip, flasher.chip, "{}", map.id);
            let mut names = BTreeSet::new();
            let mut numbers = BTreeSet::new();
            for p in &map.pins {
                assert!(names.insert(&p.name), "{}: {} twice", map.id, p.name);
                assert!(numbers.insert(p.number), "{}: number {:?} twice", map.id, p.number);
                assert!(p.digital || p.analog.is_some(), "{}: {} can do nothing", map.id, p.name);
                assert!(p.pwm.is_none() || p.digital, "{}: {} has PWM but no digital out", map.id, p.name);
                assert!(p.interrupt.is_none() || p.digital, "{}: {}", map.id, p.name);
                assert_eq!(p.volts, 5.0);
                let expected = match p.analog {
                    Some(ch) => format!("A{ch}"),
                    None => format!("D{}", p.number.unwrap()),
                };
                assert_eq!(p.name, expected, "{}: names follow the board's print", map.id);
                if let Some(t) = p.timer() {
                    assert!(t.len() == 6 && t.starts_with("timer"), "{}: {t}", p.name);
                }
            }
            for r in &map.reserved {
                for p in &r.pins {
                    assert!(map.pin(p).is_some(), "{}: reserved {p} isn't a pin", map.id);
                }
            }
            for n in &map.notes {
                for p in &n.pins {
                    assert!(map.pin(p).is_some(), "{}: note on {p}", map.id);
                }
                for a in &n.avoid {
                    assert!(super::super::parts::Need::parse(a).is_some(), "{}: avoid {a}", map.id);
                }
            }
            assert_eq!(map.reserved_reason("D0").map(|r| r.contains("USB link")), Some(true));
            assert!(map.servos_per_timer > 0 && !map.servo_timers.is_empty());
            for t in map.servo_timers.iter().chain(&map.tone_timer) {
                assert!(map.pins.iter().any(|p| p.timer() == Some(t)), "{}: {t} drives no pin", map.id);
            }
            // One I²C bus, one SPI bus, one LED.
            assert_eq!(map.bus_pins(BusKind::I2c).count(), 2, "{}", map.id);
            assert_eq!(map.bus_pins(BusKind::Spi).count(), 4, "{}", map.id);
            assert_eq!(
                map.pins.iter().filter(|p| p.led).map(|p| p.name.as_str()).collect::<Vec<_>>(),
                ["D13"]
            );
        }
    }

    /// Checks a 328P map against the standard variant. `extra_analog` are the analog-only pins
    /// beyond NUM_DIGITAL_PINS (a Nano's A6/A7).
    fn check_atmega328p(map: &PinMap, analog_inputs: u8) {
        for p in &map.pins {
            let n = p.number.unwrap();
            let digital = n < standard::NUM_DIGITAL_PINS;
            assert_eq!(p.digital, digital, "{}", p.name);
            assert_eq!(p.pwm.is_some(), standard::has_pwm(n), "{}", p.name);
            assert_eq!(p.interrupt, standard::interrupt(n), "{}", p.name);
            // digitalPinToPCICR is non-null for 0-21, but 20/21 (A6/A7) have no port: ADC6/7
            // are analog-only on the chip.
            assert_eq!(p.pin_change, digital, "{}", p.name);
            if digital {
                assert_eq!(p.pwm.as_deref(), standard::TIMERS[usize::from(n)], "{}", p.name);
            }
            // analogInputToDigitalPin(p) = p + 14, for p < NUM_ANALOG_INPUTS.
            assert_eq!(p.analog, (n >= 14).then(|| (n - 14) as u8), "{}", p.name);
        }
        assert_eq!(map.pins.iter().filter(|p| p.analog.is_some()).count(), usize::from(analog_inputs));
        let bus = |name: &str| {
            map.pin(name).unwrap().buses.iter().map(|b| format!("{}:{}", b.bus, b.line)).collect::<Vec<_>>()
        };
        assert_eq!(bus("D10"), ["spi:ss"]);
        assert_eq!(bus("D11"), ["spi:mosi"]);
        assert_eq!(bus("D12"), ["spi:miso"]);
        assert_eq!(bus("D13"), ["spi:sck"]);
        assert_eq!(bus("A4"), ["i2c:sda"]);
        assert_eq!(bus("A5"), ["i2c:scl"]);
        assert_eq!(bus("D0"), ["serial:rx"]);
        assert_eq!(map.servo_timers, ["timer1"]);
        assert_eq!(map.tone_timer.as_deref(), Some("timer2"));
    }

    #[test]
    fn uno_matches_the_variant() {
        let uno = arduino("uno").unwrap();
        assert_eq!(uno.pins.len(), 20);
        check_atmega328p(&uno, 6);
    }

    #[test]
    fn nano_matches_the_variant() {
        let nano = arduino("nano").unwrap();
        assert_eq!(nano.pins.len(), 22);
        check_atmega328p(&nano, 8);
        let a6 = nano.pin("A6").unwrap();
        assert!(!a6.digital && a6.analog == Some(6) && !a6.pin_change, "A6 is analog only");
        let old = arduino("nano-old").unwrap();
        assert_eq!((old.id.as_str(), old.name.as_str()), ("nano-old", "Arduino Nano (old bootloader)"));
        assert_eq!(old.pins, nano.pins);
    }

    #[test]
    fn mega_matches_the_variant() {
        let m = arduino("mega2560").unwrap();
        assert_eq!(m.pins.len(), usize::from(mega::NUM_DIGITAL_PINS));
        for p in &m.pins {
            let n = p.number.unwrap();
            assert!(p.digital, "every Mega pin is digital, A0-A15 too");
            assert_eq!(p.pwm.is_some(), mega::has_pwm(n), "{}", p.name);
            assert_eq!(p.pwm.as_deref(), mega::timer(n), "{}", p.name);
            assert_eq!(p.interrupt, mega::interrupt(n), "{}", p.name);
            assert_eq!(p.pin_change, mega::pin_change(n), "{}", p.name);
            assert_eq!(p.analog, (n >= 54).then(|| (n - 54) as u8), "{}", p.name);
        }
        // ServoTimers.h: Timer5 first, so the first servo takes D44-D46.
        assert_eq!(m.servo_timers, ["timer5", "timer1", "timer3", "timer4"]);
        let t5: Vec<_> = m.names_where(|p| p.timer() == Some("timer5"));
        assert_eq!(t5, ["D44", "D45", "D46"]);
        assert_eq!(m.names_where(|p| p.timer() == Some("timer2")), ["D9", "D10"], "tone()");
        let serial1: Vec<_> = m
            .pins
            .iter()
            .filter(|p| p.buses.iter().any(|b| b.bus == "serial1"))
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(serial1, ["D18", "D19"]);
        assert_eq!(m.pin("D20").unwrap().interrupt, Some(3), "an interrupt pin that is also I²C");
    }

    /// Reads `digital_pin_to_timer_PGM` from a variant header on this computer (the Arduino IDE's
    /// copy), taking the `#else` side of `#if defined(__AVR_ATmega8__)`.
    fn timer_table(header: &str) -> Vec<Option<String>> {
        let start = header.find("digital_pin_to_timer_PGM[] = {").expect("the timer table");
        let body = &header[start..];
        let body = &body[body.find('{').unwrap() + 1..body.find("};").unwrap()];
        let mut out = Vec::new();
        let mut skipping = false;
        for line in body.lines() {
            let code = line.split("//").next().unwrap().trim();
            if code.starts_with("#if") {
                skipping = code.contains("__AVR_ATmega8__");
                continue;
            }
            if code.starts_with("#else") {
                skipping = !skipping;
                continue;
            }
            if code.starts_with("#endif") {
                skipping = false;
                continue;
            }
            if skipping {
                continue;
            }
            let code = code.split("/*").next().unwrap();
            for entry in code.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                out.push(entry.strip_prefix("TIMER").map(|t| format!("timer{}", t.to_ascii_lowercase())));
            }
        }
        out
    }

    #[test]
    fn the_timer_tables_match_the_installed_variant_headers() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            eprintln!("skipped: no %LOCALAPPDATA% (the Arduino IDE's AVR core isn't on this computer)");
            return;
        };
        let avr = std::path::Path::new(&local).join("Arduino15/packages/arduino/hardware/avr");
        let Some(version) = std::fs::read_dir(&avr).ok().and_then(|d| d.flatten().map(|e| e.path()).max())
        else {
            eprintln!("skipped: no Arduino AVR core in {}", avr.display());
            return;
        };
        for (variant, board) in [("standard", "uno"), ("mega", "mega2560")] {
            let header =
                std::fs::read_to_string(version.join("variants").join(variant).join("pins_arduino.h"))
                    .unwrap();
            let table = timer_table(&header);
            let map = arduino(board).unwrap();
            let digital: Vec<_> = map.pins.iter().filter(|p| p.digital).collect();
            assert_eq!(table.len(), digital.len(), "{variant}: one entry per digital pin");
            for (p, t) in digital.iter().zip(&table) {
                assert_eq!(p.pwm.as_ref(), t.as_ref(), "{variant}: {}", p.name);
            }
        }
    }

    #[test]
    fn resolving_takes_names_and_numbers() {
        let uno = arduino("uno").unwrap();
        assert_eq!(uno.resolve("7").unwrap().name, "D7");
        assert_eq!(uno.resolve("d7").unwrap().name, "D7");
        assert_eq!(uno.resolve("A0").unwrap().name, "A0");
        assert_eq!(uno.resolve("14").unwrap().name, "A0");
        assert!(uno.resolve("A6").is_none(), "an Uno has no A6");
        assert!(uno.resolve("D20").is_none());
        assert_eq!(uno.pin("D7").unwrap().code(), "7");
        assert_eq!(uno.pin("A0").unwrap().code(), "A0");
        assert_eq!(uno.pin("D9").unwrap().timer(), Some("timer1"));
        assert_eq!(uno.pin("D7").unwrap().timer(), None);
        assert_eq!(and_list(&["D2", "D3"]), "D2 and D3");
        assert_eq!(and_list(&["D3", "D5", "D6"]), "D3, D5 and D6");
    }

    fn fake_header() -> Vec<HeaderPosition<'static>> {
        // A few positions of the real header (ports::header::HEADER), enough for the format; the
        // machine's test checks the whole header.
        vec![
            HeaderPosition { physical: 1, name: "3V3", line: None },
            HeaderPosition { physical: 3, name: "GPIO2", line: Some(2) },
            HeaderPosition { physical: 11, name: "GPIO17", line: Some(17) },
            HeaderPosition { physical: 12, name: "GPIO18", line: Some(18) },
            HeaderPosition { physical: 27, name: "GPIO0", line: Some(0) },
        ]
    }

    #[test]
    fn the_pi_header_is_a_pin_map_of_3v3_gpio_with_no_analog() {
        let pi = pi_header(fake_header());
        assert_eq!(pi.id, PI_HEADER_ID);
        assert_eq!(pi.kind, BoardKind::Pi);
        assert_eq!(pi.analog_reference_volts, None);
        assert_eq!(pi.power, [PowerPin { physical: 1, name: "3V3".into() }]);
        let g18 = pi.pin("GPIO18").unwrap();
        assert_eq!((g18.physical, g18.number, g18.volts), (Some(12), Some(18), 3.3));
        assert_eq!(g18.pwm.as_deref(), Some("pwm0-ch2"));
        assert_eq!(g18.timer(), None, "no AVR timers on a Pi");
        assert_eq!(pi.pin("GPIO2").unwrap().bus(BusKind::I2c).unwrap().bus, "i2c-1");
        assert!(pi.pins.iter().all(|p| p.analog.is_none() && p.interrupt.is_none()));
        assert!(pi.reserved_reason("GPIO0").unwrap().contains("HAT"));
        assert_eq!(pi.resolve("17").unwrap().name, "GPIO17");
        assert_eq!(pi.pin("GPIO17").unwrap().code(), "GPIO17");
        // Every line the file describes is a real GPIO line.
        let raw = raw(PI_HEADER).unwrap();
        for g in &raw.gpio {
            assert!(g.line <= 27, "GPIO{} isn't on the header", g.line);
            for b in &g.buses {
                parse_bus(b).unwrap();
            }
        }
    }

    #[test]
    fn provided_pins_join_the_map() {
        let uno = arduino("uno").unwrap();
        let extra = Pin {
            name: "servos.ch0".into(),
            number: None,
            physical: None,
            digital: false,
            analog: None,
            pwm: Some("pca9685".into()),
            interrupt: None,
            pin_change: false,
            buses: vec![],
            led: false,
            volts: 5.0,
            provided_by: Some("servos".into()),
        };
        let more = uno.with_provided(vec![extra]);
        assert_eq!(more.resolve("servos.CH0").unwrap().provided_by.as_deref(), Some("servos"));
        assert_eq!(more.pin("servos.ch0").unwrap().timer(), None);
        assert!(!more.names_where(|p| p.pwm.is_some()).contains(&"servos.ch0"), "lists name the board's own");
    }
}
