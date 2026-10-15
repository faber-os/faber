//! What a connector's settings are, declared once in its manifest.
//!
//! A part's settings live in `[parts.<name>.settings]` in parts.toml. The connector declares each
//! one here: its key, a label and a short description for people, its kind, unit, default and
//! limits. The shell builds its Add part form from these declarations, and the connector reads its
//! settings with a [`Reader`] over the same declarations, so a default or a limit is written down
//! exactly once and every mistake is explained the same way (docs/principles.md, "Errors say what
//! to do").
//!
//! Defaults are what a part gets when parts.toml leaves a setting out. Changing one changes every
//! existing part that doesn't set it, so they stay put; a form may start somewhere else instead
//! ([`Number::suggested`]).

use crate::{Settings, SettingsError, is_kebab_name};

/// One setting a connector takes.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingSpec {
    /// The key in `[parts.<name>.settings]`, kebab-case, e.g. `max-vx`.
    pub key: String,
    /// A short name for people, e.g. "Top speed forward". English for now.
    pub label: String,
    /// One sentence on what it does, for the form's help text. English for now.
    pub description: String,
    /// The unit of a number, SI as in parts.toml (`m`, `m/s`, `rad/s`, `Hz`). Data, not translated.
    pub unit: Option<String>,
    pub kind: SettingKind,
    /// Rarely needed: forms show it behind "Advanced" (docs/principles.md, progressive disclosure).
    pub advanced: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingKind {
    Number(Number),
    Bool {
        default: bool,
    },
    /// One of a few values.
    Choice {
        default: String,
        choices: Vec<Choice>,
    },
    String {
        default: String,
    },
    /// A list of other parts' names (e.g. the motors a base is built from). Empty by default.
    PartList,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Number {
    pub default: f64,
    /// The smallest value allowed; with `min_exclusive`, values must be above it.
    pub min: Option<f64>,
    pub min_exclusive: bool,
    pub max: Option<f64>,
    /// Where a new part's form starts, when that differs from `default`.
    pub suggested: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// What parts.toml holds, e.g. `laser-tof`.
    pub value: String,
    /// What people read, e.g. "Laser time-of-flight". English for now.
    pub label: String,
}

impl Choice {
    pub fn new(value: &str, label: &str) -> Choice {
        Choice { value: value.into(), label: label.into() }
    }
}

impl SettingSpec {
    fn new(key: &str, label: &str, description: &str, kind: SettingKind) -> SettingSpec {
        SettingSpec {
            key: key.into(),
            label: label.into(),
            description: description.into(),
            unit: None,
            kind,
            advanced: false,
        }
    }

    /// A number in `unit`, any value by default (see [`positive`](Self::positive) and friends).
    pub fn number(key: &str, label: &str, description: &str, unit: &str, default: f64) -> SettingSpec {
        let n = Number { default, min: None, min_exclusive: false, max: None, suggested: None };
        SettingSpec { unit: Some(unit.into()), ..Self::new(key, label, description, SettingKind::Number(n)) }
    }

    pub fn bool(key: &str, label: &str, description: &str, default: bool) -> SettingSpec {
        Self::new(key, label, description, SettingKind::Bool { default })
    }

    pub fn choice(
        key: &str,
        label: &str,
        description: &str,
        default: &str,
        choices: Vec<Choice>,
    ) -> SettingSpec {
        Self::new(key, label, description, SettingKind::Choice { default: default.into(), choices })
    }

    pub fn string(key: &str, label: &str, description: &str, default: &str) -> SettingSpec {
        Self::new(key, label, description, SettingKind::String { default: default.into() })
    }

    pub fn part_list(key: &str, label: &str, description: &str) -> SettingSpec {
        Self::new(key, label, description, SettingKind::PartList)
    }

    /// Only values above 0.
    pub fn positive(self) -> SettingSpec {
        self.with_number(|n| {
            n.min = Some(0.0);
            n.min_exclusive = true;
        })
    }

    /// Only values of at least `min`.
    pub fn at_least(self, min: f64) -> SettingSpec {
        self.with_number(|n| {
            n.min = Some(min);
            n.min_exclusive = false;
        })
    }

    /// Only values up to `max`.
    pub fn at_most(self, max: f64) -> SettingSpec {
        self.with_number(|n| n.max = Some(max))
    }

    /// A new part's form starts at `value` instead of the default.
    pub fn suggested(self, value: f64) -> SettingSpec {
        self.with_number(|n| n.suggested = Some(value))
    }

    pub fn advanced(self) -> SettingSpec {
        SettingSpec { advanced: true, ..self }
    }

    fn with_number(mut self, f: impl FnOnce(&mut Number)) -> SettingSpec {
        match &mut self.kind {
            SettingKind::Number(n) => f(n),
            _ => panic!("{} is not a number setting", self.key),
        }
        self
    }

    /// Why this declaration can't be used, if it can't.
    pub fn problem(&self) -> Option<String> {
        if !is_kebab_name(&self.key) {
            return Some(format!("setting key \"{}\" is not kebab-case", self.key));
        }
        if self.label.is_empty() || self.description.is_empty() {
            return Some(format!("setting \"{}\" needs a label and a description", self.key));
        }
        match &self.kind {
            SettingKind::Number(n) => {
                let fine = |v: f64| n.allows(v).is_none();
                if !fine(n.default) {
                    return Some(format!("the default of \"{}\" is outside its limits", self.key));
                }
                if n.suggested.is_some_and(|s| !fine(s)) {
                    return Some(format!("the suggested value of \"{}\" is outside its limits", self.key));
                }
            }
            SettingKind::Choice { default, choices } => {
                if !choices.iter().any(|c| &c.value == default) {
                    return Some(format!("the default of \"{}\" is not one of its choices", self.key));
                }
            }
            SettingKind::Bool { .. } | SettingKind::String { .. } | SettingKind::PartList => {}
        }
        None
    }
}

/// Which limit a number breaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutOfRange {
    NotAbove(f64),
    Below(f64),
    Above(f64),
}

impl Number {
    /// `None` if `v` is allowed, else the limit it breaks.
    pub fn allows(&self, v: f64) -> Option<OutOfRange> {
        match self.min {
            Some(min) if self.min_exclusive && v <= min => return Some(OutOfRange::NotAbove(min)),
            Some(min) if !self.min_exclusive && v < min => return Some(OutOfRange::Below(min)),
            _ => {}
        }
        match self.max {
            Some(max) if v > max => Some(OutOfRange::Above(max)),
            _ => None,
        }
    }
}

/// Reads one part's settings against its connector's declarations: defaults for what is left
/// out, and an error that names the setting and says what to use for anything wrong.
pub struct Reader<'a> {
    settings: &'a Settings,
    specs: &'a [SettingSpec],
}

impl<'a> Reader<'a> {
    /// Refuses keys the connector doesn't declare. `what` names the connector in messages
    /// ("the generic drive base").
    pub fn new(
        settings: &'a Settings,
        specs: &'a [SettingSpec],
        what: &str,
    ) -> Result<Reader<'a>, SettingsError> {
        if let Some(key) = settings.keys().find(|k| !specs.iter().any(|s| &s.key == *k)) {
            let mut all: Vec<&str> = specs.iter().map(|s| s.key.as_str()).collect();
            all.sort_unstable();
            let hint = if all.is_empty() {
                format!("{} needs no settings. Remove it from this part's settings.", capitalised(what))
            } else {
                format!("Its settings are {}. Check the spelling.", all.join(", "))
            };
            return Err(SettingsError {
                key: key.clone(),
                message: format!("{} has no setting called \"{key}\".", capitalised(what)),
                hint,
            });
        }
        Ok(Reader { settings, specs })
    }

    fn spec(&self, key: &str) -> &'a SettingSpec {
        self.specs.iter().find(|s| s.key == key).unwrap_or_else(|| panic!("\"{key}\" is not declared"))
    }

    /// A number setting, checked against its declared limits.
    pub fn number(&self, key: &str) -> Result<f64, SettingsError> {
        let spec = self.spec(key);
        let SettingKind::Number(n) = &spec.kind else { panic!("\"{key}\" is not a number setting") };
        let example = || format!("For example {key} = {}.", n.suggested.unwrap_or(n.default));
        let Some(v) = self.settings.get(key) else { return Ok(n.default) };
        let value = match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
            Some(x) if x.is_finite() => x,
            _ => {
                return Err(SettingsError {
                    key: key.into(),
                    message: format!("{key} must be a number, not {v}."),
                    hint: example(),
                });
            }
        };
        let message = match n.allows(value) {
            None => return Ok(value),
            Some(OutOfRange::NotAbove(min)) => format!("{key} must be more than {min}, not {value}."),
            Some(OutOfRange::Below(min)) => format!("{key} must be at least {min}, not {value}."),
            Some(OutOfRange::Above(max)) => format!("{key} must be at most {max}, not {value}."),
        };
        Err(SettingsError { key: key.into(), message, hint: example() })
    }

    pub fn bool(&self, key: &str) -> Result<bool, SettingsError> {
        let SettingKind::Bool { default } = self.spec(key).kind else {
            panic!("\"{key}\" is not a bool setting")
        };
        match self.settings.get(key) {
            None => Ok(default),
            Some(v) => v.as_bool().ok_or_else(|| SettingsError {
                key: key.into(),
                message: format!("{key} must be true or false, not {v}."),
                hint: format!("For example {key} = {default}."),
            }),
        }
    }

    /// One of the declared choices' values.
    pub fn choice(&self, key: &str) -> Result<&'a str, SettingsError> {
        let SettingKind::Choice { default, choices } = &self.spec(key).kind else {
            panic!("\"{key}\" is not a choice setting")
        };
        let Some(v) = self.settings.get(key) else { return Ok(default) };
        v.as_str().and_then(|s| choices.iter().find(|c| c.value == s)).map(|c| c.value.as_str()).ok_or_else(
            || SettingsError {
                key: key.into(),
                message: format!("{key} = {v} is not one of the choices."),
                hint: format!(
                    "Use one of {}.",
                    choices.iter().map(|c| format!("\"{}\"", c.value)).collect::<Vec<_>>().join(", ")
                ),
            },
        )
    }

    pub fn string(&self, key: &str) -> Result<String, SettingsError> {
        let SettingKind::String { default } = &self.spec(key).kind else {
            panic!("\"{key}\" is not a string setting")
        };
        match self.settings.get(key) {
            None => Ok(default.clone()),
            Some(v) => v.as_str().map(str::to_string).ok_or_else(|| SettingsError {
                key: key.into(),
                message: format!("{key} must be text in quotes, not {v}."),
                hint: format!("For example {key} = \"{default}\"."),
            }),
        }
    }

    /// A list of part names, e.g. `built-from = ["fl", "fr"]`.
    pub fn part_list(&self, key: &str) -> Result<Vec<String>, SettingsError> {
        let SettingKind::PartList = self.spec(key).kind else { panic!("\"{key}\" is not a part list") };
        let Some(v) = self.settings.get(key) else { return Ok(Vec::new()) };
        let bad = || SettingsError {
            key: key.into(),
            message: format!("{key} must be a list of part names, not {v}."),
            hint: format!("For example {key} = [\"front-left\", \"front-right\"]."),
        };
        v.as_array().ok_or_else(bad)?.iter().map(|n| n.as_str().map(str::to_string).ok_or_else(bad)).collect()
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> Vec<SettingSpec> {
        vec![
            SettingSpec::number("max-m", "Farthest", "How far it sees.", "m", 4.0).positive().suggested(2.0),
            SettingSpec::number("min-m", "Closest", "How near it sees.", "m", 0.02)
                .at_least(0.0)
                .at_most(1.0),
            SettingSpec::bool("holonomic", "Sideways", "Can it slide.", true),
            SettingSpec::choice(
                "kind",
                "Kind",
                "What it is.",
                "radar",
                vec![Choice::new("radar", "Radar"), Choice::new("laser-tof", "Laser")],
            ),
            SettingSpec::string("label", "Label", "A name.", "front"),
            SettingSpec::part_list("built-from", "Motors", "What it is built from.").advanced(),
        ]
    }

    fn settings(text: &str) -> Settings {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn left_out_settings_get_their_defaults() {
        let (s, specs) = (Settings::new(), specs());
        let r = Reader::new(&s, &specs, "the sensor").unwrap();
        assert_eq!(r.number("max-m").unwrap(), 4.0, "the default, not the suggestion");
        assert_eq!(r.number("min-m").unwrap(), 0.02);
        assert!(r.bool("holonomic").unwrap());
        assert_eq!(r.choice("kind").unwrap(), "radar");
        assert_eq!(r.string("label").unwrap(), "front");
        assert_eq!(r.part_list("built-from").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn given_settings_are_read() {
        let (s, specs) = (
            settings("max-m = 2\nmin-m = 0\nholonomic = false\nkind = \"laser-tof\"\nbuilt-from = [\"a\"]\n"),
            specs(),
        );
        let r = Reader::new(&s, &specs, "the sensor").unwrap();
        assert_eq!(r.number("max-m").unwrap(), 2.0, "an integer is a number too");
        assert_eq!(r.number("min-m").unwrap(), 0.0, "at least 0 includes 0");
        assert!(!r.bool("holonomic").unwrap());
        assert_eq!(r.choice("kind").unwrap(), "laser-tof");
        assert_eq!(r.part_list("built-from").unwrap(), ["a"]);
    }

    #[test]
    fn mistakes_name_the_setting_and_say_what_to_use() {
        let specs = specs();
        let e = Reader::new(&settings("max-speed = 1"), &specs, "the sensor").err().unwrap();
        assert_eq!(e.key, "max-speed");
        assert_eq!(e.message, "The sensor has no setting called \"max-speed\".");
        assert!(e.hint.contains("max-m") && e.hint.contains("built-from"), "{e:?}");

        let cases = [
            ("max-m = 0", "max-m", "more than 0"),
            ("max-m = \"far\"", "max-m", "must be a number"),
            ("min-m = -1", "min-m", "at least 0"),
            ("min-m = 2", "min-m", "at most 1"),
            ("holonomic = \"yes\"", "holonomic", "true or false"),
            ("kind = \"sonar\"", "kind", "not one of the choices"),
            ("label = 3", "label", "text in quotes"),
            ("built-from = \"a\"", "built-from", "list of part names"),
        ];
        for (text, key, words) in cases {
            let s = settings(text);
            let r = Reader::new(&s, &specs, "the sensor").unwrap();
            let e = match key {
                "max-m" | "min-m" => r.number(key).err(),
                "holonomic" => r.bool(key).err(),
                "kind" => r.choice(key).err(),
                "label" => r.string(key).err(),
                _ => r.part_list(key).err(),
            }
            .unwrap_or_else(|| panic!("{text} is refused"));
            assert_eq!(e.key, key);
            assert!(e.message.contains(words), "{text}: {e:?}");
            assert!(!e.hint.is_empty());
        }
        let r_hint = Reader::new(&settings("max-m = 0"), &specs, "x").unwrap().number("max-m").unwrap_err();
        assert_eq!(r_hint.hint, "For example max-m = 2.", "the example is the suggested value");
    }

    #[test]
    fn a_connector_without_settings_says_it_needs_none() {
        let e = Reader::new(&settings("speed = 10"), &[], "the LD19").err().unwrap();
        assert_eq!(e.hint, "The LD19 needs no settings. Remove it from this part's settings.");
    }

    #[test]
    fn declarations_are_checked() {
        for spec in specs() {
            assert_eq!(spec.problem(), None, "{}", spec.key);
        }
        let bad = [
            SettingSpec::number("Max", "M", "d", "m", 1.0),
            SettingSpec::number("max", "", "d", "m", 1.0),
            SettingSpec::number("max", "M", "d", "m", 0.0).positive(),
            SettingSpec::number("max", "M", "d", "m", 1.0).at_most(2.0).suggested(3.0),
            SettingSpec::choice("kind", "K", "d", "x", vec![Choice::new("y", "Y")]),
        ];
        for spec in bad {
            assert!(spec.problem().is_some(), "{spec:?}");
        }
    }
}
