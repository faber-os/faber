//! Part templates (docs/wiring.md §4): one per FaberLink part class, as data files in `parts/`
//! built into the core. A template says which pins a part needs and what each must be able to do
//! (its *roles*), its settings, its power and warnings, how to wire it, and the code it adds to a
//! sketch.
//!
//! A part can come in several *shapes*: other sets of pin roles with their own constructor, like a
//! motor driver with PWM + DIR or with EN + IN1 + IN2. A shape marked `todo` (waiting for its
//! constructor in the library) is read but not offered.
//!
//! `drift_*` tests read `firmware/faberlink/src/FaberLink.h` and fail when a template names a
//! class or a constructor the library doesn't have, or a library part class has no template.

use super::pins::{BusKind, Pin};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use ts_rs::TS;

/// Every template's file, in catalogue order.
const FILES: &[(&str, &str)] = &[
    ("ultrasonic", include_str!("parts/ultrasonic.toml")),
    ("ir-distance", include_str!("parts/ir-distance.toml")),
    ("battery", include_str!("parts/battery.toml")),
    ("motor", include_str!("parts/motor.toml")),
    ("button", include_str!("parts/button.toml")),
    ("bumper", include_str!("parts/bumper.toml")),
    ("ir-obstacle", include_str!("parts/ir-obstacle.toml")),
    ("led", include_str!("parts/led.toml")),
    ("relay", include_str!("parts/relay.toml")),
    ("mosfet", include_str!("parts/mosfet.toml")),
    ("servo", include_str!("parts/servo.toml")),
    ("encoder", include_str!("parts/encoder.toml")),
];

/// What a pin must be able to do for a role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "PinNeed")]
pub enum Need {
    DigitalOut,
    DigitalIn,
    AnalogIn,
    Pwm,
    Interrupt,
}

impl Need {
    pub const ALL: [Need; 5] =
        [Need::DigitalOut, Need::DigitalIn, Need::AnalogIn, Need::Pwm, Need::Interrupt];

    pub fn id(self) -> &'static str {
        match self {
            Need::DigitalOut => "digital-out",
            Need::DigitalIn => "digital-in",
            Need::AnalogIn => "analog-in",
            Need::Pwm => "pwm",
            Need::Interrupt => "interrupt",
        }
    }

    pub fn parse(s: &str) -> Option<Need> {
        Need::ALL.into_iter().find(|n| n.id() == s)
    }

    /// Whether the pin can do it at all (conflicts with other parts aside).
    pub fn fits(self, pin: &Pin) -> bool {
        match self {
            Need::DigitalOut | Need::DigitalIn => pin.digital,
            Need::AnalogIn => pin.analog.is_some(),
            Need::Pwm => pin.pwm.is_some(),
            Need::Interrupt => pin.interrupt.is_some(),
        }
    }

    /// "a digital pin", for "Echo needs a digital pin".
    pub fn words(self) -> &'static str {
        match self {
            Need::DigitalOut | Need::DigitalIn => "a digital pin",
            Need::AnalogIn => "an analog input",
            Need::Pwm => "a PWM pin",
            Need::Interrupt => "an external interrupt pin",
        }
    }
}

/// Whether changing a setting needs an upload (docs/wiring.md §11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "SettingApply")]
pub enum Apply {
    /// Sent to the board as a command, or applied by the core: no upload.
    Live,
    /// Compiled into the sketch: changing it means a new upload.
    Build,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum SettingType {
    Number,
    Integer,
    Choice,
}

/// A setting's value: a number, or a choice's id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum SettingValue {
    Number(f64),
    Text(String),
}

impl SettingValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SettingValue::Number(n) => Some(*n),
            SettingValue::Text(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            SettingValue::Text(s) => Some(s),
            SettingValue::Number(_) => None,
        }
    }
}

impl std::fmt::Display for SettingValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingValue::Number(n) => f.write_str(&number_text(*n)),
            SettingValue::Text(s) => f.write_str(s),
        }
    }
}

/// A number as people write it: `4`, `0.02`, `20000`.
pub fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "SettingChoice")]
pub struct Choice {
    pub id: String,
    pub name: String,
    /// What the sketch writes for it (`faberlink::BATTERY_LI_ION`).
    #[serde(skip)]
    pub code: String,
}

/// One setting of a part, for the settings form (docs/wiring.md §11).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PartSetting")]
pub struct Setting {
    pub id: String,
    pub name: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(rename = "type")]
    pub kind: SettingType,
    pub default: SettingValue,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// For a choice.
    pub options: Vec<Choice>,
    pub apply: Apply,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Setting {
    /// Why a value can't be this setting's, if it can't.
    pub fn check(&self, value: &SettingValue) -> Result<(), String> {
        let unit = self.unit.as_deref().map(|u| format!(" {u}")).unwrap_or_default();
        match (self.kind, value) {
            (SettingType::Choice, SettingValue::Text(id)) => {
                if self.options.iter().any(|o| &o.id == id) {
                    Ok(())
                } else {
                    let ids: Vec<&str> = self.options.iter().map(|o| o.id.as_str()).collect();
                    Err(format!("{} can't be \"{id}\": pick one of {}.", self.name, ids.join(", ")))
                }
            }
            (SettingType::Choice, SettingValue::Number(_)) => {
                Err(format!("{} is a choice, not a number.", self.name))
            }
            (_, SettingValue::Text(t)) => Err(format!("{} is a number, not \"{t}\".", self.name)),
            (kind, SettingValue::Number(n)) => {
                if !n.is_finite() {
                    return Err(format!("{} must be a number.", self.name));
                }
                if kind == SettingType::Integer && n.fract() != 0.0 {
                    return Err(format!("{} must be a whole number, not {}.", self.name, number_text(*n)));
                }
                if let Some(min) = self.min
                    && *n < min
                {
                    return Err(format!("{} must be at least {}{unit}.", self.name, number_text(min)));
                }
                if let Some(max) = self.max
                    && *n > max
                {
                    return Err(format!("{} can be at most {}{unit}.", self.name, number_text(max)));
                }
                Ok(())
            }
        }
    }
}

/// One pin a part needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PartRole")]
pub struct Role {
    /// `trig`, as in the plan's `pins = { trig = 7 }`.
    pub id: String,
    /// `Trig`, as on the part's label.
    pub name: String,
    pub needs: Need,
    pub help: String,
    /// It can also go on a pin a part provides (a PCA9685's channel), not only on the board's own.
    pub provided: bool,
    /// It may be left unconnected (a TB6612's STBY tied high). Its argument then is left out, so
    /// optional roles come last in the constructor.
    pub optional: bool,
}

/// One argument of a part's constructor in the sketch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// The part's name, as a string.
    Name,
    /// A role's pin.
    Pin(String),
    Setting(String),
    /// `(a + b) / b` of two settings: a divider's ratio.
    Ratio(String, String),
    /// The board's analog reference voltage.
    AnalogReference,
    /// C++ as written, after `{var}` and `{name}` are filled in.
    Code(String),
}

impl Arg {
    fn parse(s: &str) -> Result<Arg, String> {
        let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
        Ok(match (kind, rest) {
            ("name", "") => Arg::Name,
            ("pin", r) if !r.is_empty() => Arg::Pin(r.into()),
            ("setting", r) if !r.is_empty() => Arg::Setting(r.into()),
            ("ratio", r) => {
                let (a, b) = r.split_once(':').ok_or_else(|| format!("\"{s}\" isn't ratio:<r1>:<r2>"))?;
                Arg::Ratio(a.into(), b.into())
            }
            ("board", "analog-reference") => Arg::AnalogReference,
            ("code", r) if !r.is_empty() => Arg::Code(r.into()),
            _ => return Err(format!("\"{s}\" isn't an argument Faber knows")),
        })
    }
}

/// One way to wire a part: its pin roles and its constructor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PartShape")]
pub struct Shape {
    pub id: String,
    pub name: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub roles: Vec<Role>,
    /// The wiring lines, with `{pin:<role>}` and `{setting:<id>}` to fill in.
    pub wiring: Vec<String>,
    #[serde(skip)]
    pub args: Vec<Arg>,
    /// C++ lines for `setup()`, before `link.begin(...)` (`{var}.setMaxVelocity(...)`).
    #[serde(skip)]
    pub setup: Vec<String>,
    /// Why it isn't offered yet (its constructor isn't in the library).
    #[serde(skip)]
    pub todo: Option<String>,
}

impl Shape {
    pub fn role(&self, id: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.id == id)
    }
}

/// Where a part takes its power from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PartSupply")]
pub enum Supply {
    /// The board's 5V pin.
    #[serde(rename = "5V")]
    FiveVolts,
    /// The board's 3.3 V pin.
    #[serde(rename = "3V3")]
    ThreeVolts,
    /// Its own supply (motors, servos under load, LED strips).
    #[serde(rename = "own")]
    Own,
    /// It takes no power (a voltage divider).
    #[serde(rename = "none")]
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PartPower")]
pub struct Power {
    pub supply: Supply,
    /// Typical current from that supply.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_ma: Option<u32>,
    /// It must have its own supply, never the board's.
    pub own_supply: bool,
    /// The voltage of its signals (an HC-SR04's Echo is 5 V). None: it has no outputs.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logic_volts: Option<f32>,
    /// The highest voltage its inputs take (3.3 for a 3.3 V-only part).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_volts: Option<f32>,
}

/// A board resource a part takes, beyond its pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "PartResource")]
pub enum Resource {
    /// The Servo library: its timers stop PWM on their pins (one per 12 servos).
    Servo,
    /// `tone()`: its timer stops PWM on its pins.
    Tone,
    /// It turns interrupts off while it works (an addressable LED strip's update), which drops
    /// bytes from Faber and encoder edges.
    InterruptsOff,
}

/// Pins a part adds to the board (a PCA9685's channels): `<part>.<name>` with `{n}` counting from
/// 0, each able to do `abilities`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PinProvider")]
pub struct Provides {
    pub count: u16,
    /// `ch{n}`.
    pub name: String,
    pub abilities: Vec<Need>,
    pub volts: f32,
}

impl Provides {
    /// The pins a part called `part` adds.
    pub fn pins(&self, part: &str) -> Vec<Pin> {
        (0..self.count)
            .map(|n| Pin {
                name: format!("{part}.{}", self.name.replace("{n}", &n.to_string())),
                number: None,
                physical: None,
                digital: self.abilities.iter().any(|a| matches!(a, Need::DigitalIn | Need::DigitalOut)),
                analog: self.abilities.contains(&Need::AnalogIn).then_some(n as u8),
                pwm: self.abilities.contains(&Need::Pwm).then(|| part.to_string()),
                interrupt: None,
                pin_change: false,
                buses: Vec::new(),
                led: false,
                volts: self.volts,
                provided_by: Some(part.to_string()),
            })
            .collect()
    }
}

/// The battery's divider check: the pin must stay below the analog reference when the battery is
/// full.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DividerCheck {
    pub r1: String,
    pub r2: String,
    pub cells: String,
    pub chemistry: String,
    pub full_cell_volts: BTreeMap<String, f64>,
}

/// A part template (`GET /api/v0/arduino/parts`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PartTemplate")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Products it is for: `HC-SR04`, `L298N`.
    pub covers: Vec<String>,
    /// The capability the part has in Faber (`distance`); none for a part that only adds pins.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// The FaberLink class (`FaberUltrasonic`).
    pub class: String,
    /// The header the sketch includes for it.
    pub include: String,
    /// The name a new part gets unless the plan has one already.
    pub default_name: String,
    pub warnings: Vec<String>,
    pub power: Power,
    pub settings: Vec<Setting>,
    /// The shapes that can be used now. The first is the default.
    pub shapes: Vec<Shape>,
    pub uses: Vec<Resource>,
    /// The bus it sits on, whose pins it then takes (`i2c`).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<BusKind>,
    /// Its I²C addresses, the default first.
    pub i2c_addresses: Vec<u8>,
    /// The choice setting that picks its address, whose option ids are the addresses (`0x69`).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_setting: Option<String>,
    /// How to change the address on the part (a pin strap, a solder jumper).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_hint: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provides: Option<Provides>,
    /// C++ written once per part before the declarations (`{var}`, `{name}`, `{board:…}` filled in).
    #[serde(skip)]
    pub helper: Option<String>,
    #[serde(skip)]
    pub divider_check: Option<DividerCheck>,
    /// Shapes whose constructor isn't in the library yet: read and checked, never offered.
    #[serde(skip)]
    pub pending_shapes: Vec<Shape>,
}

impl Template {
    pub fn setting(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }

    /// A shape by its id, or the default (the first) for `None`.
    pub fn shape(&self, id: Option<&str>) -> Option<&Shape> {
        match id {
            None => self.shapes.first(),
            Some(id) => self.shapes.iter().find(|s| s.id == id),
        }
    }

    /// Every shape, the pending ones too.
    pub fn all_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().chain(&self.pending_shapes)
    }

    /// The I²C address this part has with these settings.
    pub fn address(&self, settings: &BTreeMap<String, SettingValue>) -> Option<u8> {
        let default = *self.i2c_addresses.first()?;
        let Some(id) = &self.address_setting else { return Some(default) };
        let chosen = settings.get(id).and_then(SettingValue::as_str).and_then(parse_address);
        Some(chosen.unwrap_or(default))
    }
}

/// `0x68` → 0x68.
pub fn parse_address(s: &str) -> Option<u8> {
    u8::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawTemplate {
    faber_part: u32,
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    covers: Vec<String>,
    capability: Option<String>,
    class: String,
    include: String,
    default_name: String,
    #[serde(default)]
    warnings: Vec<String>,
    helper: Option<String>,
    power: RawPower,
    #[serde(default)]
    settings: Vec<RawSetting>,
    shapes: Vec<RawShape>,
    #[serde(default)]
    uses: Vec<Resource>,
    bus: Option<String>,
    #[serde(default)]
    i2c_addresses: Vec<u8>,
    address_setting: Option<String>,
    address_hint: Option<String>,
    provides: Option<RawProvides>,
    divider_check: Option<DividerCheck>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawPower {
    supply: Supply,
    current_ma: Option<u32>,
    #[serde(default)]
    own_supply: bool,
    logic_volts: Option<f32>,
    max_input_volts: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawSetting {
    id: String,
    name: String,
    unit: Option<String>,
    #[serde(rename = "type")]
    kind: SettingType,
    default: toml::Value,
    min: Option<f64>,
    max: Option<f64>,
    #[serde(default)]
    options: Vec<RawChoice>,
    apply: Apply,
    help: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice {
    id: String,
    name: String,
    code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawShape {
    id: String,
    name: String,
    description: Option<String>,
    todo: Option<String>,
    args: Vec<String>,
    #[serde(default)]
    setup: Vec<String>,
    wiring: Vec<String>,
    roles: Vec<RawRole>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRole {
    id: String,
    name: String,
    needs: String,
    help: String,
    #[serde(default)]
    provided: bool,
    #[serde(default)]
    optional: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvides {
    count: u16,
    name: String,
    abilities: Vec<String>,
    volts: f32,
}

/// A TOML value as a setting's value.
pub fn setting_value(v: &toml::Value) -> Option<SettingValue> {
    match v {
        toml::Value::Integer(i) => Some(SettingValue::Number(*i as f64)),
        toml::Value::Float(f) => Some(SettingValue::Number(*f)),
        toml::Value::String(s) => Some(SettingValue::Text(s.clone())),
        _ => None,
    }
}

fn need(s: &str) -> Result<Need, String> {
    Need::parse(s).ok_or_else(|| format!("\"{s}\" isn't a pin need Faber knows"))
}

/// Reads a template file.
pub fn parse(text: &str) -> Result<Template, String> {
    let raw: RawTemplate = toml::from_str(text).map_err(|e| e.to_string())?;
    if raw.faber_part != 1 {
        return Err(format!("faber-part = {} isn't a version this Faber reads", raw.faber_part));
    }
    let settings = raw
        .settings
        .into_iter()
        .map(|s| {
            Ok(Setting {
                default: setting_value(&s.default)
                    .ok_or_else(|| format!("setting {}: its default must be a number or a text", s.id))?,
                id: s.id,
                name: s.name,
                unit: s.unit,
                kind: s.kind,
                min: s.min,
                max: s.max,
                options: s
                    .options
                    .into_iter()
                    .map(|o| Choice { id: o.id, name: o.name, code: o.code })
                    .collect(),
                apply: s.apply,
                help: s.help,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut shapes = Vec::new();
    let mut pending_shapes = Vec::new();
    for s in raw.shapes {
        let shape = Shape {
            args: s.args.iter().map(|a| Arg::parse(a)).collect::<Result<_, _>>()?,
            roles: s
                .roles
                .into_iter()
                .map(|r| {
                    Ok(Role {
                        needs: need(&r.needs)?,
                        id: r.id,
                        name: r.name,
                        help: r.help,
                        provided: r.provided,
                        optional: r.optional,
                    })
                })
                .collect::<Result<_, String>>()?,
            id: s.id,
            name: s.name,
            description: s.description,
            wiring: s.wiring,
            setup: s.setup,
            todo: s.todo,
        };
        if shape.todo.is_some() { pending_shapes.push(shape) } else { shapes.push(shape) }
    }
    let bus = match raw.bus.as_deref() {
        None => None,
        Some("i2c") => Some(BusKind::I2c),
        Some("spi") => Some(BusKind::Spi),
        Some(other) => return Err(format!("bus = \"{other}\" isn't i2c or spi")),
    };
    let provides = match raw.provides {
        None => None,
        Some(p) => Some(Provides {
            count: p.count,
            name: p.name,
            abilities: p.abilities.iter().map(|a| need(a)).collect::<Result<_, _>>()?,
            volts: p.volts,
        }),
    };
    Ok(Template {
        id: raw.id,
        name: raw.name,
        description: raw.description,
        covers: raw.covers,
        capability: raw.capability,
        class: raw.class,
        include: raw.include,
        default_name: raw.default_name,
        warnings: raw.warnings,
        power: Power {
            supply: raw.power.supply,
            current_ma: raw.power.current_ma,
            own_supply: raw.power.own_supply,
            logic_volts: raw.power.logic_volts,
            max_input_volts: raw.power.max_input_volts,
        },
        settings,
        shapes,
        uses: raw.uses,
        bus,
        i2c_addresses: raw.i2c_addresses,
        address_setting: raw.address_setting,
        address_hint: raw.address_hint,
        provides,
        helper: raw.helper,
        divider_check: raw.divider_check,
        pending_shapes,
    })
}

/// Every template, in catalogue order.
pub fn templates() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(|| {
        FILES
            .iter()
            .map(|(id, text)| {
                // The files are part of the binary and the tests check every one of them.
                parse(text).unwrap_or_else(|e| panic!("the part template {id} is invalid: {e}"))
            })
            .collect()
    })
}

/// A set of templates the allocator and the generator look parts up in: the built-in ones, or
/// the tests' own.
pub trait Catalog {
    fn template(&self, id: &str) -> Option<&Template>;
}

/// The built-in templates.
pub struct BuiltIn;

impl Catalog for BuiltIn {
    fn template(&self, id: &str) -> Option<&Template> {
        templates().iter().find(|t| t.id == id)
    }
}

impl Catalog for [Template] {
    fn template(&self, id: &str) -> Option<&Template> {
        self.iter().find(|t| t.id == id)
    }
}

impl Catalog for Vec<Template> {
    fn template(&self, id: &str) -> Option<&Template> {
        self.iter().find(|t| t.id == id)
    }
}

/// Checks a template's internal consistency: every argument and wiring line names a role or a
/// setting it has, defaults fit, ids are unique. Used by the tests on every template.
pub fn check(t: &Template) -> Result<(), String> {
    let kebab = |s: &str| {
        s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    if !kebab(&t.id) {
        return Err(format!("the id \"{}\" isn't lowercase-with-dashes", t.id));
    }
    crate::names::PartName::parse(&t.default_name)
        .map_err(|e| format!("default-name \"{}\": {}", t.default_name, e.message))?;
    if t.shapes.is_empty() {
        return Err("no shape can be used now".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for s in &t.settings {
        if !kebab(&s.id) || !ids.insert(&s.id) {
            return Err(format!("setting \"{}\": not kebab-case, or twice", s.id));
        }
        s.check(&s.default).map_err(|e| format!("setting {}'s default: {e}", s.id))?;
        if (s.kind == SettingType::Choice) == s.options.is_empty() {
            return Err(format!("setting {}: a choice has options, nothing else does", s.id));
        }
    }
    let mut shape_ids = std::collections::BTreeSet::new();
    for shape in t.all_shapes() {
        if !kebab(&shape.id) || !shape_ids.insert(&shape.id) {
            return Err(format!("shape \"{}\": not kebab-case, or twice", shape.id));
        }
        let mut role_ids = std::collections::BTreeSet::new();
        for r in &shape.roles {
            if !kebab(&r.id) || !role_ids.insert(&r.id) {
                return Err(format!("shape {}: role \"{}\" not kebab-case, or twice", shape.id, r.id));
            }
        }
        for a in &shape.args {
            let known = match a {
                Arg::Pin(r) => shape.role(r).is_some(),
                Arg::Setting(s) => t.setting(s).is_some(),
                Arg::Ratio(a, b) => t.setting(a).is_some() && t.setting(b).is_some(),
                Arg::Name | Arg::AnalogReference | Arg::Code(_) => true,
            };
            if !known {
                return Err(format!("shape {}: the argument {a:?} names nothing", shape.id));
            }
        }
        // Optional pins are left out of the call, so they must be its last arguments.
        let first_optional = shape
            .args
            .iter()
            .position(|a| matches!(a, Arg::Pin(r) if shape.role(r).is_some_and(|x| x.optional)));
        if let Some(i) = first_optional
            && !shape.args[i..]
                .iter()
                .all(|a| matches!(a, Arg::Pin(r) if shape.role(r).is_some_and(|x| x.optional)))
        {
            return Err(format!(
                "shape {}: optional pins must be the constructor's last arguments",
                shape.id
            ));
        }
        for r in &shape.roles {
            if !shape.args.contains(&Arg::Pin(r.id.clone())) {
                return Err(format!("shape {}: the role {} isn't passed to the constructor", shape.id, r.id));
            }
            if !shape.wiring.iter().any(|w| w.contains(&format!("{{pin:{}}}", r.id))) {
                return Err(format!("shape {}: no wiring line for the role {}", shape.id, r.id));
            }
        }
        for line in &shape.wiring {
            for token in placeholders(line) {
                let ok = match token.split_once(':') {
                    Some(("pin", r)) => shape.role(r).is_some(),
                    Some(("setting", s)) => t.setting(s).is_some(),
                    _ => false,
                };
                if !ok {
                    return Err(format!(
                        "shape {}: \"{{{token}}}\" in a wiring line names nothing",
                        shape.id
                    ));
                }
            }
        }
    }
    if let Some(d) = &t.divider_check {
        for s in [&d.r1, &d.r2, &d.cells, &d.chemistry] {
            t.setting(s).ok_or_else(|| format!("divider-check names no setting {s}"))?;
        }
        let chem = t.setting(&d.chemistry).expect("just checked");
        for o in &chem.options {
            if !d.full_cell_volts.contains_key(&o.id) {
                return Err(format!("divider-check has no full-cell-volts for {}", o.id));
            }
        }
    }
    if let Some(a) = &t.address_setting {
        let s = t.setting(a).ok_or_else(|| format!("address-setting names no setting {a}"))?;
        for o in &s.options {
            let addr = parse_address(&o.id).ok_or_else(|| format!("address option {} isn't 0x..", o.id))?;
            if !t.i2c_addresses.contains(&addr) {
                return Err(format!("address option {} isn't in i2c-addresses", o.id));
            }
        }
    }
    if !t.i2c_addresses.is_empty() && t.bus != Some(BusKind::I2c) {
        return Err("i2c-addresses without bus = \"i2c\"".into());
    }
    Ok(())
}

/// The `{…}` tokens in a text (wiring lines; C++ braces with a space or a newline inside aren't
/// tokens).
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else { break };
        let token = &after[..j];
        if !token.is_empty() && !token.contains(char::is_whitespace) {
            out.push(token);
        }
        rest = &after[j + 1..];
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn every_template_parses_and_is_consistent() {
        assert_eq!(templates().len(), FILES.len());
        for (id, _) in FILES {
            let t = BuiltIn.template(id).unwrap_or_else(|| panic!("{id}: the file's id differs"));
            check(t).unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }

    #[test]
    fn templates_carry_what_the_wizard_shows() {
        let u = BuiltIn.template("ultrasonic").unwrap();
        assert_eq!(u.class, "FaberUltrasonic");
        assert_eq!(u.capability.as_deref(), Some("distance"));
        let shape = u.shape(None).unwrap();
        assert_eq!(
            shape.roles.iter().map(|r| (r.id.as_str(), r.needs)).collect::<Vec<_>>(),
            [("trig", Need::DigitalOut), ("echo", Need::DigitalIn)]
        );
        assert!(shape.wiring.contains(&"VCC → 5V".to_string()));
        let m = BuiltIn.template("motor").unwrap();
        assert!(m.power.own_supply);
        assert!(m.warnings.iter().any(|w| w.contains("own supply")));
        assert_eq!(
            m.shapes.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["pwm-dir", "en-in1-in2", "two-pwm"],
            "FaberMotor's three driver shapes"
        );
        assert!(m.pending_shapes.is_empty(), "every motor shape works today");
        let b = BuiltIn.template("battery").unwrap();
        assert_eq!(b.setting("chemistry").unwrap().kind, SettingType::Choice);
        assert!(b.settings.iter().all(|s| s.apply == Apply::Build), "every setting is compiled in today");
        // The API never shows the C++ side.
        let json = serde_json::to_value(b).unwrap();
        assert!(json.get("helper").is_none() && json.get("pendingShapes").is_none());
        assert!(json["settings"][3]["options"][0].get("code").is_none());
    }

    #[test]
    fn settings_check_their_values() {
        let u = BuiltIn.template("ultrasonic").unwrap();
        let max = u.setting("max-m").unwrap();
        assert_eq!(max.check(&SettingValue::Number(2.0)), Ok(()));
        assert_eq!(
            max.check(&SettingValue::Number(5.0)).unwrap_err(),
            "Longest distance can be at most 4 m."
        );
        assert!(max.check(&SettingValue::Text("far".into())).is_err());
        let b = BuiltIn.template("battery").unwrap();
        let cells = b.setting("cells").unwrap();
        assert!(cells.check(&SettingValue::Number(2.5)).unwrap_err().contains("whole number"));
        let chem = b.setting("chemistry").unwrap();
        assert_eq!(chem.check(&SettingValue::Text("lipo".into())), Ok(()));
        assert!(chem.check(&SettingValue::Text("coal".into())).unwrap_err().contains("li-ion"));
        assert_eq!(number_text(20000.0), "20000");
        assert_eq!(number_text(0.02), "0.02");
    }

    #[test]
    fn placeholders_skip_cpp_braces() {
        assert_eq!(placeholders("Trig → {pin:trig}, {setting:r1-ohm} Ω"), ["pin:trig", "setting:r1-ohm"]);
        assert!(placeholders("float f(int raw) {\n  return 1;\n}").is_empty());
    }

    // ---- Drift against the library (docs/wiring.md §4) ----

    /// A constructor's parameter in FaberLink.h.
    #[derive(Debug)]
    pub(crate) struct Param {
        pub ty: String,
        pub has_default: bool,
    }

    /// The library's headers with part classes: FaberLink.h and the optional ones (FaberServo.h).
    fn part_headers() -> String {
        crate::arduino::library::FILES
            .iter()
            .filter(|(p, _)| p.starts_with("src/Faber") && p.ends_with(".h") && *p != "src/FaberLinkCore.h")
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The `Faber*` classes in the library's headers and their constructors.
    pub(crate) fn library_classes() -> BTreeMap<String, Vec<Vec<Param>>> {
        let header = part_headers();
        let code: String =
            header.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
        let mut out = BTreeMap::new();
        let mut rest = code.as_str();
        while let Some(i) = rest.find("class ") {
            let after = &rest[i + "class ".len()..];
            let name: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let Some(open) = after.find('{') else { break };
            // A forward declaration (`class X;`) has a `;` before its body.
            if after[..open].contains(';') {
                rest = &after[after.find(';').unwrap() + 1..];
                continue;
            }
            let mut depth = 0;
            let mut end = after.len();
            for (k, c) in after[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &after[open + 1..end];
            let mut ctors = Vec::new();
            let pattern = format!("{name}(");
            for (k, _) in body.match_indices(&pattern) {
                let before = body[..k].chars().last().unwrap_or(' ');
                if before.is_alphanumeric() || before == '~' || before == '_' {
                    continue;
                }
                let args = &body[k + pattern.len()..];
                let close = args.find(')').expect("a closing parenthesis");
                let params = args[..close]
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        let (decl, default) = p.split_once('=').map_or((p, false), |(d, _)| (d, true));
                        let decl = decl.trim();
                        let ty = decl
                            [..decl.rfind(|c: char| c.is_whitespace() || c == '*' || c == '&').unwrap() + 1]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        Param { ty, has_default: default }
                    })
                    .collect();
                ctors.push(params);
            }
            out.insert(name, ctors);
            rest = &after[end..];
        }
        out
    }

    /// Library part classes that have no template yet, and why.
    const NOT_TEMPLATED: &[(&str, &str)] = &[];

    fn arg_fits(t: &Template, arg: &Arg, ty: &str) -> bool {
        match arg {
            Arg::Name => ty == "const char*",
            Arg::Pin(_) => ty == "uint8_t",
            Arg::Ratio(..) | Arg::AnalogReference => ty == "float",
            Arg::Setting(id) => match t.setting(id).map(|s| s.kind) {
                Some(SettingType::Number) => ty == "float",
                Some(SettingType::Integer) => matches!(ty, "uint8_t" | "uint16_t" | "int" | "unsigned long"),
                Some(SettingType::Choice) => ty.starts_with("faberlink::"),
                None => false,
            },
            Arg::Code(_) => true,
        }
    }

    /// Whether a shape's arguments fit one of the class's constructors.
    fn fits_a_constructor(t: &Template, shape: &Shape, ctors: &[Vec<Param>]) -> bool {
        ctors.iter().any(|params| {
            let required = params.iter().filter(|p| !p.has_default).count();
            (required..=params.len()).contains(&shape.args.len())
                && shape.args.iter().zip(params).all(|(a, p)| arg_fits(t, a, &p.ty))
        })
    }

    /// Every library header: constants such as `DISTANCE_*` live in FaberLinkCore.h, the motor's
    /// `DRIVER_*` in FaberLink.h.
    fn core_header() -> String {
        crate::arduino::library::FILES
            .iter()
            .filter(|(p, _)| p.starts_with("src/") && p.ends_with(".h"))
            .map(|(_, t)| *t)
            .collect()
    }

    fn check_against_library(shapes: impl Fn(&Template) -> Vec<&Shape>) {
        let classes = library_classes();
        for t in templates() {
            let ctors = classes
                .get(&t.class)
                .unwrap_or_else(|| panic!("{}: FaberLink.h has no class {}", t.id, t.class));
            for shape in shapes(t) {
                assert!(
                    fits_a_constructor(t, shape, ctors),
                    "{} shape {}: no {} constructor in FaberLink.h takes {:?}",
                    t.id,
                    shape.id,
                    t.class,
                    shape.args
                );
                for a in &shape.args {
                    if let Arg::Code(c) = a
                        && let Some(name) = c.strip_prefix("faberlink::")
                    {
                        assert!(core_header().contains(name), "{}: {c} isn't in the library's headers", t.id);
                    }
                }
            }
            for s in t.settings.iter().filter(|s| s.kind == SettingType::Choice) {
                for o in &s.options {
                    let name = o.code.strip_prefix("faberlink::").unwrap_or(&o.code);
                    assert!(
                        core_header().contains(name),
                        "{}: {} isn't in the library's headers",
                        t.id,
                        o.code
                    );
                }
            }
        }
    }

    #[test]
    fn drift_every_template_names_a_class_and_constructor_the_library_has() {
        check_against_library(|t| t.shapes.iter().collect());
    }

    #[test]
    fn drift_pending_shapes_match_the_library_too() {
        check_against_library(|t| t.all_shapes().collect());
        // The methods `setup` lines call (`{var}.setMaxVelocity(...)`) exist too.
        let header = part_headers();
        for t in templates() {
            for line in t.all_shapes().flat_map(|s| &s.setup) {
                let method = line.split('.').nth(1).and_then(|m| m.split('(').next()).unwrap_or(line);
                assert!(
                    header.contains(&format!("{method}(")) || core_header().contains(&format!("{method}(")),
                    "{}: {method}",
                    t.id
                );
            }
        }
        assert!(templates().iter().all(|t| t.pending_shapes.is_empty()), "no shape is still marked todo");
    }

    #[test]
    fn drift_every_library_part_class_has_a_template() {
        let classes = library_classes();
        assert!(
            classes.contains_key("FaberLink") && classes["FaberLink"].len() == 2,
            "the parser reads the header"
        );
        assert_eq!(classes["FaberMotor"][0].len(), 4);
        assert!(classes["FaberMotor"][0][3].has_default);
        for name in classes.keys().filter(|c| c.starts_with("Faber") && *c != "FaberLink") {
            let templated = templates().iter().any(|t| &t.class == name);
            let exempt = NOT_TEMPLATED.iter().any(|(c, _)| c == name);
            assert!(
                templated || exempt,
                "FaberLink.h has {name}, but no part template uses it: add one in core/src/arduino/wiring/parts/ (or to NOT_TEMPLATED, with the reason)"
            );
            assert!(!(templated && exempt), "{name} has a template; take it off NOT_TEMPLATED");
        }
    }
}
