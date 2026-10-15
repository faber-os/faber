//! Layer 1: part names and signal addresses (docs/glossary.md), and language codes.

use std::fmt;

const MAX_PART_NAME: usize = 32;

/// Joins a bridge's name and the name of a part on its board: `mega.left` (docs/faberlink.md §9).
/// A part name the user picks can't contain it, and a signal name never does, so
/// `mega.left/motor` can only mean "the `motor` signal of the sub-part `mega.left`", and
/// `mega/left` only "the `left` signal of the part `mega`".
pub const SUB_PART_SEPARATOR: char = '.';

/// A part's name. A part the user adds has a name they chose: lowercase letters, digits and `_`,
/// starting with a letter. A part on a bridge's board (a sub-part) is named after the bridge and
/// the board's own name for it, `<bridge>.<name>`, e.g. `mega.left`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartName(String);

/// Why a part name can't be used, with what to do instead.
#[derive(Debug, PartialEq, Eq)]
pub struct BadName {
    pub message: String,
    pub hint: String,
}

const NAME_HINT: &str = "Use lowercase letters, digits and _, starting with a letter, e.g. front_lidar.";

/// One name without a separator; `whole` is what the user wrote, for the message.
fn check_segment(s: &str, whole: &str) -> Result<(), BadName> {
    let bad = |message: String| BadName { message, hint: NAME_HINT.into() };
    if whole.is_empty() {
        return Err(bad("The part needs a name.".into()));
    }
    if s.len() > MAX_PART_NAME {
        return Err(bad(format!("\"{s}\" is longer than {MAX_PART_NAME} characters.")));
    }
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    if !first_ok || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(bad(format!("\"{whole}\" isn't a valid part name.")));
    }
    Ok(())
}

impl PartName {
    /// A name the user picks for a part they add. Never a sub-part's: those come from the board.
    pub fn parse(s: &str) -> Result<PartName, BadName> {
        if s.contains(SUB_PART_SEPARATOR) {
            return Err(BadName {
                message: format!(
                    "\"{s}\" isn't a valid part name: a . only joins a board's name and the name of a part on it."
                ),
                hint: NAME_HINT.into(),
            });
        }
        check_segment(s, s)?;
        Ok(PartName(s.to_string()))
    }

    /// Any part's name: one the user picked, or a sub-part's `<bridge>.<name>`.
    pub fn parse_any(s: &str) -> Result<PartName, BadName> {
        match s.split_once(SUB_PART_SEPARATOR) {
            None => PartName::parse(s),
            Some((bridge, name)) => PartName::sub(&PartName::parse(bridge)?, name),
        }
    }

    /// The sub-part `name` on the bridge `bridge`: `mega` and `left` give `mega.left`.
    pub fn sub(bridge: &PartName, name: &str) -> Result<PartName, BadName> {
        let whole = format!("{bridge}{SUB_PART_SEPARATOR}{name}");
        if bridge.is_sub() || name.contains(SUB_PART_SEPARATOR) {
            return Err(BadName {
                message: format!("\"{whole}\" isn't a valid part name: only a bridge has parts of its own."),
                hint: NAME_HINT.into(),
            });
        }
        check_segment(name, &whole)?;
        Ok(PartName(whole))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True for a part on a bridge's board.
    pub fn is_sub(&self) -> bool {
        self.0.contains(SUB_PART_SEPARATOR)
    }

    /// The bridge a sub-part is on (`mega` for `mega.left`); `None` for any other part.
    pub fn bridge(&self) -> Option<PartName> {
        self.0.split_once(SUB_PART_SEPARATOR).map(|(b, _)| PartName(b.to_string()))
    }

    /// The name without its bridge: `left` for `mega.left`, the whole name otherwise.
    pub fn local(&self) -> &str {
        self.0.split_once(SUB_PART_SEPARATOR).map_or(&self.0, |(_, n)| n)
    }
}

impl fmt::Display for PartName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A board's own name for one of its parts (whatever text its sketch used) as the name of a
/// sub-part: lowercase, with every run of other characters turned into one `_`. `None` if nothing
/// usable is left.
pub fn board_part_name(raw: &str) -> Option<String> {
    let mut out = String::new();
    for c in raw.trim().chars().map(|c| c.to_ascii_lowercase()) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_');
    check_segment(out, out).ok().map(|()| out.to_string())
}

/// What derived signals' addresses start with: `app:<id>/<name>` (D24). A part's name can never
/// contain `:`, so no part can be named to clash with an app's signals.
pub const APP_PREFIX: &str = "app:";

/// Who publishes a signal: a part, or an app (a derived signal, read-only).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    Part(PartName),
    /// An app's id (lowercase words joined by -).
    App(String),
}

/// `part/name` (where a part's signal is published) or `app:<id>/name` (an app's derived signal).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignalAddress {
    pub owner: Owner,
    pub signal: String,
}

impl SignalAddress {
    pub fn new(part: PartName, signal: impl Into<String>) -> Self {
        SignalAddress { owner: Owner::Part(part), signal: signal.into() }
    }

    /// The derived signal `name` of the app `app`: `app:<app>/<name>`.
    pub fn derived(app: &str, signal: impl Into<String>) -> Self {
        debug_assert!(faber_connector::is_kebab_name(app), "app ids are checked by the manifest reader");
        SignalAddress { owner: Owner::App(app.to_string()), signal: signal.into() }
    }

    /// `part/name`, where the part may be a sub-part (`mega.left/motor`), or `app:<id>/name`.
    pub fn parse(s: &str) -> Option<SignalAddress> {
        let (owner, signal) = s.split_once('/')?;
        if !faber_connector::is_kebab_name(signal) {
            return None;
        }
        match owner.strip_prefix(APP_PREFIX) {
            Some(app) => faber_connector::is_kebab_name(app).then(|| SignalAddress::derived(app, signal)),
            None => Some(SignalAddress::new(PartName::parse_any(owner).ok()?, signal)),
        }
    }

    /// The part that publishes it; `None` for an app's derived signal.
    pub fn part(&self) -> Option<&PartName> {
        match &self.owner {
            Owner::Part(p) => Some(p),
            Owner::App(_) => None,
        }
    }
}

impl fmt::Display for SignalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.owner {
            Owner::Part(p) => write!(f, "{p}/{}", self.signal),
            Owner::App(a) => write!(f, "{APP_PREFIX}{a}/{}", self.signal),
        }
    }
}

/// A language code as Faber uses it in file names: `da`, `fil` or `pt-BR`. Nothing else, so a
/// code can never reach outside the translations folder.
pub fn valid_language_code(code: &str) -> bool {
    let (lang, region) = match code.split_once('-') {
        Some((l, r)) => (l, Some(r)),
        None => (code, None),
    };
    (2..=3).contains(&lang.len())
        && lang.bytes().all(|b| b.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_part_names() {
        for ok in ["lidar", "front_lidar", "a1", "x"] {
            assert_eq!(PartName::parse(ok).unwrap().as_str(), ok);
        }
    }

    #[test]
    fn bad_part_names_explain_the_rule() {
        for bad in ["", "Lidar", "1lidar", "front-lidar", "front lidar", "_x", &"a".repeat(33), "mega.left"] {
            let e = PartName::parse(bad).unwrap_err();
            assert!(e.hint.contains("lowercase"), "{bad}");
        }
    }

    #[test]
    fn addresses_round_trip() {
        let a = SignalAddress::parse("front_lidar/laser-scan").unwrap();
        assert_eq!(a.part().unwrap().as_str(), "front_lidar");
        assert_eq!(a.signal, "laser-scan");
        assert_eq!(a.to_string(), "front_lidar/laser-scan");
        for bad in ["lidar", "lidar/", "/laser-scan", "Lidar/laser-scan", "lidar/laser_scan"] {
            assert_eq!(SignalAddress::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn sub_parts_are_named_after_their_bridge() {
        let mega = PartName::parse("mega").unwrap();
        let left = PartName::sub(&mega, "left").unwrap();
        assert_eq!(left.as_str(), "mega.left");
        assert!(left.is_sub() && !mega.is_sub());
        assert_eq!(left.bridge(), Some(mega.clone()));
        assert_eq!((left.local(), mega.local()), ("left", "mega"));
        assert_eq!(mega.bridge(), None);
        assert_eq!(PartName::parse_any("mega.left").unwrap(), left);
        assert_eq!(PartName::parse_any("mega").unwrap(), mega);
        for bad in ["mega.", ".left", "mega.left.x", "Mega.left", "mega.Left", "mega.1x", ""] {
            assert!(PartName::parse_any(bad).is_err(), "{bad}");
        }
        assert!(PartName::sub(&left, "x").is_err(), "a sub-part hosts nothing");
    }

    #[test]
    fn a_name_the_user_picks_never_looks_like_a_sub_part() {
        let e = PartName::parse("mega.left").unwrap_err();
        assert!(e.message.contains("board"), "{e:?}");
    }

    #[test]
    fn sub_part_addresses_are_never_ambiguous() {
        // The `left` signal of the part `mega`, and the `motor` signal of the sub-part `mega.left`.
        let of_bridge = SignalAddress::parse("mega/left").unwrap();
        assert_eq!((of_bridge.part().unwrap().as_str(), of_bridge.signal.as_str()), ("mega", "left"));
        let of_sub = SignalAddress::parse("mega.left/motor").unwrap();
        assert_eq!((of_sub.part().unwrap().as_str(), of_sub.signal.as_str()), ("mega.left", "motor"));
        assert_eq!(of_sub.to_string(), "mega.left/motor");
        assert_eq!(SignalAddress::parse("mega/left/motor"), None, "a / never joins parts");
    }

    #[test]
    fn derived_signals_are_addressed_by_app_and_never_clash_with_a_part() {
        let a = SignalAddress::parse("app:wall-follower/wall-distance").unwrap();
        assert_eq!(a, SignalAddress::derived("wall-follower", "wall-distance"));
        assert_eq!((a.owner.clone(), a.part()), (Owner::App("wall-follower".into()), None));
        assert_eq!(a.to_string(), "app:wall-follower/wall-distance");
        for bad in ["app:/x", "app:Wall/x", "app:wall_follower/x", "app:w/x_y", "app:a:b/x"] {
            assert_eq!(SignalAddress::parse(bad), None, "{bad}");
        }
        // No part can be named like an app's signals: a part name never holds a `:`.
        assert!(PartName::parse("app:wall-follower").is_err());
        assert!(PartName::parse_any("app:x").is_err());
        // A part may be called `app`; its signals are `app/…`, never `app:…/…`.
        let part = SignalAddress::parse("app/pose").unwrap();
        assert_eq!(part.part().map(PartName::as_str), Some("app"));
        assert_ne!(part, SignalAddress::derived("app", "pose"));
    }

    #[test]
    fn board_names_become_sub_part_names() {
        assert_eq!(board_part_name("left").as_deref(), Some("left"));
        assert_eq!(board_part_name("Left Motor").as_deref(), Some("left_motor"));
        assert_eq!(board_part_name("front-IR #2").as_deref(), Some("front_ir_2"));
        for unusable in ["", "  ", "2nd", "ø", &"a".repeat(33)] {
            assert_eq!(board_part_name(unusable), None, "{unusable}");
        }
    }
}
