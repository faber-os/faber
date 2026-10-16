//! A guided board's plan, `board.toml` (docs/wiring.md §5): the board type, and each part with
//! its template, shape, pins and settings. Reading points every problem at its line; writing keeps
//! the file's comments (as `parts.toml` does).
//!
//! ```toml
//! faber-board = 1
//! board = "mega2560"
//! name = "mega"
//!
//! [parts.front]
//! template = "ultrasonic"
//! pins = { trig = 7, echo = 6 }
//! settings = { min-m = 0.02, max-m = 4.0 }
//! ```
//!
//! A pin is its number (`7`) or its name (`"A0"`, `"GPIO17"`, `"servos.ch3"`); Faber writes the
//! number for a plain digital pin and the name for every other.

use super::parts::{Catalog, SettingValue, setting_value};
use super::pins::PinMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Range;
use ts_rs::TS;

/// The plan's file in a guided board's folder.
pub const FILE: &str = "board.toml";

/// One part on the board.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlannedPart {
    /// The part template's id (`ultrasonic`).
    pub template: String,
    /// The template's shape; missing means its first.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// Role → pin (`trig` → `D7`).
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    /// Setting id → value. Missing settings take the template's default.
    #[serde(default)]
    pub settings: BTreeMap<String, SettingValue>,
}

/// A guided board's plan (`GET /api/v0/arduino/plans/{name}`, `PUT` the same).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "BoardPlan")]
pub struct Plan {
    /// The board type: a pin map's id (`mega2560`).
    pub board: String,
    /// The board's name: its sketch folder, and the name it tells Faber (`mega`).
    pub name: String,
    /// By the part's name (`front`), which is also its name in Faber (`mega.front`) and its
    /// variable in the sketch.
    #[serde(default)]
    pub parts: BTreeMap<String, PlannedPart>,
    /// When the code was taken over (docs/wiring.md §8): from then on the plan is only a note.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_over: Option<String>,
}

impl Plan {
    pub fn new(board: &str, name: &str) -> Plan {
        Plan { board: board.into(), name: name.into(), parts: BTreeMap::new(), taken_over: None }
    }

    /// The same plan with every pin under its canonical name (`7` → `D7`) and every missing
    /// setting at its template's default. Pins and templates it can't resolve stay as they are,
    /// for the check to report.
    pub fn normalized(&self, map: &PinMap, catalog: &(impl Catalog + ?Sized)) -> Plan {
        let mut out = self.clone();
        for part in out.parts.values_mut() {
            for pin in part.pins.values_mut() {
                if let Some(p) = map.resolve(pin) {
                    *pin = p.name.clone();
                }
            }
            if let Some(t) = catalog.template(&part.template) {
                for s in &t.settings {
                    part.settings.entry(s.id.clone()).or_insert_with(|| s.default.clone());
                }
            }
        }
        out
    }
}

/// A problem in `board.toml`, with where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanError {
    /// 1-based line and column, when the problem has a place in the file.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub hint: String,
}

const FIX_HINT: &str = "Fix this line in board.toml, or edit the plan in Faber.";

/// 1-based line and column of a byte offset.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

impl PlanError {
    fn at(text: &str, span: Option<Range<usize>>, message: String, hint: &str) -> PlanError {
        let (line, column) =
            span.map(|s| line_col(text, s.start)).map_or((None, None), |(l, c)| (Some(l), Some(c)));
        PlanError { line, column, message, hint: hint.into() }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawPlan {
    faber_board: toml::Spanned<i64>,
    board: String,
    name: String,
    taken_over: Option<String>,
    #[serde(default)]
    parts: BTreeMap<String, RawPart>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPart {
    template: String,
    shape: Option<String>,
    #[serde(default)]
    pins: BTreeMap<String, toml::Spanned<toml::Value>>,
    #[serde(default)]
    settings: BTreeMap<String, toml::Spanned<toml::Value>>,
}

/// Reads `board.toml`. Only the file's own form is checked here; what the plan means (pins that
/// can't do the job, two parts on a pin) is the allocator's [`super::allocator::check`].
pub fn parse(text: &str) -> Result<Plan, PlanError> {
    let raw: RawPlan = toml::from_str(text)
        .map_err(|e| PlanError::at(text, e.span(), e.message().trim().to_string(), FIX_HINT))?;
    if *raw.faber_board.get_ref() != 1 {
        return Err(PlanError::at(
            text,
            Some(raw.faber_board.span()),
            format!("faber-board = {} is a plan from a newer Faber.", raw.faber_board.get_ref()),
            "Update Faber, or start a new plan for this board.",
        ));
    }
    let mut parts = BTreeMap::new();
    for (name, p) in raw.parts {
        let mut pins = BTreeMap::new();
        for (role, v) in p.pins {
            let pin = match v.get_ref() {
                toml::Value::Integer(n) => n.to_string(),
                toml::Value::String(s) => s.clone(),
                _ => {
                    return Err(PlanError::at(
                        text,
                        Some(v.span()),
                        format!("The pin of {name}'s {role} must be a number (7) or a name (\"A0\")."),
                        FIX_HINT,
                    ));
                }
            };
            pins.insert(role, pin);
        }
        let mut settings = BTreeMap::new();
        for (id, v) in p.settings {
            let value = setting_value(v.get_ref()).ok_or_else(|| {
                PlanError::at(
                    text,
                    Some(v.span()),
                    format!("The setting {id} of {name} must be a number or a text."),
                    FIX_HINT,
                )
            })?;
            settings.insert(id, value);
        }
        parts.insert(name, PlannedPart { template: p.template, shape: p.shape, pins, settings });
    }
    Ok(Plan { board: raw.board, name: raw.name, parts, taken_over: raw.taken_over })
}

/// Where something in the plan sits in its file: `["parts", "front", "pins", "trig"]`. Falls back
/// to the nearest enclosing key that exists.
pub fn span_of(text: &str, path: &[&str]) -> Option<Range<usize>> {
    let doc = toml_edit::Document::parse(text).ok()?;
    let mut best = None;
    let mut item: &dyn toml_edit::TableLike = doc.as_table();
    for (i, key) in path.iter().enumerate() {
        let Some((k, v)) = item.get_key_value(key) else { break };
        best = k.span().or(best);
        if i + 1 == path.len() {
            // The value's place is more useful than the key's for a pin or a setting.
            best = v.span().or_else(|| v.as_value().and_then(|x| x.span())).or(best);
            break;
        }
        match v.as_table_like() {
            Some(t) => item = t,
            None => break,
        }
    }
    best
}

/// A new plan's header comment.
fn header(name: &str) -> String {
    format!(
        "# The plan for the board \"{name}\": which parts are on its pins. Faber writes {name}.ino from it.\n\
         # Edit it in Faber, or here (Faber checks it when it is read). Your own code goes in my_code.h.\n\n"
    )
}

fn pin_value(map: &PinMap, pin: &str) -> toml_edit::Value {
    match map.resolve(pin) {
        Some(p) if p.code().parse::<i64>().is_ok() => p.code().parse::<i64>().expect("just parsed").into(),
        Some(p) => p.name.as_str().into(),
        None => pin.into(),
    }
}

fn setting_item(v: &SettingValue) -> toml_edit::Value {
    match v {
        SettingValue::Number(n) if n.fract() == 0.0 && n.abs() < 9e15 => (*n as i64).into(),
        SettingValue::Number(n) => (*n).into(),
        SettingValue::Text(s) => s.as_str().into(),
    }
}

/// Writes `plan` into `old` (the file as it is, if there is one), keeping its comments and the
/// order of its parts; new parts go at the end. Pins and settings are written in the template's
/// order.
pub fn write(old: Option<&str>, plan: &Plan, map: &PinMap, catalog: &(impl Catalog + ?Sized)) -> String {
    let fresh = || {
        let start = format!(
            "{}faber-board = 1
board = {}
name = {}
",
            header(&plan.name),
            toml_edit::Value::from(plan.board.as_str()),
            toml_edit::Value::from(plan.name.as_str())
        );
        start.parse::<toml_edit::DocumentMut>().expect("a new plan parses")
    };
    let mut doc = old.and_then(|t| t.parse::<toml_edit::DocumentMut>().ok()).unwrap_or_else(fresh);
    let root = doc.as_table_mut();
    let set = |root: &mut toml_edit::Table, key: &str, value: toml_edit::Value| match root.get_mut(key) {
        Some(item) if item.is_value() => {
            let decor = item.as_value().expect("a value").decor().clone();
            let mut v = value;
            *v.decor_mut() = decor;
            *item = toml_edit::Item::Value(v);
        }
        _ => {
            root.insert(key, toml_edit::Item::Value(value));
        }
    };
    set(root, "faber-board", 1i64.into());
    set(root, "board", plan.board.as_str().into());
    set(root, "name", plan.name.as_str().into());
    match &plan.taken_over {
        Some(when) => set(root, "taken-over", when.as_str().into()),
        None => {
            root.remove("taken-over");
        }
    }
    if !root.contains_key("parts") {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        root.insert("parts", toml_edit::Item::Table(t));
    }
    let parts = root["parts"].as_table_mut().expect("parts is a table");
    let gone: Vec<String> =
        parts.iter().map(|(k, _)| k.to_string()).filter(|k| !plan.parts.contains_key(k)).collect();
    for k in gone {
        parts.remove(&k);
    }
    for (name, part) in &plan.parts {
        let template = catalog.template(&part.template);
        if !parts.contains_key(name) {
            parts.insert(name, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let t = parts[name.as_str()].as_table_mut().expect("a part is a table");
        // Clearing keeps the part's place in the file and its own comment (the header's decor).
        t.clear();
        t.insert("template", toml_edit::value(part.template.as_str()));
        if let Some(shape) = &part.shape {
            t.insert("shape", toml_edit::value(shape.as_str()));
        }
        // The template's order, then anything it doesn't know (the check reports those).
        let role_order: Vec<String> = template
            .and_then(|t| t.all_shapes().find(|s| part.shape.as_ref().is_none_or(|id| &s.id == id)))
            .map(|s| s.roles.iter().map(|r| r.id.clone()).collect())
            .unwrap_or_default();
        let mut pins = toml_edit::InlineTable::new();
        let known_roles = role_order.iter().filter(|r| part.pins.contains_key(*r));
        for role in known_roles.chain(part.pins.keys().filter(|k| !role_order.contains(k))) {
            pins.insert(role, pin_value(map, &part.pins[role]));
        }
        t.insert("pins", toml_edit::value(pins));
        let setting_order: Vec<String> =
            template.map(|t| t.settings.iter().map(|s| s.id.clone()).collect()).unwrap_or_default();
        let mut settings = toml_edit::InlineTable::new();
        let known_settings = setting_order.iter().filter(|s| part.settings.contains_key(*s));
        for id in known_settings.chain(part.settings.keys().filter(|k| !setting_order.contains(k))) {
            settings.insert(id, setting_item(&part.settings[id]));
        }
        if !settings.is_empty() {
            t.insert("settings", toml_edit::value(settings));
        }
    }
    doc.to_string()
}

#[cfg(test)]
mod tests {
    use super::super::parts::BuiltIn;
    use super::super::pins;
    use super::*;

    const EXAMPLE: &str = r#"# My Mega.
faber-board = 1
board = "mega2560"
name = "mega"

# The sonar at the front.
[parts.front]
template = "ultrasonic"
pins = { trig = 7, echo = 6 }
settings = { max-m = 4.0 }

[parts.battery]
template = "battery"
pins = { sense = "A1" }
"#;

    #[test]
    fn reads_the_designs_example() {
        let p = parse(EXAMPLE).unwrap();
        assert_eq!((p.board.as_str(), p.name.as_str()), ("mega2560", "mega"));
        let front = &p.parts["front"];
        assert_eq!(front.template, "ultrasonic");
        assert_eq!(front.shape, None);
        assert_eq!(front.pins["trig"], "7");
        assert_eq!(front.settings["max-m"], SettingValue::Number(4.0));
        assert_eq!(p.parts["battery"].pins["sense"], "A1");
        let mega = pins::arduino("mega2560").unwrap();
        let n = p.normalized(&mega, &BuiltIn);
        assert_eq!(n.parts["front"].pins["trig"], "D7");
        assert_eq!(n.parts["front"].settings["min-m"], SettingValue::Number(0.02), "defaults filled in");
        assert_eq!(n.parts["battery"].settings["chemistry"], SettingValue::Text("li-ion".into()));
    }

    #[test]
    fn errors_point_at_the_line() {
        let e = parse(
            "faber-board = 1\nboard = \"uno\"\nname = \"x\"\n[parts.a]\ntemplate = \"motor\"\ncolour = 3\n",
        )
        .unwrap_err();
        assert_eq!(e.line, Some(6), "{e:?}");
        assert!(e.message.contains("colour"), "{}", e.message);
        let e = parse("faber-board = 2\nboard = \"uno\"\nname = \"x\"\n").unwrap_err();
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("newer Faber"));
        let e = parse("faber-board = 1\nboard = \"uno\"\nname = \"x\"\n[parts.a]\ntemplate = \"motor\"\npins = { pwm = true }\n").unwrap_err();
        assert_eq!(e.line, Some(6));
        let e = parse("board = \"uno\"\nname = \"x\"\n").unwrap_err();
        assert!(e.message.contains("faber-board"), "{}", e.message);
    }

    #[test]
    fn spans_find_pins_and_settings() {
        let (line, _) =
            line_col(EXAMPLE, span_of(EXAMPLE, &["parts", "front", "pins", "echo"]).unwrap().start);
        assert_eq!(line, 9);
        let (line, _) =
            line_col(EXAMPLE, span_of(EXAMPLE, &["parts", "battery", "pins", "nope"]).unwrap().start);
        assert_eq!(line, 14, "falls back to the pins");
        assert_eq!(span_of(EXAMPLE, &["ghost"]), None, "nothing to point at");
    }

    #[test]
    fn writing_keeps_comments_and_round_trips() {
        let mega = pins::arduino("mega2560").unwrap();
        let mut p = parse(EXAMPLE).unwrap().normalized(&mega, &BuiltIn);
        p.parts.get_mut("front").unwrap().pins.insert("echo".into(), "D5".into());
        p.parts.remove("battery");
        let mut motor = PlannedPart { template: "motor".into(), ..Default::default() };
        motor.pins.insert("pwm".into(), "D6".into());
        motor.pins.insert("dir".into(), "A3".into());
        p.parts.insert("left".into(), motor);
        let p = p.normalized(&mega, &BuiltIn);
        let text = write(Some(EXAMPLE), &p, &mega, &BuiltIn);
        assert!(text.starts_with("# My Mega.\n"), "{text}");
        assert!(text.contains("# The sonar at the front.\n[parts.front]"), "{text}");
        assert!(text.contains("pins = { trig = 7, echo = 5 }"), "{text}");
        assert!(text.contains("settings = { min-m = 0.02, max-m = 4 }"), "{text}");
        assert!(!text.contains("battery"), "{text}");
        assert!(
            text.contains("[parts.left]\ntemplate = \"motor\"\npins = { pwm = 6, dir = \"A3\" }"),
            "{text}"
        );
        let again = parse(&text).unwrap().normalized(&mega, &BuiltIn);
        assert_eq!(again, p.normalized(&mega, &BuiltIn));
        assert_eq!(
            write(Some(&text), &again, &mega, &BuiltIn),
            text,
            "writing the same plan changes nothing"
        );
    }

    #[test]
    fn a_new_file_has_a_header_and_taking_over_is_written() {
        let uno = pins::arduino("uno").unwrap();
        let mut p = Plan::new("uno", "rover");
        let text = write(None, &p, &uno, &BuiltIn);
        assert!(text.starts_with("# The plan for the board \"rover\""), "{text}");
        assert!(text.contains("faber-board = 1\nboard = \"uno\"\nname = \"rover\"\n"), "{text}");
        assert_eq!(parse(&text).unwrap(), p);
        p.taken_over = Some("2026-10-16T12:00:00Z".into());
        let taken = write(Some(&text), &p, &uno, &BuiltIn);
        assert_eq!(parse(&taken).unwrap().taken_over.as_deref(), Some("2026-10-16T12:00:00Z"));
    }
}
