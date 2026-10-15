//! A connector package's `faber.toml` (docs/architecture.md §4), read into a [`Manifest`].
//!
//! Built-in connectors declare their manifest in Rust; a packaged connector (wasm or
//! declarative) declares the same thing in this file. Pure parsing: the caller reads the file.

use crate::{
    Choice, ConnectorKind, Manifest, ManifestError, Number, PortRequirement, Problem, Provide, SettingKind,
    SettingSpec, UsbHint,
};
use faber_capabilities::{Capability, Info};
use serde::Deserialize;

/// The file name inside a package folder.
pub const FILE_NAME: &str = "faber.toml";

/// The version of the manifest format this reads: `faber-manifest = 1`, the first line of every
/// `faber.toml`, whatever its kind.
pub const FORMAT: i64 = 1;

/// What `faber.toml` says beyond the [`Manifest`]: how the connector runs.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageManifest {
    pub manifest: Manifest,
    pub kind: ConnectorKind,
    /// The component file next to `faber.toml`, for a wasm connector.
    pub module: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct File {
    faber_manifest: Option<toml::Value>,
    package: Package,
    #[serde(default)]
    ports: Ports,
    #[serde(default)]
    provides: Vec<ProvideEntry>,
    #[serde(default)]
    settings: Vec<SettingEntry>,
    #[serde(default)]
    usb_hints: Vec<UsbHintEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Package {
    id: String,
    #[serde(rename = "type")]
    package_type: String,
    kind: String,
    version: String,
    name: String,
    module: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Ports {
    serial: Option<Serial>,
    #[serde(default)]
    bridge: bool,
}

#[derive(Deserialize)]
struct Serial {
    baud: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProvideEntry {
    capability: String,
    name: Option<String>,
    info: Option<toml::Table>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct SettingEntry {
    key: String,
    label: String,
    description: String,
    kind: String,
    unit: Option<String>,
    default: Option<toml::Value>,
    min: Option<f64>,
    #[serde(default)]
    min_exclusive: bool,
    max: Option<f64>,
    suggested: Option<f64>,
    #[serde(default)]
    choices: Vec<ChoiceEntry>,
    #[serde(default)]
    advanced: bool,
}

#[derive(Deserialize)]
struct ChoiceEntry {
    value: String,
    label: String,
}

#[derive(Deserialize)]
struct UsbHintEntry {
    vid: u16,
    pid: u16,
    reason: String,
}

fn problem(message: impl Into<String>, hint: impl Into<String>) -> Problem {
    Problem::new(message, hint)
}

/// The `kind` in `[package]`, read without checking anything else, so a loader can pick the
/// reader for the file (wasm, declarative, …) and leave other kinds alone. `None`: the file has no
/// readable kind.
pub fn package_kind(text: &str) -> Option<String> {
    let doc: toml::Table = toml::from_str(text).ok()?;
    doc.get("package")?.get("kind")?.as_str().map(str::to_string)
}

/// Reads a `faber.toml`. Errors say what is wrong and how to fix it.
pub fn parse(text: &str) -> Result<PackageManifest, Problem> {
    let file: File = toml::from_str(text).map_err(|e| {
        problem(
            format!("faber.toml can't be read: {}", e.message()),
            "Compare it with the example in docs/architecture.md, section 4.",
        )
    })?;
    check_format(file.faber_manifest.as_ref())?;
    let p = file.package;
    if p.package_type != "connector" {
        return Err(problem(
            format!("faber.toml describes a package of type \"{}\", not a connector.", p.package_type),
            "Use type = \"connector\" in [package].",
        ));
    }
    let kind = match p.kind.as_str() {
        "wasm" => ConnectorKind::Wasm,
        "declarative" => ConnectorKind::Declarative,
        other => {
            return Err(problem(
                format!("A connector package can't be of kind \"{other}\"."),
                "Use kind = \"wasm\" or kind = \"declarative\" in [package].",
            ));
        }
    };
    let port = match (file.ports.serial, file.ports.bridge) {
        (Some(s), false) => PortRequirement::Serial { baud: s.baud },
        (None, true) => PortRequirement::Bridge,
        _ => {
            return Err(problem(
                "faber.toml must name exactly one port the connector needs.",
                "For a serial device add [ports] with serial = { baud = 115200 } (its baud rate).",
            ));
        }
    };
    let provides = file.provides.into_iter().map(provide).collect::<Result<Vec<_>, _>>()?;
    let settings = file.settings.into_iter().map(setting).collect::<Result<Vec<_>, _>>()?;
    let usb_hints =
        file.usb_hints.into_iter().map(|h| UsbHint { vid: h.vid, pid: h.pid, reason: h.reason }).collect();
    let module = match kind {
        ConnectorKind::Wasm => Some(p.module.unwrap_or_else(|| "connector.wasm".into())),
        _ => p.module,
    };
    let manifest =
        Manifest { id: p.id, name: p.name, version: p.version, port, provides, settings, usb_hints };
    manifest
        .validate()
        .map_err(|e| problem(format!("faber.toml isn't a valid connector: {}.", why(&e)), why_hint(&e)))?;
    Ok(PackageManifest { manifest, kind, module })
}

/// The same rule, and words, as the declarative reader: no line is refused, a newer one asks for
/// a newer Faber.
fn check_format(format: Option<&toml::Value>) -> Result<(), Problem> {
    let Some(format) = format else {
        return Err(problem(
            "This faber.toml doesn't say which manifest format it is written in.",
            format!("Add faber-manifest = {FORMAT} as its first line."),
        ));
    };
    match format.as_integer() {
        Some(FORMAT) => Ok(()),
        Some(n) if n > FORMAT => Err(problem(
            format!("This faber.toml is manifest format {n}, but this Faber reads format {FORMAT}."),
            "Update Faber, or use a version of the connector made for this Faber.",
        )),
        _ => Err(problem(
            format!("faber-manifest = {format} is not a manifest format."),
            format!("Write faber-manifest = {FORMAT}."),
        )),
    }
}

fn why(e: &ManifestError) -> String {
    match e {
        ManifestError::BadId(id) => format!("the id \"{id}\" isn't lowercase words joined by -"),
        ManifestError::BadProvideName(n) => {
            format!("the signal name \"{n}\" isn't lowercase words joined by -")
        }
        ManifestError::DuplicateProvideName(n) => format!("two [[provides]] are called \"{n}\""),
        ManifestError::NothingProvided => "it provides nothing".into(),
        ManifestError::InfoMismatch(n) => format!("the info of \"{n}\" is for another capability"),
        ManifestError::BadSetting(why) => why.clone(),
        ManifestError::DuplicateSetting(k) => format!("two [[settings]] have the key \"{k}\""),
    }
}

fn why_hint(e: &ManifestError) -> &'static str {
    match e {
        ManifestError::NothingProvided => "Add a [[provides]] entry with the capability it offers.",
        _ => "Fix that entry in faber.toml; names look like \"laser-scan\" or \"my-sensor\".",
    }
}

fn provide(entry: ProvideEntry) -> Result<Provide, Problem> {
    let capability = Capability::from_id(&entry.capability).ok_or_else(|| {
        let known: Vec<_> = Capability::ALL.iter().map(|c| c.id()).collect();
        problem(
            format!("Faber doesn't know the capability \"{}\".", entry.capability),
            format!("Use one of {}.", known.join(", ")),
        )
    })?;
    let info = entry.info.map(|table| info(capability, table)).transpose()?;
    Ok(Provide { name: entry.name.unwrap_or_else(|| capability.name().into()), capability, info })
}

/// Reads `info = { range-min-m = 0.02, … }` as the capability's info.
fn info(capability: Capability, table: toml::Table) -> Result<Info, Problem> {
    let camel: toml::Table = table.into_iter().map(|(k, v)| (camel_case(&k), v)).collect();
    let mut tagged = toml::Table::new();
    tagged.insert("capability".into(), capability.id().into());
    tagged.insert("info".into(), toml::Value::Table(camel));
    toml::Value::Table(tagged).try_into::<Info>().map_err(|e| {
        problem(
            format!("The info of {} can't be read: {}", capability.id(), e.message()),
            "Check its field names and values against docs/capabilities.md.",
        )
    })
}

fn camel_case(kebab: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in kebab.chars() {
        match c {
            '-' => upper = true,
            c if upper => {
                out.extend(c.to_uppercase());
                upper = false;
            }
            c => out.push(c),
        }
    }
    out
}

fn setting(e: SettingEntry) -> Result<SettingSpec, Problem> {
    let wrong_default = |what: &str| {
        problem(
            format!("The default of setting \"{}\" must be {what}.", e.key),
            format!("Fix default = … in the [[settings]] entry for \"{}\".", e.key),
        )
    };
    let kind = match e.kind.as_str() {
        "number" => SettingKind::Number(Number {
            default: match &e.default {
                Some(v) => v
                    .as_float()
                    .or(v.as_integer().map(|i| i as f64))
                    .ok_or_else(|| wrong_default("a number"))?,
                None => return Err(wrong_default("a number")),
            },
            min: e.min,
            min_exclusive: e.min_exclusive,
            max: e.max,
            suggested: e.suggested,
        }),
        "bool" => SettingKind::Bool {
            default: match &e.default {
                Some(v) => v.as_bool().ok_or_else(|| wrong_default("true or false"))?,
                None => false,
            },
        },
        "choice" => SettingKind::Choice {
            default: match &e.default {
                Some(v) => v.as_str().ok_or_else(|| wrong_default("one of its choices"))?.to_string(),
                None => e.choices.first().map(|c| c.value.clone()).unwrap_or_default(),
            },
            choices: e.choices.iter().map(|c| Choice::new(&c.value, &c.label)).collect(),
        },
        "string" => SettingKind::String {
            default: match &e.default {
                Some(v) => v.as_str().ok_or_else(|| wrong_default("text in quotes"))?.to_string(),
                None => String::new(),
            },
        },
        "part-list" => SettingKind::PartList,
        other => {
            return Err(problem(
                format!("Setting \"{}\" has an unknown kind \"{other}\".", e.key),
                "Use number, bool, choice, string or part-list.",
            ));
        }
    };
    Ok(SettingSpec {
        key: e.key,
        label: e.label,
        description: e.description,
        unit: e.unit,
        kind,
        advanced: e.advanced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::laser_scan;

    const LD19: &str = r#"
        faber-manifest = 1

        [package]
        id = "ldrobot-ld19"
        type = "connector"
        kind = "wasm"
        version = "0.1.0"
        name = "LDROBOT LD19 / STL-19P LiDAR"

        [ports]
        serial = { baud = 230400 }

        [[provides]]
        capability = "laser-scan@1"
        info = { range-min-m = 0.02, range-max-m = 12.0 }

        [[usb-hints]]
        vid = 0x10c4
        pid = 0xea60
        reason = "Often comes with it."
    "#;

    #[test]
    fn reads_a_wasm_connector() {
        let p = parse(LD19).unwrap();
        assert_eq!(p.kind, ConnectorKind::Wasm);
        assert_eq!(p.module.as_deref(), Some("connector.wasm"), "the default file name");
        let m = p.manifest;
        assert_eq!((m.id.as_str(), m.version.as_str()), ("ldrobot-ld19", "0.1.0"));
        assert_eq!(m.port, PortRequirement::Serial { baud: 230_400 });
        assert_eq!(
            m.provides,
            [Provide::with_info(Info::LaserScan(laser_scan::Info { range_min_m: 0.02, range_max_m: 12.0 }))]
        );
        assert_eq!(
            m.usb_hints,
            [UsbHint { vid: 0x10c4, pid: 0xea60, reason: "Often comes with it.".into() }]
        );
        assert!(m.settings.is_empty());
    }

    #[test]
    fn reads_settings_like_the_rust_declarations() {
        let text = format!(
            "{LD19}\n{}",
            r#"
            [[settings]]
            key = "max-m"
            label = "Farthest it can measure"
            description = "Anything farther reads as too far."
            kind = "number"
            unit = "m"
            default = 4.0
            min = 0.0
            min-exclusive = true
            suggested = 2

            [[settings]]
            key = "mode"
            label = "Mode"
            description = "How it measures."
            kind = "choice"
            default = "fast"
            choices = [{ value = "fast", label = "Fast" }, { value = "fine", label = "Fine" }]
            advanced = true

            [[settings]]
            key = "motors"
            label = "Motors"
            description = "What it drives."
            kind = "part-list"
        "#
        );
        let s = parse(&text).unwrap().manifest.settings;
        assert_eq!(
            s,
            [
                SettingSpec::number(
                    "max-m",
                    "Farthest it can measure",
                    "Anything farther reads as too far.",
                    "m",
                    4.0
                )
                .positive()
                .suggested(2.0),
                SettingSpec::choice(
                    "mode",
                    "Mode",
                    "How it measures.",
                    "fast",
                    vec![Choice::new("fast", "Fast"), Choice::new("fine", "Fine")]
                )
                .advanced(),
                SettingSpec::part_list("motors", "Motors", "What it drives."),
            ]
        );
    }

    #[test]
    fn mistakes_are_explained() {
        let cases = [
            ("not toml at all [", "can't be read"),
            (&LD19.replace("faber-manifest = 1", ""), "doesn't say which manifest format"),
            (
                &LD19.replace("faber-manifest = 1", "faber-manifest = 2"),
                "format 2, but this Faber reads format 1",
            ),
            (&LD19.replace("faber-manifest = 1", "faber-manifest = \"one\""), "not a manifest format"),
            (&LD19.replace("type = \"connector\"", "type = \"app\""), "not a connector"),
            (&LD19.replace("kind = \"wasm\"", "kind = \"native\""), "kind \"native\""),
            (&LD19.replace("serial = { baud = 230400 }", ""), "exactly one port"),
            (&LD19.replace("laser-scan@1", "laser-scan@9"), "doesn't know the capability"),
            (&LD19.replace("range-min-m", "range-low"), "info of laser-scan@1"),
            (&LD19.replace("ldrobot-ld19", "LD19"), "isn't a valid connector"),
        ];
        for (text, words) in cases {
            let e = parse(text).err().unwrap_or_else(|| panic!("refused: {text}"));
            assert!(e.message.contains(words), "{words}: {e:?}");
            assert!(!e.hint.is_empty());
        }
    }

    #[test]
    fn a_newer_format_asks_for_a_newer_faber() {
        let e = parse(&LD19.replace("faber-manifest = 1", "faber-manifest = 2")).unwrap_err();
        assert!(e.hint.starts_with("Update Faber"), "{e:?}");
    }

    #[test]
    fn the_kind_is_read_without_reading_the_rest() {
        assert_eq!(package_kind(LD19).as_deref(), Some("wasm"));
        assert_eq!(
            package_kind(
                "[package]
kind = \"declarative\"
[[provides]]
line = 3"
            )
            .as_deref(),
            Some("declarative")
        );
        assert_eq!(
            package_kind(
                "[package]
id = 3"
            ),
            None
        );
        assert_eq!(package_kind("not toml ["), None);
    }

    #[test]
    fn kebab_keys_become_camel_case() {
        assert_eq!(camel_case("range-min-m"), "rangeMinM");
        assert_eq!(camel_case("holonomic"), "holonomic");
    }
}
