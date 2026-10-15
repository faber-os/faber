//! Layer 1: part names and signal addresses (docs/glossary.md), and language codes.

use std::fmt;

const MAX_PART_NAME: usize = 32;

/// A part's name, chosen by the user: lowercase letters, digits and `_`, starting with a letter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartName(String);

/// Why a part name can't be used, with what to do instead.
#[derive(Debug, PartialEq, Eq)]
pub struct BadName {
    pub message: String,
    pub hint: String,
}

impl PartName {
    pub fn parse(s: &str) -> Result<PartName, BadName> {
        let hint = "Use lowercase letters, digits and _, starting with a letter, e.g. front_lidar.";
        let bad = |message: String| BadName { message, hint: hint.into() };
        if s.is_empty() {
            return Err(bad("The part needs a name.".into()));
        }
        if s.len() > MAX_PART_NAME {
            return Err(bad(format!("\"{s}\" is longer than {MAX_PART_NAME} characters.")));
        }
        let mut chars = s.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        if !first_ok || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(bad(format!("\"{s}\" isn't a valid part name.")));
        }
        Ok(PartName(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PartName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `part/name`: where a part's signal is published.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignalAddress {
    pub part: PartName,
    pub signal: String,
}

impl SignalAddress {
    pub fn new(part: PartName, signal: impl Into<String>) -> Self {
        SignalAddress { part, signal: signal.into() }
    }

    pub fn parse(s: &str) -> Option<SignalAddress> {
        let (part, signal) = s.split_once('/')?;
        let part = PartName::parse(part).ok()?;
        faber_connector::is_kebab_name(signal).then(|| SignalAddress::new(part, signal))
    }
}

impl fmt::Display for SignalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.part, self.signal)
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
        for bad in ["", "Lidar", "1lidar", "front-lidar", "front lidar", "_x", &"a".repeat(33)] {
            let e = PartName::parse(bad).unwrap_err();
            assert!(e.hint.contains("lowercase"), "{bad}");
        }
    }

    #[test]
    fn addresses_round_trip() {
        let a = SignalAddress::parse("front_lidar/laser-scan").unwrap();
        assert_eq!(a.part.as_str(), "front_lidar");
        assert_eq!(a.signal, "laser-scan");
        assert_eq!(a.to_string(), "front_lidar/laser-scan");
        for bad in ["lidar", "lidar/", "/laser-scan", "Lidar/laser-scan", "lidar/laser_scan"] {
            assert_eq!(SignalAddress::parse(bad), None, "{bad}");
        }
    }
}
