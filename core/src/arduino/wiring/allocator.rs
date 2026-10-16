//! The allocator (docs/wiring.md §4): which pins a new part can go on, and what is wrong with a
//! whole plan. Pure logic over a pin map, a plan and the templates.
//!
//! [`suggest`] answers, for each role of a candidate part: the pin Faber suggests, every other
//! suitable pin, every unsuitable one with its reason, the pins in use and by whom, and the
//! reserved ones with why. [`check`] re-checks a whole plan: pins used twice, pins that can't do
//! the job, the Servo library's and `tone()`'s timers against PWM, I²C pins used as plain pins
//! while an I²C part exists, I²C address clashes, scarce interrupt pins, parts that turn
//! interrupts off, parts that need their own supply, logic levels, and a battery divider that
//! would put too much on its pin.
//!
//! "In use" can also come from outside the plan ([`ExternalUse`]): that is how a code-mode
//! board's reported pins (the `PINS` message, docs/wiring.md §6) will reach the allocator.

use super::parts::{Catalog, Need, Resource, Role, Shape, Supply, Template, number_text};
use super::pins::{BusKind, Pin, PinMap, and_list};
use super::plan::{Plan, PlannedPart};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;

/// A pin something outside the plan uses (a code-mode board's `PINS` report).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalUse {
    pub pin: String,
    /// What uses it, as people should read it: `front (Trig)`.
    pub by: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "PlanProblemSeverity")]
pub enum Severity {
    /// The plan can't be saved (or uploaded) like this.
    Error,
    /// Worth knowing; the plan still works.
    Warning,
}

/// Something wrong with a plan, or worth knowing about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PlanProblem")]
pub struct Problem {
    pub severity: Severity,
    /// Stable and kebab-case: `pin-used-twice`, `servo-timer-conflict`, …
    pub code: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    pub message: String,
    pub hint: String,
    /// Its line in `board.toml`, when the plan was read from the file.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Problem {
    fn new(severity: Severity, code: &str, message: impl Into<String>, hint: impl Into<String>) -> Problem {
        Problem {
            severity,
            code: code.into(),
            part: None,
            role: None,
            setting: None,
            pin: None,
            message: message.into(),
            hint: hint.into(),
            line: None,
        }
    }

    fn error(code: &str, message: impl Into<String>, hint: impl Into<String>) -> Problem {
        Problem::new(Severity::Error, code, message, hint)
    }

    fn warning(code: &str, message: impl Into<String>, hint: impl Into<String>) -> Problem {
        Problem::new(Severity::Warning, code, message, hint)
    }

    fn part(mut self, part: &str) -> Problem {
        self.part = Some(part.into());
        self
    }

    fn role(mut self, role: &str) -> Problem {
        self.role = Some(role.into());
        self
    }

    fn pin(mut self, pin: &str) -> Problem {
        self.pin = Some(pin.into());
        self
    }

    fn setting(mut self, setting: &str) -> Problem {
        self.setting = Some(setting.into());
        self
    }
}

/// A pin with a reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PinReason {
    pub pin: String,
    pub reason: String,
}

/// A pin in use, and by what: `D7`, `front (Trig)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PinUse {
    pub pin: String,
    pub by: String,
}

/// The pins for one role of a candidate part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RoleOptions {
    pub role: String,
    pub name: String,
    pub needs: Need,
    /// Faber's pick (or the one asked for, when it can do the job). None: no pin can.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested: Option<String>,
    /// Every other free pin that can do the job, best first.
    pub suitable: Vec<String>,
    pub unsuitable: Vec<PinReason>,
    pub in_use: Vec<PinUse>,
    pub reserved: Vec<PinReason>,
}

/// What the allocator says about a candidate part (`POST /api/v0/arduino/plans/{name}/suggest`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "PartSuggestion")]
pub struct Suggestion {
    /// The part's name: the one asked for, or a free one from the template's default.
    pub name: String,
    pub template: String,
    pub shape: String,
    pub roles: Vec<RoleOptions>,
    /// The part as it would go into the plan, with the suggested pins and every setting.
    pub part: PlannedPart,
    /// What would be wrong with the plan with this part in it, on the suggested pins.
    pub problems: Vec<Problem>,
}

/// A part to place: its template and shape, its name if it has one (an existing part's name
/// edits that part), pins already chosen, and settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SuggestRequest")]
pub struct Candidate {
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub template: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// Roles whose pin is chosen already.
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default)]
    pub settings: BTreeMap<String, super::parts::SettingValue>,
}

/// Names a part can't have: they would clash with the sketch around it (C++ keywords and the
/// Arduino functions and objects a sketch uses).
const RESERVED_NAMES: &[&str] = &[
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "compl",
    "const",
    "constexpr",
    "const_cast",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    "xor",
    "xor_eq",
    // The sketch's own names, and Arduino's.
    "link",
    "setup",
    "loop",
    "main",
    "faberlink",
    "abs",
    "analogread",
    "analogwrite",
    "attachinterrupt",
    "bit",
    "boolean",
    "byte",
    "constrain",
    "delay",
    "delaymicroseconds",
    "digitalread",
    "digitalwrite",
    "map",
    "max",
    "micros",
    "millis",
    "min",
    "pinmode",
    "pow",
    "pulsein",
    "random",
    "round",
    "sq",
    "sqrt",
    "string",
    "tone",
    "word",
];

/// Why a name can't be a part's in a plan, if it can't.
pub fn check_part_name(name: &str) -> Result<(), String> {
    crate::names::PartName::parse(name).map_err(|e| format!("{} {}", e.message, e.hint))?;
    if RESERVED_NAMES.contains(&name) {
        return Err(format!("\"{name}\" can't be a part's name: the sketch uses it for something else."));
    }
    Ok(())
}

/// The plan's pin map: the board's pins and those its parts provide.
pub fn effective_map(base: &PinMap, plan: &Plan, catalog: &(impl Catalog + ?Sized)) -> PinMap {
    let provided = plan
        .parts
        .iter()
        .filter_map(|(name, p)| Some(catalog.template(&p.template)?.provides.as_ref()?.pins(name)))
        .flatten()
        .collect();
    base.clone().with_provided(provided)
}

/// One use of a pin.
#[derive(Clone, Debug)]
struct Use {
    by: String,
    /// The part and role, for a part's pin.
    part: Option<(String, String)>,
    /// A bus a part in the plan uses.
    bus: bool,
}

/// What the plan takes from the board, apart from a part being edited.
struct Taken {
    /// Pin → its uses.
    pins: BTreeMap<String, Vec<Use>>,
    /// The timers the Servo library and `tone()` take, and why.
    timers: BTreeMap<String, String>,
    /// Buses in use, and the parts on them.
    buses: BTreeMap<BusKind, Vec<String>>,
}

fn bus_name(kind: BusKind) -> &'static str {
    match kind {
        BusKind::I2c => "I²C",
        BusKind::Spi => "SPI",
        BusKind::Serial => "serial",
    }
}

fn role_name(t: Option<&Template>, part: &PlannedPart, role: &str) -> String {
    t.and_then(|t| t.all_shapes().find(|s| part.shape.as_ref().is_none_or(|id| &s.id == id)))
        .and_then(|s| s.role(role))
        .map_or_else(|| role.to_string(), |r| r.name.clone())
}

fn shape_of<'a>(t: &'a Template, part: &PlannedPart) -> Option<&'a Shape> {
    t.shape(part.shape.as_deref())
}

/// How many servos the plan has.
fn servo_count(plan: &Plan, catalog: &(impl Catalog + ?Sized)) -> usize {
    plan.parts
        .values()
        .filter(|p| catalog.template(&p.template).is_some_and(|t| t.uses.contains(&Resource::Servo)))
        .count()
}

fn taken(
    map: &PinMap,
    plan: &Plan,
    catalog: &(impl Catalog + ?Sized),
    external: &[ExternalUse],
    except: Option<&str>,
) -> Taken {
    let mut pins: BTreeMap<String, Vec<Use>> = BTreeMap::new();
    let mut buses: BTreeMap<BusKind, Vec<String>> = BTreeMap::new();
    for (name, part) in &plan.parts {
        let t = catalog.template(&part.template);
        if let Some(bus) = t.and_then(|t| t.bus) {
            buses.entry(bus).or_default().push(name.clone());
        }
        if Some(name.as_str()) == except {
            continue;
        }
        for (role, pin) in &part.pins {
            let pin = map.resolve(pin).map_or_else(|| pin.clone(), |p| p.name.clone());
            pins.entry(pin).or_default().push(Use {
                by: format!("{name} ({})", role_name(t, part, role)),
                part: Some((name.clone(), role.clone())),
                bus: false,
            });
        }
    }
    for (kind, users) in &buses {
        for p in map.bus_pins(*kind) {
            let line = p.bus(*kind).map(|b| b.line.to_uppercase()).unwrap_or_default();
            pins.entry(p.name.clone()).or_default().push(Use {
                by: format!("the {} bus, {line} ({})", bus_name(*kind), users.join(", ")),
                part: None,
                bus: true,
            });
        }
    }
    for e in external {
        let pin = map.resolve(&e.pin).map_or_else(|| e.pin.clone(), |p| p.name.clone());
        pins.entry(pin).or_default().push(Use { by: e.by.clone(), part: None, bus: false });
    }
    let mut timers = BTreeMap::new();
    let servos = servo_count(plan, catalog);
    if servos > 0 && map.servos_per_timer > 0 {
        let n = servos.div_ceil(usize::from(map.servos_per_timer));
        for t in map.servo_timers.iter().take(n) {
            timers.insert(t.clone(), format!("the Servo library takes {} for the servos", timer_name(t)));
        }
    }
    if let Some(tone) = &map.tone_timer
        && plan
            .parts
            .values()
            .any(|p| catalog.template(&p.template).is_some_and(|t| t.uses.contains(&Resource::Tone)))
    {
        timers.insert(tone.clone(), format!("tone() takes {}", timer_name(tone)));
    }
    Taken { pins, timers, buses }
}

/// `timer5` → `Timer5`.
fn timer_name(t: &str) -> String {
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Why `pin` can't do `role`'s job for this board and plan, or `None` if it can.
fn unsuitable(map: &PinMap, taken: &Taken, role: &Role, pin: &Pin) -> Option<String> {
    let board = &map.name;
    if pin.provided_by.is_some() && !role.provided {
        return Some(format!(
            "{} is on {}, not on the board itself; {} needs a pin of the board.",
            pin.name,
            pin.provided_by.as_deref().unwrap_or("a part"),
            role.name
        ));
    }
    if !role.needs.fits(pin) {
        let what = match role.needs {
            Need::DigitalIn | Need::DigitalOut if pin.analog.is_some() => {
                format!("{} on an {board} is analog only.", pin.name)
            }
            Need::DigitalIn | Need::DigitalOut => format!("{} isn't a digital pin.", pin.name),
            Need::AnalogIn => {
                let analog = map.names_where(|p| p.analog.is_some());
                format!("{} isn't an analog input; the {board}'s are {}.", pin.name, span_text(&analog))
            }
            Need::Pwm => {
                let pwm = map.names_where(|p| p.pwm.is_some());
                format!("{} has no PWM; the {board}'s PWM pins are {}.", pin.name, and_list(&pwm))
            }
            Need::Interrupt => {
                let int = map.names_where(|p| p.interrupt.is_some());
                if int.is_empty() {
                    format!("The {board} has no interrupt pins for this.")
                } else {
                    format!("{} has no external interrupt; the {board}'s are {}.", pin.name, and_list(&int))
                }
            }
        };
        return Some(format!("{} needs {}: {what}", role.name, role.needs.words()));
    }
    if role.needs == Need::Pwm
        && let Some(timer) = pin.timer()
        && let Some(why) = taken.timers.get(timer)
    {
        return Some(format!("{}'s PWM is off while {why}.", pin.name));
    }
    None
}

/// "A0–A5" for a run, else the list.
fn span_text(names: &[&str]) -> String {
    match names {
        [first, .., last] if names.len() > 3 => format!("{first}–{last}"),
        _ => and_list(names),
    }
}

/// How much choosing `pin` for `role` costs: scarce abilities the role doesn't need, pins worth
/// keeping free, and the distance from the part's other pins. Lower is better.
fn cost(map: &PinMap, role: &Role, pin: &Pin, index: usize, near: &[usize]) -> i64 {
    let mut c = 0;
    if pin.interrupt.is_some() && role.needs != Need::Interrupt {
        c += 40;
    }
    if pin.pwm.is_some() && role.needs != Need::Pwm {
        c += 20;
    }
    if pin.analog.is_some() && role.needs != Need::AnalogIn {
        c += 10;
    }
    for b in &pin.buses {
        c += match b.kind {
            // A hardware serial port and the I²C bus are worth more than one PWM pin.
            BusKind::I2c | BusKind::Serial => 25,
            BusKind::Spi => 8,
        };
    }
    if pin.led {
        c += 5;
    }
    if map.notes_for(&pin.name).any(|n| n.avoid.iter().any(|a| a == role.needs.id())) {
        c += 30;
    }
    if role.needs == Need::Pwm
        && let Some(timer) = pin.timer()
    {
        // Keep the pins a first servo or tone() would take for last.
        if map.servo_timers.first().map(String::as_str) == Some(timer) {
            c += 6;
        }
        if map.tone_timer.as_deref() == Some(timer) {
            c += 4;
        }
    }
    // Keep a part's pins together.
    if let Some(avg) = (!near.is_empty()).then(|| near.iter().sum::<usize>() / near.len()) {
        c += 2 * (index as i64 - avg as i64).abs();
    }
    c
}

/// A free name for a new part from `base`: `front`, then `front_2`, `front_3` …
pub fn free_name(plan: &Plan, base: &str) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}_{n}") })
        .find(|n| !plan.parts.contains_key(n))
        .expect("a free name")
}

/// Where a new part could go, role by role, and what the plan would look like with it
/// (docs/wiring.md §3, the Pins step). Nothing is written.
pub fn suggest(
    base: &PinMap,
    plan: &Plan,
    catalog: &(impl Catalog + ?Sized),
    external: &[ExternalUse],
    candidate: &Candidate,
) -> Result<Suggestion, Box<Problem>> {
    let template = catalog.template(&candidate.template).ok_or_else(|| {
        Box::new(Problem::error(
            "unknown-template",
            format!("There is no part template called \"{}\".", candidate.template),
            "GET /api/v0/arduino/parts lists them.",
        ))
    })?;
    let shape = shape_or_problem(template, candidate.shape.as_deref())?;
    let name = match &candidate.name {
        Some(n) => {
            check_part_name(n)
                .map_err(|why| Problem::error("invalid-part-name", why, "Pick another name."))?;
            n.clone()
        }
        None => free_name(plan, &template.default_name),
    };
    // The plan as it would be with the candidate in it (on no pins yet), so its own timers and
    // bus count.
    let mut with = plan.clone();
    let mut settings = candidate.settings.clone();
    for s in &template.settings {
        settings.entry(s.id.clone()).or_insert_with(|| s.default.clone());
    }
    with.parts.insert(
        name.clone(),
        PlannedPart {
            template: template.id.clone(),
            shape: candidate.shape.clone(),
            pins: BTreeMap::new(),
            settings: settings.clone(),
        },
    );
    let map = effective_map(base, &with, catalog);
    let taken = taken(&map, &with, catalog, external, Some(&name));
    let index = |p: &str| map.pins.iter().position(|x| x.name == p);

    // Roles with the fewest choices first, so a scarce pin goes where it is needed.
    let options = |role: &Role, used: &BTreeSet<String>| -> Vec<&Pin> {
        map.pins
            .iter()
            .filter(|p| {
                map.reserved_reason(&p.name).is_none()
                    && !taken.pins.contains_key(&p.name)
                    && !used.contains(&p.name)
                    && unsuitable(&map, &taken, role, p).is_none()
            })
            .collect()
    };
    let mut chosen: BTreeMap<String, String> = BTreeMap::new();
    let mut used = BTreeSet::new();
    for (role_id, pin) in &candidate.pins {
        if let Some(p) = map.resolve(pin) {
            chosen.insert(role_id.clone(), p.name.clone());
            used.insert(p.name.clone());
        }
    }
    // An optional pin is only placed when asked for (it is left unconnected otherwise).
    let mut order: Vec<&Role> =
        shape.roles.iter().filter(|r| !r.optional && !chosen.contains_key(&r.id)).collect();
    order.sort_by_key(|r| options(r, &used).len());
    for role in order {
        let near: Vec<usize> = chosen.values().filter_map(|p| index(p)).collect();
        let best = options(role, &used)
            .into_iter()
            .map(|p| {
                let i = index(&p.name).expect("a pin of the map");
                (cost(&map, role, p, i, &near), i, p)
            })
            .min_by_key(|(c, i, _)| (*c, *i));
        if let Some((_, _, p)) = best {
            chosen.insert(role.id.clone(), p.name.clone());
            used.insert(p.name.clone());
        }
    }

    let roles = shape
        .roles
        .iter()
        .map(|role| {
            let suggested = chosen.get(&role.id).cloned();
            let mut others: BTreeSet<String> = chosen.values().cloned().collect();
            if let Some(s) = &suggested {
                others.remove(s);
            }
            let mine: Vec<usize> =
                chosen.iter().filter(|(r, _)| *r != &role.id).filter_map(|(_, p)| index(p)).collect();
            let mut suitable: Vec<(i64, usize, String)> = Vec::new();
            let mut out = RoleOptions {
                role: role.id.clone(),
                name: role.name.clone(),
                needs: role.needs,
                suggested: suggested.clone(),
                suitable: Vec::new(),
                unsuitable: Vec::new(),
                in_use: Vec::new(),
                reserved: Vec::new(),
            };
            for (i, p) in map.pins.iter().enumerate() {
                if Some(&p.name) == suggested.as_ref() {
                    continue;
                }
                if let Some(why) = map.reserved_reason(&p.name) {
                    out.reserved.push(PinReason { pin: p.name.clone(), reason: why.into() });
                } else if let Some(uses) = taken.pins.get(&p.name) {
                    let by = uses.iter().map(|u| u.by.as_str()).collect::<Vec<_>>().join(", ");
                    out.in_use.push(PinUse { pin: p.name.clone(), by });
                } else if others.contains(&p.name) {
                    let r = chosen.iter().find(|(_, v)| *v == &p.name).map(|(r, _)| r.as_str()).unwrap_or("");
                    let by = format!("{name} ({})", shape.role(r).map_or(r, |x| x.name.as_str()));
                    out.in_use.push(PinUse { pin: p.name.clone(), by });
                } else if let Some(why) = unsuitable(&map, &taken, role, p) {
                    out.unsuitable.push(PinReason { pin: p.name.clone(), reason: why });
                } else {
                    suitable.push((cost(&map, role, p, i, &mine), i, p.name.clone()));
                }
            }
            suitable.sort();
            out.suitable = suitable.into_iter().map(|(_, _, n)| n).collect();
            out
        })
        .collect();

    let part = PlannedPart {
        template: template.id.clone(),
        shape: candidate.shape.clone(),
        pins: shape.roles.iter().filter_map(|r| Some((r.id.clone(), chosen.get(&r.id)?.clone()))).collect(),
        settings,
    };
    with.parts.insert(name.clone(), part.clone());
    // What the candidate adds: its own problems, and those it causes for other parts (a servo
    // stopping another part's PWM).
    let mut without = plan.clone();
    without.parts.remove(&name);
    let before = check(base, &without, catalog, external);
    let problems = check(base, &with, catalog, external)
        .into_iter()
        .filter(|p| p.part.as_deref() == Some(name.as_str()) || !before.contains(p))
        .collect();
    Ok(Suggestion { name, template: template.id.clone(), shape: shape.id.clone(), roles, part, problems })
}

fn shape_or_problem<'a>(t: &'a Template, id: Option<&str>) -> Result<&'a Shape, Box<Problem>> {
    if let Some(s) = t.shape(id) {
        return Ok(s);
    }
    let id = id.unwrap_or_default();
    if let Some(pending) = t.pending_shapes.iter().find(|s| s.id == id) {
        return Err(Box::new(Problem::error(
            "shape-not-ready",
            format!(
                "{} as \"{}\" isn't ready yet: the FaberLink library can't drive it.",
                t.name, pending.name
            ),
            "Pick another shape, or write the sketch yourself.",
        )));
    }
    let ids: Vec<&str> = t.shapes.iter().map(|s| s.id.as_str()).collect();
    Err(Box::new(Problem::error(
        "unknown-shape",
        format!("{} has no shape called \"{id}\".", t.name),
        format!("Its shapes: {}.", ids.join(", ")),
    )))
}

/// Everything wrong with a plan, and what is worth knowing about it, part by part. An empty list
/// of errors means it can be saved and its sketch written.
pub fn check(
    base: &PinMap,
    plan: &Plan,
    catalog: &(impl Catalog + ?Sized),
    external: &[ExternalUse],
) -> Vec<Problem> {
    let mut out = Vec::new();
    let map = effective_map(base, plan, catalog);
    let taken = taken(&map, plan, catalog, external, None);

    for (name, part) in &plan.parts {
        if let Err(why) = check_part_name(name) {
            out.push(Problem::error("invalid-part-name", why, "Rename the part.").part(name));
        }
        let Some(t) = catalog.template(&part.template) else {
            out.push(
                Problem::error(
                    "unknown-template",
                    format!("{name}: there is no part template called \"{}\".", part.template),
                    "Remove the part, or pick one from the catalogue.",
                )
                .part(name),
            );
            continue;
        };
        let shape = match shape_or_problem(t, part.shape.as_deref()) {
            Ok(s) => s,
            Err(p) => {
                out.push(p.part(name));
                continue;
            }
        };
        for (id, value) in &part.settings {
            match t.setting(id) {
                None => out.push(
                    Problem::error(
                        "unknown-setting",
                        format!("{name}: {} has no setting called \"{id}\".", t.name),
                        "Remove it from the plan.",
                    )
                    .part(name)
                    .setting(id),
                ),
                Some(s) => {
                    if let Err(why) = s.check(value) {
                        out.push(
                            Problem::error(
                                "invalid-setting",
                                format!("{name}: {why}"),
                                "Change the setting.",
                            )
                            .part(name)
                            .setting(id),
                        );
                    }
                }
            }
        }
        for role in &part.pins.keys().filter(|r| shape.role(r).is_none()).collect::<Vec<_>>() {
            out.push(
                Problem::error(
                    "unknown-role",
                    format!("{name}: {} ({}) has no pin called \"{role}\".", t.name, shape.name),
                    format!(
                        "Its pins: {}.",
                        shape.roles.iter().map(|r| r.id.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                )
                .part(name)
                .role(role),
            );
        }
        for role in &shape.roles {
            let Some(text) = part.pins.get(&role.id) else {
                if role.optional {
                    continue;
                }
                out.push(
                    Problem::error(
                        "missing-pin",
                        format!("{name}: {} has no pin yet.", role.name),
                        "Pick a pin for it.",
                    )
                    .part(name)
                    .role(&role.id),
                );
                continue;
            };
            let Some(pin) = map.resolve(text) else {
                out.push(
                    Problem::error(
                        "unknown-pin",
                        format!("{name}: the {} has no pin \"{text}\" (for {}).", map.name, role.name),
                        "Pick one of the board's pins.",
                    )
                    .part(name)
                    .role(&role.id),
                );
                continue;
            };
            if let Some(why) = map.reserved_reason(&pin.name) {
                out.push(
                    Problem::error(
                        "reserved-pin",
                        format!("{name}: {} can't use {}: {why}", role.name, pin.name),
                        "Pick another pin.",
                    )
                    .part(name)
                    .role(&role.id)
                    .pin(&pin.name),
                );
            }
            if let Some(why) = unsuitable(&map, &taken, role, pin) {
                let code = if why.contains("PWM is off while") {
                    if why.contains("Servo") { "servo-timer-conflict" } else { "tone-timer-conflict" }
                } else {
                    "unsuitable-pin"
                };
                out.push(
                    Problem::error(code, format!("{name}: {why}"), "Pick another pin; Faber suggests one.")
                        .part(name)
                        .role(&role.id)
                        .pin(&pin.name),
                );
            }
        }
        // Logic levels.
        if let Some(v) = t.power.logic_volts
            && v > map.logic_volts + 0.1
        {
            out.push(
                Problem::warning(
                    "level-shift-needed",
                    format!(
                        "{name}: its signals are {} V, but the {}'s pins take {} V.",
                        volts(v),
                        map.name,
                        volts(map.logic_volts)
                    ),
                    "Put a voltage divider or a level shifter on its outputs.",
                )
                .part(name),
            );
        }
        if let Some(v) = t.power.max_input_volts
            && map.logic_volts > v + 0.1
        {
            out.push(
                Problem::warning(
                    "level-shift-needed",
                    format!(
                        "{name}: it takes at most {} V on its inputs, but the {} drives {} V.",
                        volts(v),
                        map.name,
                        volts(map.logic_volts)
                    ),
                    "Use a level shifter between the board and the part.",
                )
                .part(name),
            );
        }
        if t.power.own_supply {
            out.push(
                Problem::warning(
                    "own-supply",
                    format!("{name} needs its own supply: never power it from the board's 5V pin."),
                    "Join the supply's - to the board's GND.",
                )
                .part(name),
            );
        }
        if t.uses.contains(&Resource::InterruptsOff) {
            let busy = plan.parts.values().filter_map(|p| catalog.template(&p.template)).any(|t| {
                t.power.own_supply
                    || t.shapes.iter().any(|s| s.roles.iter().any(|r| r.needs == Need::Interrupt))
            });
            out.push(
                Problem::warning(
                    "interrupts-off",
                    format!(
                        "{name} turns interrupts off while it updates (about 30 µs per LED), so bytes from Faber{} can be lost on a long strip.",
                        if busy { " and encoder edges" } else { "" }
                    ),
                    "A separate small board for the strip is better.",
                )
                .part(name),
            );
        }
        if let Some(d) = &t.divider_check
            && let Some(problem) = divider(&map, name, part, d)
        {
            out.push(problem);
        }
    }

    // Two things on one pin.
    for (pin, uses) in &taken.pins {
        if uses.len() < 2 || map.reserved_reason(pin).is_some() {
            continue;
        }
        let parts: Vec<&Use> = uses.iter().filter(|u| u.part.is_some()).collect();
        if parts.is_empty() {
            continue;
        }
        let all: Vec<&str> = uses.iter().map(|u| u.by.as_str()).collect();
        let bus = uses.iter().any(|u| u.bus);
        let (code, hint) = if bus {
            (
                "bus-pin-used",
                "Move the part to another pin: a bus's pins can't be plain pins while a part uses the bus.",
            )
        } else {
            ("pin-used-twice", "Move one of them to another pin.")
        };
        for u in parts.iter().skip(if bus || parts.len() < uses.len() { 0 } else { 1 }) {
            let (part, role) = u.part.as_ref().expect("filtered");
            out.push(
                Problem::error(code, format!("{pin} is used by {}.", and_list(&all)), hint)
                    .part(part)
                    .role(role)
                    .pin(pin),
            );
        }
    }

    // More servos than the library can drive.
    let servos = servo_count(plan, catalog);
    let max = map.servo_timers.len() * usize::from(map.servos_per_timer);
    if servos > max {
        out.push(Problem::error(
            "too-many-servos",
            format!(
                "The plan has {servos} servos; the Servo library drives at most {max} on the {}.",
                map.name
            ),
            "Put some on a PCA9685 servo board, or on another board.",
        ));
    }

    // I²C addresses.
    let mut addresses: BTreeMap<u8, Vec<(&String, &Template)>> = BTreeMap::new();
    for (name, part) in &plan.parts {
        if let Some(t) = catalog.template(&part.template)
            && let Some(a) = t.address(&part.settings)
        {
            addresses.entry(a).or_default().push((name, t));
        }
    }
    for (addr, users) in addresses.iter().filter(|(_, u)| u.len() > 1) {
        let names: Vec<String> = users.iter().map(|(n, t)| format!("{n} ({})", t.name)).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        for (n, t) in users.iter().skip(1) {
            out.push(
                Problem::error(
                    "i2c-address-clash",
                    format!("{} share the I²C address 0x{addr:02X}.", and_list(&names)),
                    t.address_hint.clone().unwrap_or_else(|| "Give one of them another address.".into()),
                )
                .part(n),
            );
        }
    }

    // Interrupt pins are scarce: plain uses of all of them leave nothing for an encoder.
    let interrupt_pins: Vec<&Pin> = map
        .pins
        .iter()
        .filter(|p| {
            p.interrupt.is_some() && p.provided_by.is_none() && map.reserved_reason(&p.name).is_none()
        })
        .collect();
    let usable: Vec<&&Pin> = interrupt_pins
        .iter()
        .filter(|p| !taken.buses.contains_key(&BusKind::I2c) || p.bus(BusKind::I2c).is_none())
        .collect();
    if !usable.is_empty() {
        let plain: Vec<&str> = usable
            .iter()
            .filter(|p| {
                taken.pins.get(&p.name).is_some_and(|uses| {
                    uses.iter().any(|u| {
                        u.part.as_ref().is_some_and(|(part, role)| {
                            plan.parts.get(part).and_then(|pp| {
                                let t = catalog.template(&pp.template)?;
                                Some(shape_of(t, pp)?.role(role)?.needs != Need::Interrupt)
                            }) == Some(true)
                        })
                    })
                })
            })
            .map(|p| p.name.as_str())
            .collect();
        if plain.len() == usable.len() {
            let message = if plain.len() == 1 {
                format!(
                    "{} is the {}'s only free interrupt pin, and it is used as a plain pin",
                    plain[0], map.name
                )
            } else {
                format!(
                    "{} are the {}'s only free interrupt pins, and they are used as plain pins",
                    and_list(&plain),
                    map.name
                )
            };
            out.push(Problem::warning(
                "interrupt-pins-used",
                format!("{message}: a wheel encoder added later will need one."),
                "Move a part off them, unless the board will never have an encoder.",
            ));
        }
    }

    // The board's own supply.
    let from_board: u32 = plan
        .parts
        .values()
        .filter_map(|p| catalog.template(&p.template))
        .filter(|t| matches!(t.power.supply, Supply::FiveVolts | Supply::ThreeVolts))
        .filter_map(|t| t.power.current_ma)
        .sum();
    if from_board > 400 {
        out.push(Problem::warning(
            "power-budget",
            format!("The parts take about {from_board} mA from the board's own supply; USB gives about 500 mA for everything."),
            "Power some parts from their own supply.",
        ));
    }
    out
}

/// A voltage as people write it: `3.3`, not `3.299999952316284`.
fn volts(v: f32) -> String {
    number_text((f64::from(v) * 100.0).round() / 100.0)
}

/// The battery divider: the pin must stay below the analog reference with a full battery.
fn divider(map: &PinMap, name: &str, part: &PlannedPart, d: &super::parts::DividerCheck) -> Option<Problem> {
    let reference = f64::from(map.analog_reference_volts?);
    let num = |id: &str| part.settings.get(id).and_then(|v| v.as_f64());
    let (r1, r2, cells) = (num(&d.r1)?, num(&d.r2)?, num(&d.cells)?);
    let chem = part.settings.get(&d.chemistry).and_then(|v| v.as_str())?;
    let full = d.full_cell_volts.get(chem)? * cells;
    if r2 <= 0.0 {
        return None;
    }
    let at_pin = full * r2 / (r1 + r2);
    (at_pin > reference).then(|| {
        Problem::error(
            "divider-too-high",
            format!(
                "{name}: a full battery ({:.1} V) puts {:.2} V on the pin, above the {}'s {} V reference: that can damage the board.",
                full,
                at_pin,
                map.name,
                number_text(reference)
            ),
            format!(
                "Make R1 bigger or R2 smaller: R1 at least {} Ω with this R2.",
                // Rounded first, so 15200.000000002 Ω isn't asked for as 15201.
                number_text((((r2 * (full / reference - 1.0)) * 1e6).round() / 1e6).ceil())
            ),
        )
        .part(name)
        .setting(&d.r1)
    })
}

#[cfg(test)]
mod tests;
