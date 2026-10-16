//! The sketch Faber writes from a plan (docs/wiring.md §5): `<name>.ino`, marked as generated,
//! with the includes, one declaration per part, `setup()` starting FaberLink, and `loop()`
//! polling it; your own code is in `my_code.h`, which Faber creates once and never overwrites.
//! The same plan always gives the same bytes.

use super::allocator::{Problem, Severity, check};
use super::parts::{Arg, Catalog, SettingType, SettingValue, Template, number_text};
use super::pins::PinMap;
use super::plan::{Plan, PlannedPart};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The words that mark a sketch as Faber's: while its first line has them, Faber may rewrite the
/// file from the plan. Taking the code over removes them.
pub const GENERATED_MARK: &str = "Faber writes this file from board.toml.";

/// Your own code's tab.
pub const MY_CODE: &str = "my_code.h";

/// The baud rate of the link to Faber.
const BAUD: u32 = 115_200;

/// The firmware version a generated sketch tells Faber.
const VERSION: &str = "guided";

/// A float literal C++ reads as such: `4.0`, `0.02`.
fn cpp_float(n: f64) -> String {
    let t = number_text(n);
    if t.contains('.') || t.contains('e') { t } else { format!("{t}.0") }
}

fn fill(text: &str, name: &str, part: &PlannedPart, map: &PinMap) -> String {
    let mut out = text.replace("{var}", name).replace("{name}", name);
    let aref = map.analog_reference_volts.map_or(0.0, f64::from);
    out = out.replace("{board:analog-reference}", &cpp_float(aref));
    for (role, pin) in &part.pins {
        let shown = map.resolve(pin).map_or_else(|| pin.clone(), |p| p.name.clone());
        out = out.replace(&format!("{{pin:{role}}}"), &shown);
    }
    for (id, value) in &part.settings {
        out = out.replace(&format!("{{setting:{id}}}"), &value.to_string());
    }
    out
}

/// One part's wiring, for the checklist (docs/wiring.md §3, step 4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "PartWiring")]
pub struct Wiring {
    pub part: String,
    pub template: String,
    /// "Trig → D7", in the template's order.
    pub lines: Vec<String>,
    /// The template's warnings for this part.
    pub warnings: Vec<String>,
}

/// Every part's wiring lines, with the plan's pins and settings filled in.
pub fn wiring(plan: &Plan, map: &PinMap, catalog: &(impl Catalog + ?Sized)) -> Vec<Wiring> {
    plan.parts
        .iter()
        .filter_map(|(name, part)| {
            let t = catalog.template(&part.template)?;
            let shape = t.shape(part.shape.as_deref())?;
            Some(Wiring {
                part: name.clone(),
                template: t.id.clone(),
                lines: shape.wiring.iter().map(|l| fill(l, name, part, map)).collect(),
                warnings: t.warnings.clone(),
            })
        })
        .collect()
}

fn arg(t: &Template, a: &Arg, name: &str, part: &PlannedPart, map: &PinMap) -> String {
    let number = |id: &str| part.settings.get(id).and_then(SettingValue::as_f64).unwrap_or(0.0);
    match a {
        Arg::Name => format!("\"{name}\""),
        Arg::Pin(role) => {
            let pin = &part.pins[role];
            map.resolve(pin).map_or_else(|| pin.clone(), |p| p.code())
        }
        Arg::Setting(id) => {
            let s = t.setting(id).expect("checked");
            match (s.kind, part.settings.get(id)) {
                (SettingType::Choice, Some(SettingValue::Text(choice))) => {
                    s.options.iter().find(|o| &o.id == choice).map(|o| o.code.clone()).expect("checked")
                }
                (SettingType::Integer, _) => number_text(number(id)),
                _ => cpp_float(number(id)),
            }
        }
        Arg::Ratio(a, b) => cpp_float((number(a) + number(b)) / number(b)),
        Arg::AnalogReference => cpp_float(map.analog_reference_volts.map_or(0.0, f64::from)),
        Arg::Code(code) => fill(code, name, part, map),
    }
}

/// The header comment: what the file is, and the wiring.
fn header(plan: &Plan, map: &PinMap, catalog: &(impl Catalog + ?Sized)) -> String {
    let mut h = format!(
        "// {name}.ino: {GENERATED_MARK} Don't edit it here: change the\n\
         // board's plan in Faber, or press Take over the code to make the sketch yours.\n\
         // Your own code goes in {MY_CODE}, which Faber never changes.\n\
         //\n\
         // Board: {board} ({id})\n",
        name = plan.name,
        board = map.name,
        id = map.id,
    );
    let wiring = wiring(plan, map, catalog);
    if !wiring.is_empty() {
        h.push_str("//\n// Wiring:\n");
        for w in &wiring {
            h.push_str(&format!("//   {}: {}\n", w.part, w.lines.join(", ")));
        }
    }
    h
}

/// The sketch for a plan. Refuses a plan with errors (with them), so a sketch Faber writes always
/// matches a valid plan.
pub fn sketch(plan: &Plan, map: &PinMap, catalog: &(impl Catalog + ?Sized)) -> Result<String, Vec<Problem>> {
    let errors: Vec<Problem> =
        check(map, plan, catalog, &[]).into_iter().filter(|p| p.severity == Severity::Error).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut out = header(plan, map, catalog);
    out.push('\n');
    let mut includes: Vec<&str> = vec!["FaberLink.h"];
    for part in plan.parts.values() {
        let t = catalog.template(&part.template).expect("checked");
        if !includes.contains(&t.include.as_str()) {
            includes.push(&t.include);
        }
    }
    for i in &includes {
        out.push_str(&format!("#include <{i}>\n"));
    }
    out.push_str("\nFaberLink link(Serial);\n");
    for (name, part) in &plan.parts {
        let t = catalog.template(&part.template).expect("checked");
        let shape = t.shape(part.shape.as_deref()).expect("checked");
        out.push('\n');
        if let Some(helper) = &t.helper {
            out.push_str(fill(helper, name, part, map).trim_start_matches('\n'));
            out.push('\n');
        }
        let pins: Vec<String> = shape
            .roles
            .iter()
            .map(|r| {
                format!("{} {}", r.name, map.resolve(&part.pins[&r.id]).map_or("?", |p| p.name.as_str()))
            })
            .collect();
        out.push_str(&format!("// {name}: {}, {}\n", t.name, pins.join(", ")));
        let args: Vec<String> = shape.args.iter().map(|a| arg(t, a, name, part, map)).collect();
        out.push_str(&format!("{} {name}({});\n", t.class, args.join(", ")));
    }
    out.push_str(&format!(
        "\n#include \"{MY_CODE}\"\n\
         \n\
         void setup() {{\n\
         \x20 link.begin({BAUD}, \"{board}\", \"{VERSION}\");\n\
         \x20 userSetup();\n\
         }}\n\
         \n\
         void loop() {{\n\
         \x20 link.poll();\n\
         \x20 userLoop();\n\
         }}\n",
        board = plan.name.to_ascii_lowercase(),
    ));
    Ok(out)
}

/// `my_code.h` as Faber creates it, once.
pub fn my_code(plan: &Plan) -> String {
    format!(
        "// {MY_CODE}: your own code for the board {name}. Faber never changes this file.\n\
         //\n\
         // userSetup() runs once, after FaberLink and the parts have started. userLoop() runs over\n\
         // and over, after FaberLink has had its turn. The parts in {name}.ino can be used here by\n\
         // their names.\n\
         //\n\
         // Keep userLoop() quick: no delay(), or the board stops answering Faber (time things with\n\
         // millis() instead). Don't print to Serial: the port belongs to FaberLink; use\n\
         // link.log(\"...\") instead.\n\
         \n\
         void userSetup() {{\n\
         }}\n\
         \n\
         void userLoop() {{\n\
         }}\n",
        name = plan.name
    )
}

/// Whether a sketch's text is still Faber's to rewrite.
pub fn is_generated(ino: &str) -> bool {
    ino.lines().next().is_some_and(|l| l.contains(GENERATED_MARK))
}

/// The sketch after "Take over the code" (docs/wiring.md §8): the generated mark and its comment
/// become a note that the code is yours now. Everything else stays byte for byte.
pub fn taken_over(ino: &str, name: &str, when: &str) -> String {
    let body_start = ino.find("\n\n").map_or(0, |i| i + 1);
    let (head, body) = ino.split_at(body_start);
    let wiring: String =
        head.lines().skip_while(|l| !l.starts_with("// Board:")).map(|l| format!("{l}\n")).collect();
    format!(
        "// {name}.ino: your code. Faber wrote it from board.toml until you took it over ({when});\n\
         // it never changes this file again, and board.toml is only a note of how it was planned.\n\
         //\n\
         {wiring}{body}"
    )
}

#[cfg(test)]
mod tests;
