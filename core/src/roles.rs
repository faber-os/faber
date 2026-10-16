//! Layer 2: `config/apps.toml`, which apps are on and which parts fill their roles (docs/apps.md
//! §5, D04, D13: the files are the source of truth).
//!
//! ```toml
//! [apps.wall-follower]
//! enabled = true
//!
//! [apps.wall-follower.roles]
//! lidar = "lidar"
//! base = "base"
//! proximity = ["front", "rear"]   # a role with count = "1.." (or "0..")
//! ```
//!
//! Pure text in, text out, like `config`: reading validates and points errors at the exact line;
//! editing keeps the user's comments and formatting. The file I/O lives in `machine`.

use crate::names::PartName;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ops::Range;

pub const FILE_NAME: &str = "apps.toml";

/// What a new `apps.toml` starts with.
pub const NEW_FILE_HEADER: &str = "# The apps on this Faber machine: which are on, and which parts fill their roles.\n\
# Turn apps on and pick their parts in the shell, or edit this file by hand: Faber keeps your comments.\n";

/// One app's entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppEntry {
    pub enabled: bool,
    /// Role → the parts that fill it, in order (one, or more for a `count = "1.."` or `"0.."` role).
    pub roles: BTreeMap<String, Vec<PartName>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppsConfig {
    pub apps: BTreeMap<String, AppEntry>,
}

impl AppsConfig {
    pub fn entry(&self, id: &str) -> AppEntry {
        self.apps.get(id).cloned().unwrap_or_default()
    }
}

/// A problem in `apps.toml`, with where it is and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolesError {
    /// 1-based, when the problem has a place in the file.
    pub line: Option<usize>,
    pub message: String,
    pub hint: String,
}

const FIX_HINT: &str =
    "Fix this line in config/apps.toml, or delete the file: every app is then off, with no parts picked.";

fn line_of(text: &str, span: Option<Range<usize>>) -> Option<usize> {
    span.map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1)
}

fn at(
    text: &str,
    span: Option<Range<usize>>,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> RolesError {
    RolesError { line: line_of(text, span), message: message.into(), hint: hint.into() }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    apps: BTreeMap<String, toml::Spanned<RawApp>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    roles: BTreeMap<String, toml::Spanned<RawParts>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawParts {
    One(String),
    Many(Vec<String>),
}

pub fn parse(text: &str) -> Result<AppsConfig, RolesError> {
    let raw: RawFile =
        toml::from_str(text).map_err(|e| at(text, e.span(), e.message().trim().to_string(), FIX_HINT))?;
    let mut apps = BTreeMap::new();
    for (id, app) in raw.apps {
        let span = app.span();
        if !faber_connector::is_kebab_name(&id) {
            return Err(at(
                text,
                Some(span),
                format!("\"{id}\" is not an app id."),
                "App ids are lowercase words joined by -, like wall-follower.",
            ));
        }
        let RawApp { enabled, roles: raw_roles } = app.into_inner();
        let mut roles = BTreeMap::new();
        for (role, parts) in raw_roles {
            let span = parts.span();
            let names = match parts.into_inner() {
                RawParts::One(p) => vec![p],
                RawParts::Many(ps) => ps,
            };
            let parts = names
                .iter()
                .map(|n| {
                    PartName::parse_any(n).map_err(|bad| {
                        at(
                            text,
                            Some(span.clone()),
                            format!("Role \"{role}\" of {id}: {}", bad.message),
                            bad.hint,
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            roles.insert(role, parts);
        }
        apps.insert(id, AppEntry { enabled, roles });
    }
    Ok(AppsConfig { apps })
}

fn edit_doc(text: &str) -> Result<toml_edit::DocumentMut, RolesError> {
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|e| at(text, e.span(), e.message().trim().to_string(), FIX_HINT))
}

/// `[apps.<id>]`, made if missing (with `apps` an implicit table, so the file reads naturally).
fn app_table<'d>(
    doc: &'d mut toml_edit::DocumentMut,
    id: &str,
) -> Result<&'d mut toml_edit::Table, RolesError> {
    // Comments after the last table (or a comment-only file) are the document's trailing text,
    // printed after every table: like a person appending a table, put a new one after them.
    let is_new = doc.get("apps").and_then(|a| a.get(id)).is_none();
    let mut new = toml_edit::Table::new();
    let trailing = doc.trailing().as_str().unwrap_or("").to_string();
    if is_new && !trailing.trim().is_empty() {
        let gap = if trailing.ends_with("\n\n") { "" } else { "\n" };
        new.decor_mut().set_prefix(format!("{trailing}{gap}"));
        doc.set_trailing("");
    }
    let apps = doc.entry("apps").or_insert_with(|| {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        toml_edit::Item::Table(t)
    });
    let not_table = || RolesError {
        line: None,
        message: "`apps` in apps.toml is not a table.".into(),
        hint: FIX_HINT.into(),
    };
    let apps = apps.as_table_mut().ok_or_else(not_table)?;
    apps.entry(id).or_insert_with(|| toml_edit::Item::Table(new)).as_table_mut().ok_or_else(|| RolesError {
        line: None,
        message: format!("[apps.{id}] in apps.toml is not a table."),
        hint: FIX_HINT.into(),
    })
}

/// Replaces the value of `key`, keeping the comments around the old one.
fn set_value(table: &mut toml_edit::Table, key: &str, value: toml_edit::Value) {
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

/// Turns an app on or off.
pub fn set_enabled(text: &str, id: &str, enabled: bool) -> Result<String, RolesError> {
    let mut doc = edit_doc(text)?;
    set_value(app_table(&mut doc, id)?, "enabled", enabled.into());
    Ok(doc.to_string())
}

/// Sets which parts fill an app's roles: exactly these roles, each a part name, or a list for
/// the roles in `lists` (`count = "1.."` or `"0.."`). Roles left out are removed.
pub fn set_roles(
    text: &str,
    id: &str,
    roles: &BTreeMap<String, Vec<PartName>>,
    lists: &[&str],
) -> Result<String, RolesError> {
    let mut doc = edit_doc(text)?;
    let app = app_table(&mut doc, id)?;
    if !app.contains_key("enabled") {
        app.insert("enabled", toml_edit::value(false));
    }
    let table = app
        .entry("roles")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| RolesError {
            line: None,
            message: format!("[apps.{id}.roles] in apps.toml is not a table."),
            hint: FIX_HINT.into(),
        })?;
    let gone: Vec<String> =
        table.iter().map(|(k, _)| k.to_string()).filter(|k| !roles.contains_key(k)).collect();
    for k in gone {
        table.remove(&k);
    }
    for (role, parts) in roles {
        let value: toml_edit::Value = match parts.as_slice() {
            [one] if !lists.contains(&role.as_str()) => one.as_str().into(),
            many => many.iter().map(PartName::as_str).collect::<toml_edit::Array>().into(),
        };
        set_value(table, role, value);
    }
    Ok(doc.to_string())
}

/// Removes `[apps.<id>]` and its roles (the app was uninstalled). Comments directly above it go
/// with it; anything above a blank line (a file header, say) stays.
pub fn remove_app(text: &str, id: &str) -> Result<String, RolesError> {
    let mut doc = edit_doc(text)?;
    let Some(apps) = doc.get_mut("apps").and_then(|a| a.as_table_mut()) else { return Ok(text.to_string()) };
    let Some(toml_edit::Item::Table(removed)) = apps.remove(id) else { return Ok(text.to_string()) };
    let prefix = removed.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
    let keep = prefix.rfind("\n\n").map(|i| prefix[..i + 2].to_string()).unwrap_or_default();
    if keep.is_empty() {
        return Ok(doc.to_string());
    }
    let after = removed.position().unwrap_or(isize::MAX);
    let next = apps
        .iter_mut()
        .filter_map(|(_, item)| item.as_table_mut())
        .filter(|t| t.position().is_some_and(|p| p > after))
        .min_by_key(|t| t.position());
    match next {
        Some(t) => {
            let old = t.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_string();
            t.decor_mut().set_prefix(format!("{keep}{old}"));
        }
        None => {
            let trailing = doc.trailing().as_str().unwrap_or("").to_string();
            doc.set_trailing(format!("{keep}{trailing}"));
        }
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PartName {
        PartName::parse_any(s).unwrap()
    }

    const FILE: &str = "# my apps\n\n[apps.wall-follower] # follows walls\nenabled = true\n\n\
        [apps.wall-follower.roles]\nlidar = \"lidar\" # the LD19\nbase = \"base\"\nproximity = [\"front\", \"mega.rear\"]\n";

    #[test]
    fn reads_which_apps_are_on_and_their_parts() {
        let c = parse(FILE).unwrap();
        let e = c.entry("wall-follower");
        assert!(e.enabled);
        assert_eq!(e.roles["lidar"], [p("lidar")]);
        assert_eq!(e.roles["proximity"], [p("front"), p("mega.rear")]);
        assert_eq!(c.entry("other"), AppEntry::default(), "an app not in the file is off");
        assert_eq!(parse("").unwrap(), AppsConfig::default());
    }

    #[test]
    fn mistakes_name_their_line() {
        for (text, line, words) in [
            ("[apps.x]\nenabled = \"yes\"\n", 2, "invalid type"),
            ("[apps.x]\ncolour = 1\n", 2, "unknown field"),
            ("[apps.x.roles]\nlidar = \"Lidar\"\n", 2, "isn't a valid part name"),
            ("[apps.x.roles]\nlidar = 3\n", 2, "did not match"),
            ("[apps.X]\nenabled = true\n", 1, "not an app id"),
        ] {
            let e = parse(text).unwrap_err();
            assert!(e.message.contains(words), "{text}: {e:?}");
            assert_eq!(e.line, Some(line), "{text}: {e:?}");
            assert!(!e.hint.is_empty());
        }
    }

    #[test]
    fn edits_keep_comments_and_write_lists_only_for_list_roles() {
        let off = set_enabled(FILE, "wall-follower", false).unwrap();
        assert!(off.contains("[apps.wall-follower] # follows walls\nenabled = false\n"), "{off}");
        assert!(off.starts_with("# my apps\n") && off.contains("# the LD19"), "{off}");

        let mut roles = BTreeMap::new();
        roles.insert("lidar".to_string(), vec![p("front_lidar")]);
        roles.insert("proximity".to_string(), vec![p("front")]);
        let out = set_roles(FILE, "wall-follower", &roles, &["proximity"]).unwrap();
        assert!(out.contains("lidar = \"front_lidar\" # the LD19"), "{out}");
        assert!(out.contains("proximity = [\"front\"]"), "{out}");
        assert!(!out.contains("base ="), "roles left out are removed: {out}");
        assert_eq!(parse(&out).unwrap().entry("wall-follower").roles, roles);
    }

    #[test]
    fn a_new_app_is_added_off_and_removed_again() {
        let mut roles = BTreeMap::new();
        roles.insert("base".to_string(), vec![p("base")]);
        let out = set_roles(NEW_FILE_HEADER, "sennep", &roles, &[]).unwrap();
        assert!(out.starts_with(NEW_FILE_HEADER), "{out}");
        assert!(out.contains("comments.\n\n[apps.sennep]\nenabled = false\n"), "{out}");
        assert!(out.contains("[apps.sennep.roles]\nbase = \"base\"\n"), "{out}");
        assert!(!parse(&out).unwrap().entry("sennep").enabled);
        let on = set_enabled(&out, "sennep", true).unwrap();
        assert!(parse(&on).unwrap().entry("sennep").enabled);
        let gone = remove_app(&on, "sennep").unwrap();
        assert_eq!(parse(&gone).unwrap(), AppsConfig::default());
        assert!(gone.starts_with(NEW_FILE_HEADER) && !gone.contains("sennep"), "{gone}");
    }
}
