//! Layer 2: the machine config, `parts.toml` (D13: the files are the source of truth).
//!
//! Pure text in, text out: reading validates and points errors at the exact line; editing keeps
//! the user's comments and formatting. The file I/O lives in `machine`.

use crate::names::PartName;
use faber_capabilities::Mount;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ops::Range;

pub const FILE_NAME: &str = "parts.toml";

#[derive(Clone, Debug, PartialEq)]
pub struct PartConfig {
    pub connector: String,
    /// Where its values come from: a real port, or a simulation (D36).
    pub source: Source,
    pub mount: Mount,
    pub settings: toml::Table,
}

/// `port = "…"` or `simulate = "…"`: exactly one of them (docs/simulation.md, section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The real part, on this port (e.g. `usb:10c4:ea60`).
    Port(String),
    /// A simulated part: same name and capability, no hardware, no port claimed.
    Simulated(Simulate),
}

impl Source {
    pub fn port(&self) -> Option<&str> {
        match self {
            Source::Port(p) => Some(p),
            Source::Simulated(_) => None,
        }
    }

    pub fn simulate(&self) -> Option<Simulate> {
        match self {
            Source::Port(_) => None,
            Source::Simulated(s) => Some(*s),
        }
    }
}

/// How a simulated part gets its values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Simulate {
    /// A value or pattern set by hand; actuators report the commands they get.
    Manual,
    /// From the simulated world: raycasts from the part's mount, and the base moves the machine.
    World,
}

impl Simulate {
    pub const ALL: &'static [Simulate] = &[Simulate::Manual, Simulate::World];

    pub fn id(self) -> &'static str {
        match self {
            Simulate::Manual => "manual",
            Simulate::World => "world",
        }
    }

    pub fn parse(s: &str) -> Option<Simulate> {
        Simulate::ALL.iter().copied().find(|m| m.id() == s)
    }
}

/// The `[world]` table: which world simulated parts live in, and where the machine starts.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldConfig {
    pub name: String,
    /// Where the machine starts; the world's own start when left out.
    pub start: Option<Start>,
}

/// A pose on the world's floor: `x`, `y` (m) and `yaw` (rad).
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Start {
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartsConfig {
    pub parts: BTreeMap<PartName, PartConfig>,
    /// `None` in files from before Phase 2, or when no world was picked.
    pub world: Option<WorldConfig>,
}

/// A problem in `parts.toml`, with where it is and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// 1-based line and column, when the problem has a place in the file.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub hint: String,
}

impl ConfigError {
    fn at(text: &str, span: Option<Range<usize>>, message: String, hint: &str) -> ConfigError {
        let (line, column) = match span {
            Some(s) => {
                let (l, c) = line_col(text, s.start);
                (Some(l), Some(c))
            }
            None => (None, None),
        };
        ConfigError { line, column, message, hint: hint.into() }
    }
}

const FIX_HINT: &str = "Fix this line in parts.toml, or remove the part in the shell and add it again.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    parts: BTreeMap<String, RawPart>,
    world: Option<WorldConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPart {
    connector: String,
    port: Option<toml::Spanned<String>>,
    simulate: Option<toml::Spanned<String>>,
    #[serde(default)]
    mount: Mount,
    #[serde(default)]
    settings: toml::Table,
}

/// 1-based line and column of a byte offset.
fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// Where the key of `[parts.<name>]` sits in the file.
fn part_key_span(text: &str, name: &str) -> Option<Range<usize>> {
    let doc = toml_edit::Document::parse(text).ok()?;
    let parts = doc.get("parts")?.as_table_like()?;
    let (key, _) = parts.get_key_value(name)?;
    key.span()
}

pub fn parse(text: &str) -> Result<PartsConfig, ConfigError> {
    let raw: RawFile = toml::from_str(text)
        .map_err(|e| ConfigError::at(text, e.span(), e.message().trim().to_string(), FIX_HINT))?;
    let mut parts = BTreeMap::new();
    for (name, p) in raw.parts {
        let part_name = PartName::parse(&name).map_err(|bad| ConfigError {
            hint: bad.hint,
            ..ConfigError::at(text, part_key_span(text, &name), bad.message, "")
        })?;
        let source = source(text, &name, p.port, p.simulate)?;
        parts.insert(
            part_name,
            PartConfig { connector: p.connector, source, mount: p.mount, settings: p.settings },
        );
    }
    Ok(PartsConfig { parts, world: raw.world })
}

/// `port` or `simulate`, exactly one, with the line of whatever is wrong.
fn source(
    text: &str,
    name: &str,
    port: Option<toml::Spanned<String>>,
    simulate: Option<toml::Spanned<String>>,
) -> Result<Source, ConfigError> {
    match (port, simulate) {
        (Some(port), None) => Ok(Source::Port(port.into_inner())),
        (None, Some(sim)) => match Simulate::parse(sim.get_ref()) {
            Some(mode) => Ok(Source::Simulated(mode)),
            None => Err(ConfigError::at(
                text,
                Some(sim.span()),
                format!("simulate = \"{}\" is not a way to simulate a part.", sim.get_ref()),
                "Use simulate = \"manual\" (values you set by hand) or simulate = \"world\" (values from the simulated world).",
            )),
        },
        (Some(_), Some(sim)) => Err(ConfigError::at(
            text,
            Some(sim.span()),
            format!("The part \"{name}\" has both port and simulate, but it can only have one."),
            "Remove port to simulate the part, or remove simulate to use the real part on its port.",
        )),
        (None, None) => Err(ConfigError::at(
            text,
            part_key_span(text, name),
            format!("The part \"{name}\" has neither port nor simulate."),
            "Add port = \"…\" with the port it is plugged into, or simulate = \"manual\" or \"world\" to simulate it.",
        )),
    }
}

/// Checks that `[world] name` is one of the `known` worlds, pointing at the line if it isn't.
/// The worlds live in another module, so the caller says which exist.
pub fn check_world(text: &str, config: &PartsConfig, known: &[&str]) -> Result<(), ConfigError> {
    let Some(world) = &config.world else { return Ok(()) };
    if known.contains(&world.name.as_str()) {
        return Ok(());
    }
    Err(ConfigError::at(
        text,
        world_name_span(text),
        format!("There is no world called \"{}\".", world.name),
        &format!("Use one of the prebuilt worlds: {}.", known.join(", ")),
    ))
}

/// Where the value of `[world] name` sits in the file.
fn world_name_span(text: &str) -> Option<Range<usize>> {
    let doc = toml_edit::Document::parse(text).ok()?;
    doc.get("world")?.as_table_like()?.get("name")?.span()
}

/// Writes `[world]` (its `name`, and `start` if given), keeping everything else in the file,
/// comments included. Without a `start` the world's own start is used, so any old one goes.
pub fn set_world(text: &str, world: &WorldConfig) -> Result<String, ConfigError> {
    let mut doc = edit_doc(text)?;
    let start = world.start.map(|s| {
        let mut t = toml_edit::InlineTable::new();
        for (k, v) in [("x", s.x), ("y", s.y), ("yaw", s.yaw)] {
            t.insert(k, v.into());
        }
        toml_edit::Value::InlineTable(t)
    });
    if let Some(table) = doc.get_mut("world").and_then(|w| w.as_table_like_mut()) {
        set_value(table, "name", world.name.as_str().into());
        match start {
            Some(s) => set_value(table, "start", s),
            None => {
                table.remove("start");
            }
        }
        return Ok(doc.to_string());
    }
    let mut t = toml_edit::Table::new();
    t.insert("name", toml_edit::value(world.name.as_str()));
    if let Some(s) = start {
        t.insert("start", toml_edit::value(s));
    }
    move_trailing_comments_above(&mut doc, &mut t);
    doc.insert("world", toml_edit::Item::Table(t));
    Ok(doc.to_string())
}

/// Replaces the value of `key`, keeping the comments around the old one.
fn set_value(table: &mut dyn toml_edit::TableLike, key: &str, value: toml_edit::Value) {
    match table.get_mut(key).and_then(|i| i.as_value_mut()) {
        Some(old) => {
            let decor = old.decor().clone();
            *old = value;
            *old.decor_mut() = decor;
        }
        None => {
            table.insert(key, toml_edit::Item::Value(value));
        }
    }
}

/// Comments after the last table (or a comment-only file) are the document's "trailing" text,
/// which is printed after every table. Like a person appending a table, put the new one after
/// those comments, not before them.
fn move_trailing_comments_above(doc: &mut toml_edit::DocumentMut, t: &mut toml_edit::Table) {
    let trailing = doc.trailing().as_str().unwrap_or("").to_string();
    if !trailing.trim().is_empty() {
        let gap = if trailing.ends_with("\n\n") { "" } else { "\n" };
        t.decor_mut().set_prefix(format!("{trailing}{gap}"));
        doc.set_trailing("");
    }
}

fn edit_doc(text: &str) -> Result<toml_edit::DocumentMut, ConfigError> {
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|e| ConfigError::at(text, e.span(), e.message().trim().to_string(), FIX_HINT))
}

fn mount_item(m: &Mount) -> Option<toml_edit::Item> {
    let fields = [("x", m.x), ("y", m.y), ("z", m.z), ("roll", m.roll), ("pitch", m.pitch), ("yaw", m.yaw)];
    let mut t = toml_edit::InlineTable::new();
    for (k, v) in fields.into_iter().filter(|(_, v)| *v != 0.0) {
        t.insert(k, f64::from(v).into());
    }
    (!t.is_empty()).then(|| toml_edit::value(t))
}

/// Adds `[parts.<name>]`, keeping everything else in the file as it was.
pub fn add_part(text: &str, name: &PartName, cfg: &PartConfig) -> Result<String, ConfigError> {
    let mut doc = edit_doc(text)?;
    let parts_is_table = doc
        .entry("parts")
        .or_insert_with(|| {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            toml_edit::Item::Table(t)
        })
        .is_table();
    if !parts_is_table {
        return Err(ConfigError {
            line: None,
            column: None,
            message: "`parts` in parts.toml is not a table.".into(),
            hint: FIX_HINT.into(),
        });
    }
    let mut t = toml_edit::Table::new();
    t.insert("connector", toml_edit::value(cfg.connector.as_str()));
    match &cfg.source {
        Source::Port(port) => t.insert("port", toml_edit::value(port.as_str())),
        Source::Simulated(mode) => t.insert("simulate", toml_edit::value(mode.id())),
    };
    if let Some(m) = mount_item(&cfg.mount) {
        t.insert("mount", m);
    }
    if !cfg.settings.is_empty() {
        let settings: toml_edit::DocumentMut =
            toml::to_string(&cfg.settings).expect("a toml table serialises").parse().expect("valid toml");
        t.insert("settings", toml_edit::Item::Table(settings.as_table().clone()));
    }
    move_trailing_comments_above(&mut doc, &mut t);
    let parts = doc.get_mut("parts").and_then(|p| p.as_table_mut()).expect("checked above");
    parts.insert(name.as_str(), toml_edit::Item::Table(t));
    Ok(doc.to_string())
}

/// Removes `[parts.<name>]`, keeping everything else in the file as it was.
///
/// Comment rule: a comment block directly above the part (no blank line in between) describes
/// that part and goes with it. Anything above a blank line (e.g. a file header) is kept.
pub fn remove_part(text: &str, name: &PartName) -> Result<String, ConfigError> {
    let mut doc = edit_doc(text)?;
    let Some(parts) = doc.get_mut("parts").and_then(|p| p.as_table_mut()) else {
        return Ok(text.to_string());
    };
    let Some(toml_edit::Item::Table(removed)) = parts.remove(name.as_str()) else {
        return Ok(text.to_string());
    };
    let prefix = removed.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
    let keep = prefix.rfind("\n\n").map(|i| prefix[..i + 2].to_string()).unwrap_or_default();
    if !keep.is_empty() {
        let after = removed.position().unwrap_or(isize::MAX);
        match next_table_after(doc.as_table_mut(), after) {
            Some(next) => {
                let old = next.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
                next.decor_mut().set_prefix(format!("{keep}{old}"));
            }
            None => {
                let old = doc.trailing().as_str().unwrap_or("").to_string();
                doc.set_trailing(format!("{keep}{old}"));
            }
        }
    }
    Ok(doc.to_string())
}

/// The table that comes first in the file after document position `after`.
fn next_table_after(root: &mut toml_edit::Table, after: isize) -> Option<&mut toml_edit::Table> {
    fn best_position(t: &toml_edit::Table, after: isize) -> Option<isize> {
        let own = t.position().filter(|p| *p > after && !t.is_implicit());
        let children = t.iter().filter_map(|(_, i)| i.as_table()).filter_map(|c| best_position(c, after));
        own.into_iter().chain(children).min()
    }
    fn find(t: &mut toml_edit::Table, pos: isize) -> Option<&mut toml_edit::Table> {
        if t.position() == Some(pos) && !t.is_implicit() {
            return Some(t);
        }
        t.iter_mut().filter_map(|(_, i)| i.as_table_mut()).find_map(|c| find(c, pos))
    }
    let pos = best_position(root, after)?;
    find(root, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> PartName {
        PartName::parse(s).unwrap()
    }

    fn lidar() -> PartConfig {
        PartConfig {
            connector: "ldrobot-ld19".into(),
            source: Source::Port("usb:10c4:ea60".into()),
            mount: Mount::default(),
            settings: toml::Table::new(),
        }
    }

    #[test]
    fn empty_file_has_no_parts() {
        assert_eq!(parse("").unwrap(), PartsConfig::default());
    }

    #[test]
    fn parses_parts_with_mount_and_settings() {
        let text = r#"
[parts.front_lidar]
connector = "ldrobot-ld19"
port = "usb:10c4:ea60"
mount = { x = 0.2, yaw = 1.5 }

[parts.front_lidar.settings]
speed = 10
"#;
        let c = parse(text).unwrap();
        let p = &c.parts[&name("front_lidar")];
        assert_eq!(p.connector, "ldrobot-ld19");
        assert_eq!(p.mount.x, 0.2);
        assert_eq!(p.mount.yaw, 1.5);
        assert_eq!(p.settings["speed"].as_integer(), Some(10));
    }

    #[test]
    fn syntax_errors_point_at_the_line() {
        let text = "[parts.lidar]\nconnector = \"x\"\nport = \n";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, Some(3), "{e:?}");
        assert!(!e.hint.is_empty());
    }

    #[test]
    fn unknown_fields_are_errors_not_ignored() {
        let text = "[parts.lidar]\nconnector = \"x\"\nport = \"y\"\nbaud = 9600\n";
        let e = parse(text).unwrap_err();
        assert!(e.message.contains("baud"), "{e:?}");
        assert_eq!(e.line, Some(4));
    }

    #[test]
    fn missing_fields_are_named() {
        let e = parse("[parts.lidar]\nconnector = \"x\"\n").unwrap_err();
        assert!(e.message.contains("port"), "{e:?}");
    }

    #[test]
    fn bad_part_names_point_at_the_key() {
        let text = "# my robot\n\n[parts.FrontLidar]\nconnector = \"x\"\nport = \"y\"\n";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, Some(3), "{e:?}");
        assert!(e.hint.contains("lowercase"));
    }

    #[test]
    fn adding_keeps_comments_and_round_trips() {
        let text =
            "# My robot\n\n[parts.gps] # the GNSS module\nconnector = \"nmea-gps\"\nport = \"serial:COM4\"\n";
        let mut cfg = lidar();
        cfg.mount = Mount { x: 0.25, yaw: 1.5, ..Mount::default() };
        let out = add_part(text, &name("lidar"), &cfg).unwrap();
        assert!(out.contains("# My robot"));
        assert!(out.contains("# the GNSS module"));
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.parts.len(), 2);
        assert_eq!(parsed.parts[&name("lidar")], cfg);
    }

    #[test]
    fn adding_to_a_comment_only_file_keeps_the_header_on_top() {
        let text = "# The parts on this machine.\n# Edit freely.\n";
        let out = add_part(text, &name("lidar"), &lidar()).unwrap();
        assert!(out.starts_with(text), "{out}");
        assert_eq!(parse(&out).unwrap().parts.len(), 1);
    }

    #[test]
    fn adding_after_trailing_comments_appends_below_them() {
        let text = "[parts.a]\nconnector = \"x\"\nport = \"p\"\n\n# spare parts go below\n";
        let out = add_part(text, &name("lidar"), &lidar()).unwrap();
        let comment = out.find("# spare parts go below").unwrap();
        let new_part = out.find("[parts.lidar]").unwrap();
        assert!(comment < new_part, "{out}");
        assert_eq!(parse(&out).unwrap().parts.len(), 2);
    }

    #[test]
    fn adding_to_an_empty_file() {
        let out = add_part("", &name("lidar"), &lidar()).unwrap();
        assert_eq!(parse(&out).unwrap().parts[&name("lidar")], lidar());
        assert!(!out.contains("mount"), "a zero mount is not written:\n{out}");
    }

    #[test]
    fn settings_are_written_as_a_table() {
        let mut cfg = lidar();
        cfg.settings.insert("rate-hz".into(), 5.into());
        let out = add_part("", &name("lidar"), &cfg).unwrap();
        assert_eq!(parse(&out).unwrap().parts[&name("lidar")], cfg);
    }

    #[test]
    fn removing_keeps_the_rest() {
        let text = "# My robot\n\n# the lidar\n[parts.a]\nconnector = \"x\"\nport = \"p\"\n\n[parts.b] # keep me\nconnector = \"y\"\nport = \"q\"\n";
        let out = remove_part(text, &name("a")).unwrap();
        assert!(out.starts_with("# My robot\n"), "the file header stays:\n{out}");
        assert!(out.contains("# keep me"));
        assert!(!out.contains("# the lidar"), "the comment directly above the part goes with it:\n{out}");
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.parts.keys().map(|k| k.as_str()).collect::<Vec<_>>(), ["b"]);
    }

    #[test]
    fn removing_the_last_part_keeps_the_header() {
        let text = "# My robot\n\n[parts.a]\nconnector = \"x\"\nport = \"p\"\n";
        let out = remove_part(text, &name("a")).unwrap();
        assert!(out.contains("# My robot"), "{out}");
        assert_eq!(parse(&out).unwrap(), PartsConfig::default());
    }

    #[test]
    fn removing_a_part_with_settings_removes_them_too() {
        let text = "[parts.a]\nconnector = \"x\"\nport = \"p\"\n\n[parts.a.settings]\nk = 1\n\n[parts.b]\nconnector = \"y\"\nport = \"q\"\n";
        let out = remove_part(text, &name("a")).unwrap();
        assert!(!out.contains("k = 1"), "{out}");
        assert_eq!(parse(&out).unwrap().parts.len(), 1);
    }

    #[test]
    fn simulated_parts_and_the_world_from_the_design_parse() {
        let text = r#"
[world]
name = "apartment"
start = { x = 1.0, y = 1.0, yaw = 0.0 }

[parts.base]
connector = "generic-planar-base"
simulate = "world"
settings = { length = 0.8, width = 0.65, max-vx = 0.6, max-vy = 0.6, max-omega = 1.5 }

[parts.lidar]
connector = "ldrobot-ld19"
simulate = "world"
mount = { x = 0.2, z = 0.3 }

[parts.front]
connector = "generic-distance"
simulate = "manual"
"#;
        let c = parse(text).unwrap();
        assert_eq!(c.parts[&name("base")].source, Source::Simulated(Simulate::World));
        assert_eq!(c.parts[&name("front")].source, Source::Simulated(Simulate::Manual));
        assert_eq!(c.parts[&name("base")].settings["max-omega"].as_float(), Some(1.5));
        assert_eq!(
            c.world,
            Some(WorldConfig { name: "apartment".into(), start: Some(Start { x: 1.0, y: 1.0, yaw: 0.0 }) })
        );
    }

    #[test]
    fn the_world_start_is_optional_and_old_files_have_no_world() {
        let c = parse(
            "[world]
name = \"empty\"
",
        )
        .unwrap();
        assert_eq!(c.world.unwrap().start, None, "the world's own start");
        assert_eq!(
            parse(
                "[parts.a]
connector = \"x\"
port = \"p\"
"
            )
            .unwrap()
            .world,
            None
        );
    }

    #[test]
    fn port_and_simulate_together_point_at_the_line() {
        let text = "[parts.a]
connector = \"x\"
port = \"p\"
simulate = \"manual\"
";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, Some(4), "{e:?}");
        assert!(e.message.contains("both"), "{e:?}");
        assert!(e.hint.contains("Remove port"), "{e:?}");
    }

    #[test]
    fn neither_port_nor_simulate_points_at_the_part() {
        let text = "# robot

[parts.a]
connector = \"x\"
";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, Some(3), "{e:?}");
        assert!(e.message.contains("port") && e.message.contains("simulate"), "{e:?}");
    }

    #[test]
    fn an_unknown_simulation_mode_is_named_with_its_line() {
        let text = "[parts.a]
connector = \"x\"
simulate = \"wrld\"
";
        let e = parse(text).unwrap_err();
        assert_eq!(e.line, Some(3), "{e:?}");
        assert!(e.message.contains("\"wrld\""), "{e:?}");
        assert!(e.hint.contains("\"manual\"") && e.hint.contains("\"world\""), "{e:?}");
    }

    #[test]
    fn adding_a_simulated_part_writes_simulate_instead_of_port() {
        let text = "# My robot

[parts.lidar] # real
connector = \"ldrobot-ld19\"
port = \"usb:10c4:ea60\"
";
        let mut settings = toml::Table::new();
        settings.insert("max-vx".into(), 0.6.into());
        let cfg = PartConfig {
            connector: "generic-planar-base".into(),
            source: Source::Simulated(Simulate::Manual),
            mount: Mount::default(),
            settings,
        };
        let out = add_part(text, &name("base"), &cfg).unwrap();
        assert!(out.contains("# My robot") && out.contains("# real"), "{out}");
        assert!(out.contains("simulate = \"manual\""), "{out}");
        assert!(!out[out.find("[parts.base]").unwrap()..].contains("port"), "{out}");
        assert_eq!(parse(&out).unwrap().parts[&name("base")], cfg);
    }

    const WORLDS: &[&str] = &["empty", "apartment"];

    #[test]
    fn an_unknown_world_points_at_its_name_and_lists_the_worlds() {
        let text = "# robot\n\n[world]\nname = \"apartmnet\"\n";
        let e = check_world(text, &parse(text).unwrap(), WORLDS).unwrap_err();
        assert_eq!((e.line, e.column), (Some(4), Some(8)), "{e:?}");
        assert!(e.message.contains("\"apartmnet\""), "{e:?}");
        assert!(e.hint.contains("empty, apartment"), "{e:?}");
        let ok = "[world]\nname = \"apartment\"\n";
        assert_eq!(check_world(ok, &parse(ok).unwrap(), WORLDS), Ok(()));
        assert_eq!(check_world("", &parse("").unwrap(), WORLDS), Ok(()), "no [world] is fine");
    }

    #[test]
    fn setting_the_world_adds_it_after_the_comments() {
        let text = "# My robot\n\n[parts.a] # first\nconnector = \"x\"\nsimulate = \"world\"\n\n# spare\n";
        let w = WorldConfig { name: "apartment".into(), start: Some(Start { x: 1.0, y: 2.0, yaw: 0.5 }) };
        let out = set_world(text, &w).unwrap();
        assert!(out.starts_with("# My robot\n") && out.contains("# first"), "{out}");
        assert!(out.find("# spare").unwrap() < out.find("[world]").unwrap(), "{out}");
        assert_eq!(parse(&out).unwrap().world, Some(w));
        assert_eq!(parse(&out).unwrap().parts.len(), 1);
    }

    #[test]
    fn setting_the_world_again_keeps_its_comments_and_drops_an_old_start() {
        let text = "# My robot\n\n# where it drives\n[world]\nname = \"empty\" # for now\nstart = { x = 1.0, y = 1.0, yaw = 0.0 }\n\n[parts.a]\nconnector = \"x\"\nsimulate = \"world\"\n";
        let w = WorldConfig { name: "apartment".into(), start: None };
        let out = set_world(text, &w).unwrap();
        assert!(out.contains("# where it drives") && out.contains("# for now"), "{out}");
        assert!(!out.contains("start"), "{out}");
        assert_eq!(parse(&out).unwrap().world, Some(w));
        let inline = "world = { name = \"empty\" }\n";
        let out = set_world(inline, &WorldConfig { name: "maze".into(), start: None }).unwrap();
        assert_eq!(parse(&out).unwrap().world.unwrap().name, "maze");
    }

    #[test]
    fn removing_an_unknown_part_changes_nothing() {
        let text = "# hi\n[parts.b]\nconnector = \"y\"\nport = \"q\"\n";
        assert_eq!(remove_part(text, &name("zz")).unwrap(), text);
    }
}
