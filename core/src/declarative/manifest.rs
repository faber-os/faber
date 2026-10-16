//! Reads a declarative connector's `faber.toml` (docs/connectors-declarative.md).
//!
//! Strict: every key must be known, and every mistake names its line and says what to write
//! instead (docs/principles.md, "Clear errors from the tools too").

use super::lines::Ending;
use super::pattern::{FieldType, Pattern};
use super::values;
use faber_capabilities::Capability;
use faber_connector::settings::Reader;
use faber_connector::{
    Choice, Manifest, Number, PortRequirement, Provide, SettingKind, SettingSpec, Settings, SettingsError,
    is_kebab_name,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use toml::Spanned;

pub const FILE_NAME: &str = "faber.toml";

/// The version of the manifest format this core reads: `faber-manifest = 1`.
pub const FORMAT: i64 = 1;

/// A problem in a `faber.toml`, with where it is and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    /// 1-based, when the problem has a place in the file.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub hint: String,
}

impl ManifestError {
    pub fn at(
        text: &str,
        span: Option<Range<usize>>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        let (line, column) = match span {
            Some(s) => {
                let before = &text[..s.start.min(text.len())];
                let line = before.matches('\n').count() + 1;
                let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
                (Some(line), Some(column))
            }
            None => (None, None),
        };
        ManifestError { line, column, message: message.into(), hint: hint.into() }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "line {line}: ")?;
        }
        write!(f, "{} {}", self.message, self.hint)
    }
}

/// How the stream is cut into lines, and when the part is unwell.
#[derive(Clone, Debug, PartialEq)]
pub struct LineOptions {
    pub ending: Ending,
    /// Bytes; a longer line is a bad line.
    pub max_length: usize,
    /// This many bad lines in a row make the part "degraded".
    pub warn_after_bad_lines: u32,
    /// No good line for this long makes the part "failed".
    pub timeout_us: u64,
}

impl Default for LineOptions {
    fn default() -> Self {
        LineOptions { ending: Ending::Lf, max_length: 256, warn_after_bad_lines: 10, timeout_us: 5_000_000 }
    }
}

/// A number written in the manifest, or the value of one of the part's settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Num {
    Fixed(f64),
    Setting(String),
}

impl Num {
    pub fn resolve(&self, reader: &Reader) -> Result<f64, SettingsError> {
        match self {
            Num::Fixed(v) => Ok(*v),
            Num::Setting(key) => reader.number(key),
        }
    }

    fn is_identity(&self, identity: f64) -> bool {
        *self == Num::Fixed(identity)
    }
}

/// A table whose values may be `{ setting = "key" }`, filled in per part.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Template(toml::Table);

impl Template {
    /// The table with every setting reference replaced by the part's value, as camelCase JSON.
    pub fn resolve(
        &self,
        reader: &Reader,
        specs: &[SettingSpec],
    ) -> Result<Map<String, Value>, SettingsError> {
        Ok(values::toml_to_json(&resolve_table(&self.0, reader, specs)?))
    }

    fn references(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_references(&self.0, &mut out);
        out
    }
}

fn setting_reference(v: &toml::Value) -> Option<&str> {
    let t = v.as_table()?;
    (t.len() == 1).then(|| t.get("setting")?.as_str()).flatten()
}

fn collect_references(t: &toml::Table, out: &mut Vec<String>) {
    for v in t.values() {
        match (setting_reference(v), v) {
            (Some(key), _) => out.push(key.to_string()),
            (None, toml::Value::Table(inner)) => collect_references(inner, out),
            _ => {}
        }
    }
}

fn resolve_table(
    t: &toml::Table,
    reader: &Reader,
    specs: &[SettingSpec],
) -> Result<toml::Table, SettingsError> {
    let mut out = toml::Table::new();
    for (k, v) in t {
        let value = match (setting_reference(v), v) {
            (Some(key), _) => {
                let spec = specs.iter().find(|s| s.key == key).expect("references are checked when loading");
                match spec.kind {
                    SettingKind::Number(_) => toml::Value::Float(reader.number(key)?),
                    SettingKind::Bool { .. } => toml::Value::Boolean(reader.bool(key)?),
                    SettingKind::Choice { .. } => toml::Value::String(reader.choice(key)?.to_string()),
                    SettingKind::String { .. } => toml::Value::String(reader.string(key)?),
                    SettingKind::PartList => {
                        unreachable!("part lists can't be referenced; checked when loading")
                    }
                }
            }
            (None, toml::Value::Table(inner)) => toml::Value::Table(resolve_table(inner, reader, specs)?),
            (None, other) => other.clone(),
        };
        out.insert(k.clone(), value);
    }
    Ok(out)
}

/// One field of a pattern and how its value is scaled: `value = raw × scale + offset`.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRule {
    pub name: String,
    pub ty: FieldType,
    pub scale: Num,
    pub offset: Num,
}

impl FieldRule {
    /// A whole-number field that isn't scaled stays a whole number (e.g. `satellites`).
    pub fn stays_whole(&self) -> bool {
        self.ty.is_whole() && self.scale.is_identity(1.0) && self.offset.is_identity(0.0)
    }
}

/// How a provide's lines are read.
#[derive(Clone, Debug, PartialEq)]
pub enum Parse {
    Pattern {
        pattern: Pattern,
        fields: Vec<FieldRule>,
        /// Fixed reading fields (`set = { status = "ok" }`).
        set: Template,
        /// A required reading field the line doesn't give, which Faber works out instead.
        derive: Derive,
    },
    /// The built-in NMEA reader (`format = "nmea"`), for `position@1`.
    Nmea,
}

/// A required reading field that a pattern rule doesn't give, worked out by the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Derive {
    Nothing,
    /// `distance@1` without a `status` field: from the info's range (or `ok`).
    StatusFromRange,
    /// `battery@1` without a `level` field: from `volts` and the battery's info (chemistry and
    /// cells in series), with the capability's thresholds and hysteresis.
    LevelFromVolts,
}

/// One `[[provides]]`: which signal, and how its lines become readings.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub provide: String,
    pub capability: Capability,
    pub info: Option<Template>,
    pub parse: Parse,
}

/// A whole declarative connector, ready to run.
#[derive(Clone, Debug, PartialEq)]
pub struct Declarative {
    /// What the core shows and checks; the provides carry their info for the default settings.
    pub manifest: Manifest,
    pub lines: LineOptions,
    /// One per provide, in the manifest's order.
    pub rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct Head {
    #[serde(rename = "faber-manifest")]
    format: Option<Spanned<toml::Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawFile {
    #[allow(dead_code)] // checked first, from `Head`
    faber_manifest: toml::Value,
    package: Spanned<RawPackage>,
    ports: Spanned<RawPorts>,
    lines: Option<Spanned<RawLines>>,
    #[serde(default)]
    provides: Vec<Spanned<RawProvide>>,
    #[serde(default)]
    settings: Vec<Spanned<RawSetting>>,
    /// Bridge firmware the package carries; the package checks read it (core/src/packages.rs).
    #[allow(dead_code)]
    #[serde(default)]
    firmware: Vec<toml::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    id: Spanned<String>,
    #[serde(rename = "type")]
    package_type: Spanned<String>,
    kind: Spanned<String>,
    version: Spanned<String>,
    name: Spanned<String>,
    /// Who made it; shown when the package is installed.
    publisher: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPorts {
    serial: Option<Spanned<RawSerial>>,
    i2c: Option<Spanned<toml::Value>>,
    spi: Option<Spanned<toml::Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSerial {
    baud: Spanned<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawLines {
    ending: Option<Spanned<String>>,
    max_length: Option<Spanned<i64>>,
    warn_after_bad_lines: Option<Spanned<i64>>,
    timeout_s: Option<Spanned<f64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvide {
    capability: Spanned<String>,
    name: Option<Spanned<String>>,
    info: Option<Spanned<toml::Table>>,
    line: Option<Spanned<RawLine>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    pattern: Option<Spanned<String>>,
    format: Option<Spanned<String>>,
    scale: Option<Spanned<toml::Value>>,
    offset: Option<Spanned<toml::Value>>,
    fields: Option<Spanned<BTreeMap<String, RawField>>>,
    set: Option<Spanned<toml::Table>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawField {
    scale: Option<toml::Value>,
    offset: Option<toml::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawSetting {
    key: Spanned<String>,
    label: String,
    description: String,
    kind: Spanned<String>,
    unit: Option<String>,
    default: Option<Spanned<toml::Value>>,
    min: Option<f64>,
    min_exclusive: Option<bool>,
    max: Option<f64>,
    suggested: Option<f64>,
    #[serde(default)]
    advanced: bool,
    choices: Option<Vec<RawChoice>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice {
    value: String,
    label: String,
}

/// The `kind` in `[package]`, read without checking anything else, so a loader can pick the
/// reader for the file (declarative, wasm, …). `None`: the file has no readable kind.
pub fn package_kind(text: &str) -> Option<String> {
    let doc: toml::Table = toml::from_str(text).ok()?;
    doc.get("package")?.get("kind")?.as_str().map(str::to_string)
}

/// Reads and checks a declarative connector's `faber.toml`.
pub fn parse(text: &str) -> Result<Declarative, ManifestError> {
    let syntax = |e: toml::de::Error| {
        ManifestError::at(
            text,
            e.span(),
            e.message().trim().to_string(),
            "Fix this line. docs/connectors-declarative.md shows every key a faber.toml can have.",
        )
    };
    let head: Head = toml::from_str(text).map_err(syntax)?;
    check_format(text, head.format)?;
    let raw: RawFile = toml::from_str(text).map_err(syntax)?;
    Builder { text }.build(raw)
}

fn check_format(text: &str, format: Option<Spanned<toml::Value>>) -> Result<(), ManifestError> {
    let Some(format) = format else {
        return Err(ManifestError::at(
            text,
            Some(0..0),
            "This faber.toml doesn't say which manifest format it is written in.",
            format!("Add faber-manifest = {FORMAT} as its first line."),
        ));
    };
    match format.get_ref().as_integer() {
        Some(FORMAT) => Ok(()),
        Some(n) if n > FORMAT => Err(ManifestError::at(
            text,
            Some(format.span()),
            format!("This faber.toml is manifest format {n}, but this Faber reads format {FORMAT}."),
            "Update Faber, or use a version of the connector made for this Faber.",
        )),
        _ => Err(ManifestError::at(
            text,
            Some(format.span()),
            format!("faber-manifest = {} is not a manifest format.", format.get_ref()),
            format!("Write faber-manifest = {FORMAT}."),
        )),
    }
}

struct Builder<'a> {
    text: &'a str,
}

impl Builder<'_> {
    fn err(&self, span: Range<usize>, message: impl Into<String>, hint: impl Into<String>) -> ManifestError {
        ManifestError::at(self.text, Some(span), message, hint)
    }

    fn build(&self, raw: RawFile) -> Result<Declarative, ManifestError> {
        let package = raw.package.get_ref();
        self.check_package(package)?;
        let port = self.port(raw.ports)?;
        let lines = match raw.lines {
            Some(l) => self.lines(l.into_inner())?,
            None => LineOptions::default(),
        };
        let settings = self.settings(raw.settings)?;
        if raw.provides.is_empty() {
            return Err(self.err(
                raw.package.span(),
                "This connector provides nothing.",
                "Add a [[provides]] table with a capability and a line, like capability = \"distance@1\".",
            ));
        }
        let mut rules = Vec::new();
        for p in raw.provides {
            let rule = self.rule(p, &settings, &rules)?;
            rules.push(rule);
        }
        let defaults = Settings::new();
        let reader = Reader::new(&defaults, &settings, "this connector").expect("no settings given");
        let provides = rules
            .iter()
            .map(|r| provide(r, &reader, &settings))
            .collect::<Result<Vec<_>, _>>()
            .expect("each info was checked with the defaults");
        let manifest = Manifest {
            id: package.id.get_ref().clone(),
            name: package.name.get_ref().clone(),
            version: package.version.get_ref().clone(),
            port,
            provides,
            settings,
            usb_hints: Vec::new(),
        };
        Ok(Declarative { manifest, lines, rules })
    }

    fn check_package(&self, p: &RawPackage) -> Result<(), ManifestError> {
        if !is_kebab_name(p.id.get_ref()) {
            return Err(self.err(
                p.id.span(),
                format!("id = \"{}\" is not a valid connector id.", p.id.get_ref()),
                "Use lowercase words joined by -, starting with a letter, like \"acme-range-finder\".",
            ));
        }
        if p.package_type.get_ref() != "connector" {
            return Err(self.err(
                p.package_type.span(),
                format!(
                    "type = \"{}\" is not a package type a faber.toml in connectors/ can have.",
                    p.package_type.get_ref()
                ),
                "Write type = \"connector\".",
            ));
        }
        if p.kind.get_ref() != "declarative" {
            return Err(self.err(
                p.kind.span(),
                format!("kind = \"{}\" is not a declarative connector.", p.kind.get_ref()),
                "Write kind = \"declarative\" for a connector that is only this file.",
            ));
        }
        let version_ok = {
            let parts: Vec<&str> = p.version.get_ref().split('.').collect();
            parts.len() == 3 && parts.iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        if !version_ok {
            return Err(self.err(
                p.version.span(),
                format!("version = \"{}\" is not a version number.", p.version.get_ref()),
                "Use three numbers, like version = \"0.1.0\".",
            ));
        }
        if p.name.get_ref().trim().is_empty() {
            return Err(self.err(
                p.name.span(),
                "The connector has no name.",
                "Give it one people recognise, like name = \"Acme range finder\".",
            ));
        }
        if let Some(publisher) = &p.publisher
            && publisher.get_ref().trim().is_empty()
        {
            return Err(self.err(
                publisher.span(),
                "publisher is empty.",
                "Write who made it, like publisher = \"Your name\", or leave the line out.",
            ));
        }
        Ok(())
    }

    fn port(&self, ports: Spanned<RawPorts>) -> Result<PortRequirement, ManifestError> {
        let span = ports.span();
        let ports = ports.into_inner();
        if let Some(bus) = ports.i2c.as_ref().or(ports.spi.as_ref()) {
            return Err(self.err(
                bus.span(),
                "Declarative connectors can't use I²C or SPI yet; they arrive with the buses in Phase 6.",
                "Use a serial port, or write a wasm connector.",
            ));
        }
        let Some(serial) = ports.serial else {
            return Err(self.err(
                span,
                "The connector names no port.",
                "Add serial = { baud = 9600 } under [ports], with the sensor's baud rate.",
            ));
        };
        let baud = serial.into_inner().baud;
        match u32::try_from(*baud.get_ref()) {
            Ok(b) if (1..=4_000_000).contains(&b) => Ok(PortRequirement::Serial { baud: b }),
            _ => Err(self.err(
                baud.span(),
                format!("baud = {} is not a baud rate.", baud.get_ref()),
                "Use the sensor's baud rate from its datasheet, like baud = 9600 or baud = 115200.",
            )),
        }
    }

    fn lines(&self, l: RawLines) -> Result<LineOptions, ManifestError> {
        let mut o = LineOptions::default();
        if let Some(e) = l.ending {
            o.ending = Ending::parse(e.get_ref()).ok_or_else(|| {
                self.err(
                    e.span(),
                    format!("ending = {:?} is not a line ending.", e.get_ref()),
                    "Use ending = \"\\n\", \"\\r\\n\" or \"\\r\".",
                )
            })?;
        }
        if let Some(m) = l.max_length {
            o.max_length = match usize::try_from(*m.get_ref()) {
                Ok(n) if (1..=65_536).contains(&n) => n,
                _ => {
                    return Err(self.err(
                        m.span(),
                        format!("max-length = {} is not a line length.", m.get_ref()),
                        "Use the longest line the sensor sends, in bytes, between 1 and 65536.",
                    ));
                }
            };
        }
        if let Some(w) = l.warn_after_bad_lines {
            o.warn_after_bad_lines = match u32::try_from(*w.get_ref()) {
                Ok(n) if (1..=100_000).contains(&n) => n,
                _ => {
                    return Err(self.err(
                        w.span(),
                        format!("warn-after-bad-lines = {} is not a number of lines.", w.get_ref()),
                        "Use a whole number of 1 or more, like 10.",
                    ));
                }
            };
        }
        if let Some(t) = l.timeout_s {
            let s = *t.get_ref();
            if !(s > 0.0 && s <= 3600.0) {
                return Err(self.err(
                    t.span(),
                    format!("timeout-s = {s} is not a timeout."),
                    "Use the seconds after which silence means trouble, more than 0, like 5.",
                ));
            }
            o.timeout_us = (s * 1e6) as u64;
        }
        Ok(o)
    }

    fn settings(&self, raw: Vec<Spanned<RawSetting>>) -> Result<Vec<SettingSpec>, ManifestError> {
        let mut specs: Vec<SettingSpec> = Vec::new();
        for s in raw {
            let span = s.span();
            let s = s.into_inner();
            let key_span = s.key.span();
            let spec = self.setting(span.clone(), s)?;
            if let Some(problem) = spec.problem() {
                return Err(self.err(
                    span,
                    format!("This setting can't be used: {problem}."),
                    "Fix it so the default and the suggested value are allowed values.",
                ));
            }
            if specs.iter().any(|o| o.key == spec.key) {
                return Err(self.err(
                    key_span,
                    format!("There are two settings called \"{}\".", spec.key),
                    "Give each setting its own key.",
                ));
            }
            specs.push(spec);
        }
        Ok(specs)
    }

    fn setting(&self, span: Range<usize>, s: RawSetting) -> Result<SettingSpec, ManifestError> {
        let key = s.key.get_ref().clone();
        let kind_name = s.kind.get_ref().as_str();
        let only_for_numbers =
            s.min.is_some() || s.max.is_some() || s.min_exclusive.is_some() || s.suggested.is_some();
        if kind_name != "number" && only_for_numbers {
            return Err(self.err(span, format!("Setting \"{key}\" is a {kind_name}, so it can't have min, max, min-exclusive or suggested."), "Remove them, or make it kind = \"number\"."));
        }
        if kind_name != "choice" && s.choices.is_some() {
            return Err(self.err(
                span,
                format!("Setting \"{key}\" is a {kind_name}, so it can't have choices."),
                "Remove choices, or make it kind = \"choice\".",
            ));
        }
        let default = s.default.as_ref();
        let need_default = |example: &str| {
            self.err(
                span.clone(),
                format!("Setting \"{key}\" needs a default."),
                format!("Add default = {example}: what a part gets when it leaves the setting out."),
            )
        };
        let wrong_default = |d: &Spanned<toml::Value>, what: &str| {
            self.err(
                d.span(),
                format!("The default of \"{key}\" must be {what}, not {}.", d.get_ref()),
                format!("Write a default that is {what}."),
            )
        };
        let kind = match kind_name {
            "number" => {
                let d = default.ok_or_else(|| need_default("1.0"))?;
                let v = d.get_ref().as_float().or_else(|| d.get_ref().as_integer().map(|i| i as f64));
                let v = v.ok_or_else(|| wrong_default(d, "a number"))?;
                SettingKind::Number(Number {
                    default: v,
                    min: s.min,
                    min_exclusive: s.min_exclusive.unwrap_or(false),
                    max: s.max,
                    suggested: s.suggested,
                })
            }
            "bool" => {
                let d = default.ok_or_else(|| need_default("false"))?;
                SettingKind::Bool {
                    default: d.get_ref().as_bool().ok_or_else(|| wrong_default(d, "true or false"))?,
                }
            }
            "string" => {
                let d = default.ok_or_else(|| need_default("\"\""))?;
                SettingKind::String {
                    default: d
                        .get_ref()
                        .as_str()
                        .ok_or_else(|| wrong_default(d, "text in quotes"))?
                        .to_string(),
                }
            }
            "choice" => {
                let choices: Vec<Choice> = s
                    .choices
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| Choice { value: c.value, label: c.label })
                    .collect();
                if choices.is_empty() {
                    return Err(self.err(
                        span,
                        format!("Setting \"{key}\" is a choice without choices."),
                        "Add choices = [{ value = \"a\", label = \"A\" }, …].",
                    ));
                }
                let d = default.ok_or_else(|| need_default(&format!("\"{}\"", choices[0].value)))?;
                let default = d
                    .get_ref()
                    .as_str()
                    .ok_or_else(|| wrong_default(d, "one of the choices' values"))?
                    .to_string();
                SettingKind::Choice { default, choices }
            }
            "part-list" => {
                if let Some(d) = default {
                    return Err(self.err(
                        d.span(),
                        format!("Setting \"{key}\" is a part list, which starts empty and has no default."),
                        "Remove default.",
                    ));
                }
                SettingKind::PartList
            }
            other => {
                return Err(self.err(
                    s.kind.span(),
                    format!("kind = \"{other}\" is not a kind of setting."),
                    "Use number, bool, choice, string or part-list.",
                ));
            }
        };
        Ok(SettingSpec {
            key,
            label: s.label,
            description: s.description,
            unit: s.unit,
            kind,
            advanced: s.advanced,
        })
    }

    fn rule(
        &self,
        p: Spanned<RawProvide>,
        settings: &[SettingSpec],
        before: &[Rule],
    ) -> Result<Rule, ManifestError> {
        let span = p.span();
        let p = p.into_inner();
        let cap_span = p.capability.span();
        let Some(capability) = Capability::from_id(p.capability.get_ref()) else {
            let ids: Vec<&str> = Capability::ALL.iter().map(|c| c.id()).collect();
            return Err(self.err(
                cap_span,
                format!("capability = \"{}\" is not a capability this Faber knows.", p.capability.get_ref()),
                format!("Use one of {}. Capabilities are versioned, like \"distance@1\".", ids.join(", ")),
            ));
        };
        if capability.lease().is_some() {
            return Err(self.err(
                cap_span,
                format!(
                    "{} parts take commands, and a declarative connector can only read lines.",
                    capability.id()
                ),
                "Write a wasm connector for hardware that is commanded.",
            ));
        }
        if capability == Capability::LaserScan {
            return Err(self.err(
                cap_span,
                "A laser-scan@1 reading is a whole scan, which one text line can't hold.",
                "Write a wasm connector for a LiDAR.",
            ));
        }
        let name = match &p.name {
            Some(n) => {
                if !is_kebab_name(n.get_ref()) {
                    return Err(self.err(
                        n.span(),
                        format!("name = \"{}\" is not a signal name.", n.get_ref()),
                        "Use lowercase words joined by -, like name = \"front\".",
                    ));
                }
                n.get_ref().clone()
            }
            None => capability.name().to_string(),
        };
        if before.iter().any(|r| r.provide == name) {
            return Err(self.err(
                span,
                format!("Two provides are called \"{name}\"."),
                "Give each one its own name = \"…\" (for example \"front\" and \"rear\").",
            ));
        }
        let info = match p.info {
            None => None,
            Some(i) => {
                let template = Template(i.get_ref().clone());
                self.check_references(i.span(), &template, settings)?;
                let defaults = Settings::new();
                let reader = Reader::new(&defaults, settings, "this connector").expect("no settings given");
                let fields = template.resolve(&reader, settings).expect("references are checked");
                let given = values::leaf_paths(&fields);
                match values::info(capability, fields) {
                    Ok(info) => {
                        if let Some(unknown) = values::unknown_paths(&given, &info, "info").first() {
                            return Err(self.err(
                                i.span(),
                                format!("{} info has no `{unknown}`.", capability.id()),
                                "Check the spelling against docs/capabilities.md.",
                            ));
                        }
                    }
                    Err(why) if why.contains("unknown variant") => {
                        return Err(self.err(
                            i.span(),
                            format!("{} has no info.", capability.id()),
                            "Remove info.",
                        ));
                    }
                    Err(why) => {
                        return Err(self.err(
                            i.span(),
                            format!("This info isn't {} info: {why}.", capability.id()),
                            "docs/capabilities.md lists each capability's info.",
                        ));
                    }
                }
                Some(template)
            }
        };
        let Some(line) = p.line else {
            return Err(self.err(
                span,
                format!("The provide \"{name}\" doesn't say how to read it."),
                "Add line = { pattern = \"D:{meters:f32}\" }, or line = { format = \"nmea\" } for a GPS.",
            ));
        };
        let parse = self.parse(line, capability, info.is_some(), settings)?;
        Ok(Rule { provide: name, capability, info, parse })
    }

    fn check_references(
        &self,
        span: Range<usize>,
        t: &Template,
        settings: &[SettingSpec],
    ) -> Result<(), ManifestError> {
        for key in t.references() {
            match settings.iter().find(|s| s.key == key) {
                None => {
                    return Err(self.err(
                        span,
                        format!("{{ setting = \"{key}\" }} refers to a setting that isn't declared."),
                        format!("Add a [[settings]] table with key = \"{key}\", or fix the spelling."),
                    ));
                }
                Some(s) if s.kind == SettingKind::PartList => {
                    return Err(self.err(
                        span,
                        format!("\"{key}\" is a part list, which can't be a value here."),
                        "Refer to a number, bool, choice or string setting.",
                    ));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn num(
        &self,
        span: Range<usize>,
        what: &str,
        v: &toml::Value,
        settings: &[SettingSpec],
    ) -> Result<Num, ManifestError> {
        if let Some(key) = setting_reference(v) {
            return match settings.iter().find(|s| s.key == key) {
                Some(SettingSpec { kind: SettingKind::Number(_), .. }) => Ok(Num::Setting(key.to_string())),
                Some(_) => Err(self.err(
                    span,
                    format!("{what} refers to \"{key}\", which is not a number setting."),
                    "Refer to a setting with kind = \"number\".",
                )),
                None => Err(self.err(
                    span,
                    format!("{what} refers to the setting \"{key}\", which isn't declared."),
                    format!("Add a [[settings]] table with key = \"{key}\" and kind = \"number\"."),
                )),
            };
        }
        match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
            Some(x) if x.is_finite() => Ok(Num::Fixed(x)),
            _ => Err(self.err(span, format!("{what} = {v} is not a number."), format!("Write a number, like {what} = 0.01, or a setting, like {what} = {{ setting = \"{what}\" }}."))),
        }
    }

    fn parse(
        &self,
        line: Spanned<RawLine>,
        capability: Capability,
        has_info: bool,
        settings: &[SettingSpec],
    ) -> Result<Parse, ManifestError> {
        let span = line.span();
        let l = line.into_inner();
        match (&l.pattern, &l.format) {
            (Some(_), Some(f)) => {
                return Err(self.err(
                    f.span(),
                    "A line has either a pattern or a format, not both.",
                    "Remove one of them.",
                ));
            }
            (None, None) => {
                return Err(self.err(
                    span,
                    "This line has no pattern.",
                    "Add pattern = \"D:{meters:f32}\", with a field for each value the sensor sends.",
                ));
            }
            (None, Some(f)) => {
                if f.get_ref() != "nmea" {
                    return Err(self.err(
                        f.span(),
                        format!("format = \"{}\" is not a line format Faber knows.", f.get_ref()),
                        "Use format = \"nmea\" for a GPS, or a pattern.",
                    ));
                }
                if capability != Capability::Position {
                    return Err(self.err(
                        f.span(),
                        format!("The nmea format gives position@1 readings, not {}.", capability.id()),
                        "Write capability = \"position@1\".",
                    ));
                }
                if l.scale.is_some() || l.offset.is_some() || l.fields.is_some() || l.set.is_some() {
                    return Err(self.err(span, "The nmea format reads everything itself, so it takes no scale, offset, fields or set.", "Remove them."));
                }
                return Ok(Parse::Nmea);
            }
            (Some(_), None) => {}
        }
        let raw_pattern = l.pattern.expect("checked above");
        let pattern = Pattern::parse(raw_pattern.get_ref())
            .map_err(|e| self.err(raw_pattern.span(), e.message, e.hint))?;
        let typed = pattern.fields();
        let mut fields: Vec<FieldRule> = typed
            .iter()
            .map(|(name, ty)| FieldRule {
                name: name.to_string(),
                ty: *ty,
                scale: Num::Fixed(1.0),
                offset: Num::Fixed(0.0),
            })
            .collect();
        for (what, v) in [("scale", &l.scale), ("offset", &l.offset)] {
            let Some(v) = v else { continue };
            if fields.len() != 1 {
                return Err(self.err(
                    v.span(),
                    format!(
                        "A {what} on the line is for a pattern with one field, and this one has {}.",
                        fields.len()
                    ),
                    format!("Put it under the field instead: fields.{}.{what} = …", fields[0].name),
                ));
            }
            let n = self.num(v.span(), what, v.get_ref(), settings)?;
            if what == "scale" { fields[0].scale = n } else { fields[0].offset = n }
        }
        if let Some(per_field) = &l.fields {
            for (name, f) in per_field.get_ref() {
                let Some(rule) = fields.iter_mut().find(|r| &r.name == name) else {
                    return Err(self.err(
                        per_field.span(),
                        format!("fields.{name} is not a field of the pattern."),
                        format!(
                            "The pattern's fields are {}.",
                            typed.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                        ),
                    ));
                };
                if let Some(s) = &f.scale {
                    rule.scale = self.num(per_field.span(), "scale", s, settings)?;
                }
                if let Some(o) = &f.offset {
                    rule.offset = self.num(per_field.span(), "offset", o, settings)?;
                }
            }
        }
        let set = match &l.set {
            Some(s) => {
                let t = Template(s.get_ref().clone());
                self.check_references(s.span(), &t, settings)?;
                t
            }
            None => Template::default(),
        };
        // Try the rule once with stand-in values, so a wrong field name shows up now, with its
        // line, and not as bad lines on a running part.
        let defaults = Settings::new();
        let reader = Reader::new(&defaults, settings, "this connector").expect("no settings given");
        let mut trial = set.resolve(&reader, settings).expect("references are checked");
        let set_paths = values::leaf_paths(&trial);
        for f in &fields {
            if f.name == "t-us" {
                return Err(self.err(
                    raw_pattern.span(),
                    "t-us is the time the line arrived, which the core fills in.",
                    "Remove the field, or use {_} to skip that text.",
                ));
            }
            if set_paths.iter().any(|p| p == &f.name || p.starts_with(&format!("{}.", f.name))) {
                return Err(self.err(
                    raw_pattern.span(),
                    format!("\"{}\" is both a pattern field and in set.", f.name),
                    "Remove it from one of them.",
                ));
            }
            let stand_in = if f.stays_whole() { Value::from(1) } else { Value::from(1.5) };
            values::set_path(&mut trial, &f.name, stand_in);
        }
        let derive = match capability {
            Capability::Distance if !values::has_path(&trial, "status") => Derive::StatusFromRange,
            Capability::Battery if !values::has_path(&trial, "level") => Derive::LevelFromVolts,
            _ => Derive::Nothing,
        };
        match derive {
            Derive::StatusFromRange => values::set_path(&mut trial, "status", "ok".into()),
            Derive::LevelFromVolts if !has_info => {
                return Err(self.err(
                    span,
                    "Faber works out a battery's level from its voltage, and needs its info for that.",
                    "Add info = { chemistry = \"li-ion\", cells-in-series = 3 } to the provide (with your                      battery's chemistry and cell count), or a level field.",
                ));
            }
            Derive::LevelFromVolts => values::set_path(&mut trial, "level", "ok".into()),
            Derive::Nothing => {}
        }
        let given: Vec<String> = fields.iter().map(|f| f.name.clone()).chain(set_paths).collect();
        let reading = values::reading(capability, 0, trial.clone()).map_err(|why| {
            // Which pattern field makes it fail? The one whose removal changes the reason.
            let culprit = fields.iter().find(|f| {
                let mut without = trial.clone();
                values::remove_path(&mut without, &f.name);
                values::reading(capability, 0, without).err().as_ref() != Some(&why)
            });
            match culprit {
                Some(f) => self.err(
                    span.clone(),
                    format!("The {} field `{}` can't be read from a number: {why}.", capability.id(), f.name),
                    format!("Give it a fixed value under set instead, like set = {{ {} = … }}.", f.name),
                ),
                None => self.err(
                    span.clone(),
                    format!("This line can't make a {} reading: {why}.", capability.id()),
                    "Name the pattern's fields after the reading's fields, like {meters:f32}, and give fixed \
                     ones under set. docs/capabilities.md lists them.",
                ),
            }
        })?;
        if let Some(unknown) = values::unknown_paths(&given, &reading, "reading").first() {
            return Err(self.err(
                span,
                format!("{} readings have no field `{unknown}`.", capability.id()),
                "Check the spelling against docs/capabilities.md.",
            ));
        }
        Ok(Parse::Pattern { pattern, fields, set, derive })
    }
}

/// What a rule provides for one part's settings: its signal and its resolved info.
pub fn provide(rule: &Rule, reader: &Reader, specs: &[SettingSpec]) -> Result<Provide, SettingsError> {
    let info = match &rule.info {
        None => None,
        Some(t) => {
            let fields = t.resolve(reader, specs)?;
            Some(values::info(rule.capability, fields).map_err(|why| SettingsError {
                key: t.references().first().cloned().unwrap_or_default(),
                message: format!("With these settings the {} info isn't valid: {why}.", rule.capability.id()),
                hint: "Check the settings this connector's info uses.".into(),
            })?)
        }
    };
    Ok(Provide { name: rule.provide.clone(), capability: rule.capability, info })
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{Info, distance};

    const SENSOR: &str = r#"faber-manifest = 1

[package]
id = "example-line-sensor"
type = "connector"
kind = "declarative"
version = "0.1.0"
name = "Example line sensor"

[ports]
serial = { baud = 9600 }

[[provides]]
capability = "distance@1"
line = { pattern = "D:{meters:f32}", scale = 0.01 }   # "D:123" → 1.23 m
"#;

    #[test]
    fn the_example_from_the_design_reads() {
        let d = parse(SENSOR).unwrap();
        assert_eq!(d.manifest.id, "example-line-sensor");
        assert_eq!(d.manifest.port, PortRequirement::Serial { baud: 9600 });
        assert_eq!(d.manifest.provides, [Provide::default_for(Capability::Distance)]);
        assert_eq!(d.manifest.validate(), Ok(()));
        assert_eq!(d.lines, LineOptions::default());
        let Parse::Pattern { fields, derive, .. } = &d.rules[0].parse else { panic!() };
        assert_eq!(fields[0].scale, Num::Fixed(0.01));
        assert_eq!(*derive, Derive::StatusFromRange);
        assert_eq!(package_kind(SENSOR).as_deref(), Some("declarative"));
    }

    #[test]
    fn a_publisher_may_be_named() {
        let text = SENSOR.replace("version = \"0.1.0\"", "version = \"0.1.0\"\npublisher = \"Ada Lovelace\"");
        assert!(parse(&text).is_ok());
    }

    #[test]
    fn everything_a_manifest_can_say() {
        let text = r#"faber-manifest = 1
[package]
id = "acme-ranger"
type = "connector"
kind = "declarative"
version = "1.2.3"
name = "Acme ranger"

[ports]
serial = { baud = 115200 }

[lines]
ending = "\r\n"
max-length = 64
warn-after-bad-lines = 3
timeout-s = 2.5

[[settings]]
key = "max-m"
label = "Farthest it can measure"
description = "Anything farther reads as too far."
kind = "number"
unit = "m"
default = 4.0
min = 0.0
min-exclusive = true

[[settings]]
key = "kind"
label = "Kind"
description = "How it measures."
kind = "choice"
default = "ultrasonic"
choices = [{ value = "ultrasonic", label = "Ultrasonic" }, { value = "infrared", label = "Infrared" }]
advanced = true

[[provides]]
capability = "distance@1"
name = "front"
info = { min-m = 0.02, max-m = { setting = "max-m" }, field-of-view-rad = 0.26, kind = { setting = "kind" } }

[provides.line]
pattern = "R,{meters:u32},{_},{raw:hex}"
fields = { meters = { scale = 0.001 } }
"#;
        let e = parse(text).unwrap_err();
        assert!(e.message.contains("no field `raw`"), "an unknown field is found at load: {e:?}");
        let text = text.replace(",{raw:hex}", "");
        let d = parse(&text).unwrap();
        assert_eq!(
            d.lines,
            LineOptions {
                ending: Ending::CrLf,
                max_length: 64,
                warn_after_bad_lines: 3,
                timeout_us: 2_500_000
            }
        );
        assert_eq!(d.manifest.settings.len(), 2);
        assert!(d.manifest.settings[1].advanced);
        assert_eq!(
            d.manifest.provides[0].info,
            Some(Info::Distance(distance::Info {
                min_m: 0.02,
                max_m: 4.0,
                field_of_view_rad: 0.26,
                kind: distance::Kind::Ultrasonic
            }))
        );
        // A part's own settings change its info.
        let mut s = Settings::new();
        s.insert("max-m".into(), 2.into());
        s.insert("kind".into(), "infrared".into());
        let reader = Reader::new(&s, &d.manifest.settings, "x").unwrap();
        let p = provide(&d.rules[0], &reader, &d.manifest.settings).unwrap();
        assert!(matches!(
            p.info,
            Some(Info::Distance(distance::Info { max_m: 2.0, kind: distance::Kind::Infrared, .. }))
        ));
    }

    /// Replaces `from` in the example with `to` and returns the error.
    fn broken(from: &str, to: &str) -> ManifestError {
        assert!(SENSOR.contains(from), "{from}");
        parse(&SENSOR.replacen(from, to, 1)).expect_err(to)
    }

    #[test]
    fn mistakes_name_their_line_and_say_what_to_write() {
        // (from, to, line, words in the message or hint)
        let cases: &[(&str, &str, usize, &str)] = &[
            ("faber-manifest = 1\n", "", 1, "Add faber-manifest = 1"),
            ("faber-manifest = 1", "faber-manifest = 2", 1, "Update Faber"),
            ("faber-manifest = 1", "faber-manifest = \"one\"", 1, "Write faber-manifest = 1"),
            ("id = \"example-line-sensor\"", "id = \"Example\"", 4, "lowercase words"),
            ("type = \"connector\"", "type = \"app\"", 5, "type = \"connector\""),
            ("kind = \"declarative\"", "kind = \"wasm\"", 6, "kind = \"declarative\""),
            ("version = \"0.1.0\"", "version = \"one\"", 7, "three numbers"),
            ("version = \"0.1.0\"", "version = \"0.1.0\"\npublisher = \" \"", 8, "publisher = \"Your name\""),
            ("name = \"Example line sensor\"", "nmae = \"x\"", 8, "unknown field `nmae`"),
            ("baud = 9600", "baud = 0", 11, "baud rate"),
            ("serial = { baud = 9600 }", "i2c = { address = 0x48 }", 11, "Phase 6"),
            ("\"distance@1\"", "\"distance@2\"", 14, "distance@1"),
            ("\"distance@1\"", "\"motor@1\"", 14, "wasm connector"),
            ("\"distance@1\"", "\"laser-scan@1\"", 14, "wasm connector"),
            ("D:{meters:f32}", "D:", 15, "at least one field like {meters:f32}"),
            ("D:{meters:f32}", "D:{meters:float}", 15, "f32, i32, u32, hex"),
            ("D:{meters:f32}", "D:{meter:f32}", 15, "missing field `meters`"),
            ("D:{meters:f32}\", scale = 0.01", "D:{meters:f32},{status:f32}\"", 15, "status"),
            ("D:{meters:f32}\", scale = 0.01", "D:{meters:f32},{t-us:u32}\"", 15, "core fills in"),
            ("D:{meters:f32}", "D:{meters:f32},{x:f32}", 15, "one field"),
            ("scale = 0.01", "scale = \"big\"", 15, "not a number"),
            ("scale = 0.01", "scale = { setting = \"gain\" }", 15, "isn't declared"),
            ("pattern = \"D:{meters:f32}\", scale = 0.01", "format = \"nmea\"", 15, "position@1"),
            ("pattern = \"D:{meters:f32}\", scale = 0.01", "format = \"csv\"", 15, "nmea"),
            ("line = { pattern", "info = { min-m = 1 }\nline = { pattern", 15, "missing field `max-m`"),
            (
                "line = { pattern",
                "info = { min-m = 0, max-m = 1, field-of-view-rad = 1, kind = \"radar\", far = 2 }\nline = { pattern",
                15,
                "no `far`",
            ),
            ("line = { pattern = \"D:{meters:f32}\", scale = 0.01 }", "", 13, "how to read it"),
        ];
        for (from, to, line, words) in cases {
            let e = broken(from, to);
            assert_eq!(e.line, Some(*line), "{to}: {e}");
            assert!(e.to_string().contains(words), "{to}: {e}");
        }
    }

    #[test]
    fn settings_are_declared_like_the_sdks() {
        let with = |setting: &str| {
            parse(&format!(
                "{SENSOR}\n[[settings]]\nkey = \"gain\"\nlabel = \"Gain\"\ndescription = \"How much.\"\n{setting}\n"
            ))
        };
        let d = with("kind = \"number\"\ndefault = 2\nmin = 0").unwrap();
        assert_eq!(
            d.manifest.settings[0],
            SettingSpec {
                unit: None,
                ..SettingSpec::number("gain", "Gain", "How much.", "", 2.0).at_least(0.0)
            }
        );
        for (setting, words) in [
            ("kind = \"number\"", "needs a default"),
            ("kind = \"number\"\ndefault = true", "must be a number"),
            ("kind = \"number\"\ndefault = -1\nmin = 0", "outside its limits"),
            ("kind = \"bool\"\ndefault = false\nmax = 3", "can't have min"),
            ("kind = \"choice\"\ndefault = \"a\"", "without choices"),
            (
                "kind = \"choice\"\ndefault = \"b\"\nchoices = [{ value = \"a\", label = \"A\" }]",
                "not one of its choices",
            ),
            ("kind = \"colour\"\ndefault = 1", "not a kind of setting"),
        ] {
            let e = with(setting).unwrap_err();
            assert!(e.to_string().contains(words), "{setting}: {e}");
            assert!(e.line.is_some_and(|l| l >= 17), "{setting}: {e}");
        }
    }

    #[test]
    fn nmea_lines_give_positions() {
        let text = SENSOR
            .replace("distance@1", "position@1")
            .replace("pattern = \"D:{meters:f32}\", scale = 0.01", "format = \"nmea\"");
        let d = parse(&text).unwrap();
        assert_eq!(d.rules[0].parse, Parse::Nmea);
        assert_eq!(d.manifest.provides[0].name, "position");
    }

    #[test]
    fn fixed_values_and_nested_fields_fill_other_capabilities() {
        let text = SENSOR.replace("distance@1", "pose@1").replace(
            "line = { pattern = \"D:{meters:f32}\", scale = 0.01 }",
            "line = { pattern = \"P {position.x:f32} {position.y:f32}\", set = { frame = \"odom\", position.z = 0, orientation = { x = 0, y = 0, z = 0, w = 1 } } }",
        );
        let d = parse(&text).unwrap();
        let Parse::Pattern { derive, .. } = d.rules[0].parse else { panic!() };
        assert_eq!(derive, Derive::Nothing);
        let e = parse(&text.replace("frame = \"odom\", ", "")).unwrap_err();
        assert!(e.message.contains("missing field `frame`"), "{e}");
    }
}
