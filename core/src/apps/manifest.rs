//! Reads an app's `faber.toml` (docs/apps.md §3, docs/architecture.md §7).
//!
//! Strict, like the declarative connector reader: every key must be known, and every mistake
//! names its line and says what to write instead (docs/principles.md, "Clear errors from the
//! tools too").

use faber_capabilities::Capability;
use faber_connector::is_kebab_name;
use serde::Deserialize;
use std::fmt;
use std::ops::Range;
use toml::Spanned;

pub const FILE_NAME: &str = "faber.toml";

/// The version of the manifest format this core reads: `faber-manifest = 1`.
pub const FORMAT: i64 = 1;

/// The fastest an app's logic may be ticked.
pub const MAX_TICK_HZ: u32 = 100;
/// How often an app's logic is ticked when the manifest doesn't say.
pub const DEFAULT_TICK_HZ: u32 = 10;

/// The folder in the package that holds the app's UI.
pub const UI_DIR: &str = "ui/";

/// A problem in an app's `faber.toml`, with where it is and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    /// 1-based, when the problem has a place in the file.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub hint: String,
}

impl ManifestError {
    fn at(
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

/// How many parts may fill a role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    /// Exactly one (the default).
    One,
    /// One or more (`count = "1.."`): the role is a list of parts.
    OneOrMore,
    /// None or one (`count = "0..1"`): the app runs without it.
    Optional,
    /// Any number, none too (`count = "0.."`): a list of parts the app runs without.
    Any,
}

impl Count {
    pub fn id(self) -> &'static str {
        match self {
            Count::One => "1",
            Count::OneOrMore => "1..",
            Count::Optional => "0..1",
            Count::Any => "0..",
        }
    }

    /// The role is a list of parts (`1..` or `0..`).
    pub fn many(self) -> bool {
        matches!(self, Count::OneOrMore | Count::Any)
    }

    /// The app can't run until a part fills the role (`1` or `1..`).
    pub fn required(self) -> bool {
        matches!(self, Count::One | Count::OneOrMore)
    }
}

/// One `[[requires]]`: a role the user fills with parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSpec {
    pub role: String,
    pub capability: Capability,
    /// The app may drive it (it needs a lease).
    pub command: bool,
    pub count: Count,
}

/// One `[[suggests]]`: a connector offered when nothing fills a role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub connector: String,
    pub for_role: String,
}

/// One `[[publishes]]`: a derived signal (D24), `app:<id>/<name>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishSpec {
    pub name: String,
    pub capability: Capability,
}

/// `[window]`: how the app's UI opens in the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub title: Option<String>,
    /// A file in the package, e.g. `icon.svg`.
    pub icon: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Pinned to the taskbar when installed.
    pub pinned: bool,
}

/// An app's manifest, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppManifest {
    pub id: String,
    pub version: String,
    pub name: String,
    pub publisher: Option<String>,
    /// The logic's component, next to `faber.toml`. `None`: a UI-only app.
    pub module: Option<String>,
    /// The app has a UI in `ui/`.
    pub ui: bool,
    /// How often the logic is ticked.
    pub tick_hz: u32,
    pub window: Option<Window>,
    pub requires: Vec<RoleSpec>,
    pub suggests: Vec<Suggestion>,
    pub publishes: Vec<PublishSpec>,
}

impl AppManifest {
    pub fn role(&self, name: &str) -> Option<&RoleSpec> {
        self.requires.iter().find(|r| r.role == name)
    }

    pub fn publish(&self, name: &str) -> Option<&PublishSpec> {
        self.publishes.iter().find(|p| p.name == name)
    }
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
    window: Option<Spanned<RawWindow>>,
    #[serde(default)]
    requires: Vec<Spanned<RawRequire>>,
    #[serde(default)]
    suggests: Vec<Spanned<RawSuggest>>,
    #[serde(default)]
    publishes: Vec<Spanned<RawPublish>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawPackage {
    id: Spanned<String>,
    #[serde(rename = "type")]
    package_type: Spanned<String>,
    version: Spanned<String>,
    name: Spanned<String>,
    publisher: Option<Spanned<String>>,
    module: Option<Spanned<String>>,
    ui: Option<Spanned<String>>,
    tick_hz: Option<Spanned<i64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWindow {
    title: Option<Spanned<String>>,
    icon: Option<Spanned<String>>,
    width: Option<Spanned<i64>>,
    height: Option<Spanned<i64>>,
    pinned: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRequire {
    role: Spanned<String>,
    capability: Spanned<String>,
    command: Option<Spanned<bool>>,
    count: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawSuggest {
    connector: Spanned<String>,
    for_role: Spanned<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPublish {
    name: Spanned<String>,
    capability: Spanned<String>,
}

/// Reads and checks an app's `faber.toml`.
pub fn parse(text: &str) -> Result<AppManifest, ManifestError> {
    let syntax = |e: toml::de::Error| {
        ManifestError::at(
            text,
            e.span(),
            e.message().trim().to_string(),
            "Fix this line. docs/apps.md shows every key an app's faber.toml can have.",
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
            "Update Faber, or use a version of the app made for this Faber.",
        )),
        _ => Err(ManifestError::at(
            text,
            Some(format.span()),
            format!("faber-manifest = {} is not a manifest format.", format.get_ref()),
            format!("Write faber-manifest = {FORMAT}."),
        )),
    }
}

fn known_capabilities() -> String {
    Capability::ALL.iter().map(|c| c.id()).collect::<Vec<_>>().join(", ")
}

/// A plain file name or relative path inside the package: no `..`, no `/x`, no `C:`, no `\`.
fn plain_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

struct Builder<'a> {
    text: &'a str,
}

impl Builder<'_> {
    fn err(&self, span: Range<usize>, message: impl Into<String>, hint: impl Into<String>) -> ManifestError {
        ManifestError::at(self.text, Some(span), message, hint)
    }

    fn build(&self, raw: RawFile) -> Result<AppManifest, ManifestError> {
        let package_span = raw.package.span();
        let p = raw.package.into_inner();
        self.check_header(&p)?;
        let module = match &p.module {
            Some(m)
                if plain_path(m.get_ref())
                    && !m.get_ref().contains('/')
                    && m.get_ref().ends_with(".wasm") =>
            {
                Some(m.get_ref().clone())
            }
            Some(m) => {
                return Err(self.err(
                    m.span(),
                    format!("module = \"{}\" is not a component file next to faber.toml.", m.get_ref()),
                    "Write module = \"app.wasm\": the file faber pack builds.",
                ));
            }
            None => None,
        };
        let ui = match &p.ui {
            Some(u) if matches!(u.get_ref().as_str(), "ui/" | "ui") => true,
            Some(u) => {
                return Err(self.err(
                    u.span(),
                    format!("ui = \"{}\" is not where an app's UI lives.", u.get_ref()),
                    "Put the UI's files in the folder ui/ (with ui/index.html) and write ui = \"ui/\".",
                ));
            }
            None => false,
        };
        if module.is_none() && !ui {
            return Err(self.err(
                package_span,
                "This app has neither logic nor a UI.",
                "Add module = \"app.wasm\" for logic that runs in the background, ui = \"ui/\" for a window, or both.",
            ));
        }
        let tick_hz = match &p.tick_hz {
            Some(t) if module.is_none() => {
                return Err(self.err(
                    t.span(),
                    "tick-hz is how often the app's logic runs, but this app has no module.",
                    "Remove tick-hz, or add module = \"app.wasm\".",
                ));
            }
            Some(t) => match u32::try_from(*t.get_ref()) {
                Ok(hz) if (1..=MAX_TICK_HZ).contains(&hz) => hz,
                _ => {
                    return Err(self.err(
                        t.span(),
                        format!("tick-hz = {} is not a rate Faber runs apps at.", t.get_ref()),
                        format!("Use a whole number from 1 to {MAX_TICK_HZ} (times a second)."),
                    ));
                }
            },
            None => DEFAULT_TICK_HZ,
        };
        let window = raw.window.map(|w| self.window(w, ui)).transpose()?;
        let mut requires: Vec<RoleSpec> = Vec::new();
        for r in raw.requires {
            let spec = self.require(r.into_inner())?;
            requires.push(spec);
        }
        for (i, r) in requires.iter().enumerate() {
            if requires[..i].iter().any(|o| o.role == r.role) {
                return Err(ManifestError::at(
                    self.text,
                    None,
                    format!("Two [[requires]] are both called role = \"{}\".", r.role),
                    "Give every role its own name.",
                ));
            }
        }
        let mut suggests = Vec::new();
        for s in raw.suggests {
            let s = s.into_inner();
            if !is_kebab_name(s.connector.get_ref()) {
                return Err(self.err(
                    s.connector.span(),
                    format!("connector = \"{}\" is not a connector id.", s.connector.get_ref()),
                    "Write the connector's id, like connector = \"ldrobot-ld19\".",
                ));
            }
            if !requires.iter().any(|r| &r.role == s.for_role.get_ref()) {
                return Err(self.err(
                    s.for_role.span(),
                    format!("for-role = \"{}\" is not one of this app's roles.", s.for_role.get_ref()),
                    "Use the role = … of one of the [[requires]] above.",
                ));
            }
            suggests
                .push(Suggestion { connector: s.connector.into_inner(), for_role: s.for_role.into_inner() });
        }
        let mut publishes: Vec<PublishSpec> = Vec::new();
        for pb in raw.publishes {
            let pb = pb.into_inner();
            let name = pb.name.get_ref();
            if !is_kebab_name(name) {
                return Err(self.err(
                    pb.name.span(),
                    format!("name = \"{name}\" is not a signal name."),
                    "Use lowercase words joined by -, like name = \"wall-distance\".",
                ));
            }
            if publishes.iter().any(|o| &o.name == name) {
                return Err(self.err(
                    pb.name.span(),
                    format!("Two [[publishes]] are both called \"{name}\"."),
                    "Give every derived signal its own name.",
                ));
            }
            let capability = self.capability(&pb.capability)?;
            if capability.lease().is_some() {
                return Err(self.err(
                    pb.capability.span(),
                    format!("An app can't publish {}: derived signals take no commands.", capability.id()),
                    "Publish what the app computes as a sensor capability, like distance@1 or pose@1.",
                ));
            }
            publishes.push(PublishSpec { name: pb.name.into_inner(), capability });
        }
        Ok(AppManifest {
            id: p.id.into_inner(),
            version: p.version.into_inner(),
            name: p.name.into_inner(),
            publisher: p.publisher.map(Spanned::into_inner),
            module,
            ui,
            tick_hz,
            window,
            requires,
            suggests,
            publishes,
        })
    }

    fn check_header(&self, p: &RawPackage) -> Result<(), ManifestError> {
        if p.package_type.get_ref() != "app" {
            return Err(self.err(
                p.package_type.span(),
                format!("type = \"{}\" is not an app.", p.package_type.get_ref()),
                "Write type = \"app\" in [package].",
            ));
        }
        if !is_kebab_name(p.id.get_ref()) {
            return Err(self.err(
                p.id.span(),
                format!("id = \"{}\" is not a valid app id.", p.id.get_ref()),
                "Use lowercase words joined by -, starting with a letter, like \"wall-follower\".",
            ));
        }
        if !faber_connector::manifest_file::is_version(p.version.get_ref()) {
            return Err(self.err(
                p.version.span(),
                format!("version = \"{}\" is not a version number.", p.version.get_ref()),
                "Use three numbers, like version = \"0.1.0\".",
            ));
        }
        if p.name.get_ref().trim().is_empty() {
            return Err(self.err(
                p.name.span(),
                "The app has no name.",
                "Give it one people recognise, like name = \"Wall follower\".",
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

    fn capability(&self, c: &Spanned<String>) -> Result<Capability, ManifestError> {
        Capability::from_id(c.get_ref()).ok_or_else(|| {
            self.err(
                c.span(),
                format!("Faber doesn't know the capability \"{}\".", c.get_ref()),
                format!("Use one of {}.", known_capabilities()),
            )
        })
    }

    fn require(&self, r: RawRequire) -> Result<RoleSpec, ManifestError> {
        if !is_kebab_name(r.role.get_ref()) {
            return Err(self.err(
                r.role.span(),
                format!("role = \"{}\" is not a role name.", r.role.get_ref()),
                "Use lowercase words joined by -, like role = \"lidar\".",
            ));
        }
        let capability = self.capability(&r.capability)?;
        let command = match r.command {
            Some(c) if *c.get_ref() && capability.lease().is_none() => {
                return Err(self.err(
                    c.span(),
                    format!("{} takes no commands, so command = true can't be met.", capability.id()),
                    "Remove command = true: the app only reads this role.",
                ));
            }
            Some(c) => *c.get_ref(),
            None => false,
        };
        let count = match &r.count {
            None => Count::One,
            Some(c) => match c.get_ref().as_str() {
                "1" => Count::One,
                "1.." => Count::OneOrMore,
                "0..1" => Count::Optional,
                "0.." => Count::Any,
                other => {
                    return Err(self.err(
                        c.span(),
                        format!("count = \"{other}\" is not a number of parts Faber understands."),
                        "Write count = \"1\" (one part, the default), \"1..\" (one or more), \"0..1\" (one, or none) or \"0..\" (any number, or none).",
                    ));
                }
            },
        };
        Ok(RoleSpec { role: r.role.into_inner(), capability, command, count })
    }

    fn window(&self, w: Spanned<RawWindow>, ui: bool) -> Result<Window, ManifestError> {
        let span = w.span();
        let w = w.into_inner();
        if !ui {
            return Err(self.err(
                span,
                "[window] says how the app's UI opens, but this app has no UI.",
                "Remove [window], or add ui = \"ui/\" to [package].",
            ));
        }
        let size = |v: &Option<Spanned<i64>>, what: &str| -> Result<Option<u32>, ManifestError> {
            match v {
                None => Ok(None),
                Some(v) => match u32::try_from(*v.get_ref()) {
                    Ok(n) if (100..=4000).contains(&n) => Ok(Some(n)),
                    _ => Err(self.err(
                        v.span(),
                        format!("{what} = {} is not a window size.", v.get_ref()),
                        "Use a number of pixels from 100 to 4000.",
                    )),
                },
            }
        };
        if let Some(icon) = &w.icon
            && !plain_path(icon.get_ref())
        {
            return Err(self.err(
                icon.span(),
                format!("icon = \"{}\" is not a file in the package.", icon.get_ref()),
                "Write a path inside the package, like icon = \"icon.svg\".",
            ));
        }
        if let Some(title) = &w.title
            && title.get_ref().trim().is_empty()
        {
            return Err(self.err(title.span(), "title is empty.", "Write a title, or leave the line out."));
        }
        Ok(Window {
            width: size(&w.width, "width")?,
            height: size(&w.height, "height")?,
            title: w.title.map(Spanned::into_inner),
            icon: w.icon.map(Spanned::into_inner),
            pinned: w.pinned.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SENNEP: &str = r#"faber-manifest = 1

[package]
id = "sennep"
type = "app"
version = "0.1.0"
name = "SENNEP"
publisher = "Mikkel"
module = "sennep.wasm"
ui = "ui/"
tick-hz = 20

[window]
title = "SENNEP"
icon = "icon.svg"
width = 800
height = 600

[[requires]]
role = "lidar"
capability = "laser-scan@1"

[[requires]]
role = "base"
capability = "planar-base@1"
command = true

[[requires]]
role = "proximity"
capability = "distance@1"
count = "1.."

[[suggests]]
connector = "ldrobot-ld19"
for-role = "lidar"

[[publishes]]
name = "pose"
capability = "pose@1"
"#;

    #[test]
    fn reads_the_architecture_example() {
        let m = parse(SENNEP).unwrap();
        assert_eq!((m.id.as_str(), m.version.as_str(), m.name.as_str()), ("sennep", "0.1.0", "SENNEP"));
        assert_eq!(m.publisher.as_deref(), Some("Mikkel"));
        assert_eq!(m.module.as_deref(), Some("sennep.wasm"));
        assert!(m.ui);
        assert_eq!(m.tick_hz, 20);
        assert_eq!(
            m.window,
            Some(Window {
                title: Some("SENNEP".into()),
                icon: Some("icon.svg".into()),
                width: Some(800),
                height: Some(600),
                pinned: false
            })
        );
        assert_eq!(
            m.requires,
            [
                RoleSpec {
                    role: "lidar".into(),
                    capability: Capability::LaserScan,
                    command: false,
                    count: Count::One
                },
                RoleSpec {
                    role: "base".into(),
                    capability: Capability::PlanarBase,
                    command: true,
                    count: Count::One
                },
                RoleSpec {
                    role: "proximity".into(),
                    capability: Capability::Distance,
                    command: false,
                    count: Count::OneOrMore
                },
            ]
        );
        assert_eq!(m.suggests, [Suggestion { connector: "ldrobot-ld19".into(), for_role: "lidar".into() }]);
        assert_eq!(m.publishes, [PublishSpec { name: "pose".into(), capability: Capability::Pose }]);
    }

    #[test]
    fn a_role_can_be_optional() {
        let text = SENNEP.replace("count = \"1..\"", "count = \"0..\"").replace(
            "capability = \"planar-base@1\"
command = true
",
            "capability = \"planar-base@1\"
command = true

[[requires]]
role = \"battery\"
capability = \"battery@1\"
count = \"0..1\"
",
        );
        let m = parse(&text).unwrap();
        let counts: Vec<(&str, Count)> = m.requires.iter().map(|r| (r.role.as_str(), r.count)).collect();
        assert_eq!(
            counts,
            [
                ("lidar", Count::One),
                ("base", Count::One),
                ("battery", Count::Optional),
                ("proximity", Count::Any)
            ]
        );
        let ids: Vec<&str> =
            [Count::One, Count::OneOrMore, Count::Optional, Count::Any].map(Count::id).to_vec();
        assert_eq!(ids, ["1", "1..", "0..1", "0.."]);
        assert!(
            !Count::Optional.many() && Count::Any.many() && Count::OneOrMore.many() && !Count::One.many()
        );
        assert!(Count::One.required() && Count::OneOrMore.required());
        assert!(!Count::Optional.required() && !Count::Any.required());
    }

    #[test]
    fn a_ui_only_app_needs_no_module_and_ticks_at_the_default() {
        let text = "faber-manifest = 1\n[package]\nid = \"gauges\"\ntype = \"app\"\nversion = \"0.1.0\"\n\
                    name = \"Gauges\"\nui = \"ui/\"\n";
        let m = parse(text).unwrap();
        assert_eq!((m.module, m.ui, m.tick_hz, m.window), (None, true, DEFAULT_TICK_HZ, None));
        assert!(m.requires.is_empty() && m.publishes.is_empty());
    }

    /// Each mistake, the line it is on, and words from its message.
    #[test]
    fn mistakes_name_their_line_and_say_what_to_write() {
        let cases: &[(&str, &str, usize, &str)] = &[
            ("faber-manifest = 1", "", 1, "doesn't say which manifest format"),
            ("faber-manifest = 1", "faber-manifest = 2", 1, "format 2, but this Faber reads format 1"),
            ("type = \"app\"", "type = \"connector\"", 5, "is not an app"),
            ("id = \"sennep\"", "id = \"Sennep\"", 4, "not a valid app id"),
            ("version = \"0.1.0\"", "version = \"1\"", 6, "not a version number"),
            ("name = \"SENNEP\"", "name = \" \"", 7, "has no name"),
            ("publisher = \"Mikkel\"", "publisher = \"\"", 8, "publisher is empty"),
            ("module = \"sennep.wasm\"", "module = \"../x.wasm\"", 9, "not a component file"),
            ("ui = \"ui/\"", "ui = \"web/\"", 10, "not where an app's UI lives"),
            ("tick-hz = 20", "tick-hz = 1000", 11, "not a rate"),
            ("tick-hz = 20", "tick-hz = 20\ncolour = \"red\"", 12, "unknown field `colour`"),
            ("width = 800", "width = 5", 16, "not a window size"),
            ("icon = \"icon.svg\"", "icon = \"../icon.svg\"", 15, "not a file in the package"),
            (
                "capability = \"laser-scan@1\"",
                "capability = \"laser-scan@9\"",
                21,
                "doesn't know the capability",
            ),
            ("role = \"lidar\"", "role = \"Lidar\"", 20, "not a role name"),
            ("count = \"1..\"", "count = \"2\"", 31, "not a number of parts"),
            ("for-role = \"lidar\"", "for-role = \"camera\"", 35, "not one of this app's roles"),
            ("connector = \"ldrobot-ld19\"", "connector = \"LD19\"", 34, "not a connector id"),
            ("capability = \"pose@1\"", "capability = \"motor@1\"", 39, "derived signals take no commands"),
            ("name = \"pose\"", "name = \"Pose\"", 38, "not a signal name"),
        ];
        for (from, to, line, words) in cases {
            assert!(SENNEP.contains(from), "{from}");
            let text = SENNEP.replacen(from, to, 1);
            let e = parse(&text).unwrap_err();
            assert!(e.message.contains(words), "{from} → {to}: {e:?}");
            assert_eq!(e.line, Some(*line), "{from} → {to}: {e:?}");
            assert!(!e.hint.is_empty(), "{e:?}");
        }
    }

    #[test]
    fn rules_that_span_entries_are_checked() {
        let e = parse(&SENNEP.replace("role = \"proximity\"", "role = \"lidar\"")).unwrap_err();
        assert!(e.message.contains("both called role = \"lidar\""), "{e:?}");
        let e = parse(
            &SENNEP.replace("capability = \"laser-scan@1\"", "capability = \"laser-scan@1\"\ncommand = true"),
        )
        .unwrap_err();
        assert!(e.message.contains("takes no commands"), "{e:?}");
        let ui_only = SENNEP.replace("module = \"sennep.wasm\"\n", "");
        let e = parse(&ui_only).unwrap_err();
        assert!(e.message.contains("no module") && e.hint.contains("Remove tick-hz"), "{e:?}");
        let neither = ui_only.replace("ui = \"ui/\"\n", "").replace("tick-hz = 20\n", "");
        let e = parse(&neither).unwrap_err();
        assert!(e.message.contains("neither logic nor a UI"), "{e:?}");
        let no_ui_window = SENNEP.replace("ui = \"ui/\"\n", "");
        let e = parse(&no_ui_window).unwrap_err();
        assert!(e.message.contains("has no UI") && e.line == Some(12), "{e:?}");
    }
}
