//! Layer 2: translations of the shell installed on this machine (docs/translations.md, D44),
//! kept as `<code>.json` files in `config/translations/`.
//!
//! The core only lists and serves the files. It doesn't know the English catalogue, so how
//! complete a translation is gets worked out by the shell, which has the catalogue.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub use crate::names::valid_language_code as valid_code;

pub const FOLDER: &str = "translations";

/// A translation file bigger than this is refused: a whole shell is a few hundred short texts.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The key in a translation file that holds the language's own name, e.g. `"Dansk"`.
const NAME_KEY: &str = "_language";

/// One translation installed on the machine (`GET /api/v0/translations`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TranslationInfo {
    /// The language code, from the file name: `da` for `da.json`, `pt-BR` for `pt-BR.json`.
    pub code: String,
    /// The language's own name, from the file's `_language`; the code if the file has none.
    pub name: String,
    /// Set when the file can't be used (broken JSON, too big), with the line where possible.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationErrorKind {
    BadRequest,
    NotFound,
    Unprocessable,
}

/// Why a translation can't be served, in words the user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationError {
    pub kind: TranslationErrorKind,
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl TranslationError {
    fn new(
        kind: TranslationErrorKind,
        code: &'static str,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        TranslationError { kind, code, message: message.into(), hint: hint.into() }
    }
}

/// The translations folder, `<data>/config/translations`.
pub struct Translations {
    dir: PathBuf,
}

impl Translations {
    /// Uses `config_dir/translations`, creating it so there is an obvious place to put a file.
    pub fn open(config_dir: &Path) -> Translations {
        let dir = config_dir.join(FOLDER);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(target: "core::translations", "Could not create {}: {e}", dir.display());
        }
        Translations { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every `<code>.json` in the folder, sorted by code. Other files are ignored.
    pub fn list(&self) -> Vec<TranslationInfo> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut found: Vec<TranslationInfo> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let code = name.strip_suffix(".json")?;
                valid_code(code).then(|| code.to_string())
            })
            .map(|code| match self.get(&code) {
                Ok(file) => TranslationInfo {
                    name: language_name(&file).unwrap_or_else(|| code.clone()),
                    code,
                    problem: None,
                },
                Err(e) => TranslationInfo { name: code.clone(), code, problem: Some(e.message) },
            })
            .collect();
        found.sort_by(|a, b| a.code.cmp(&b.code));
        found
    }

    /// The file for `code`, as JSON. It must be one JSON object; its texts are checked by the
    /// shell, which falls back to English for anything it can't use.
    pub fn get(&self, code: &str) -> Result<serde_json::Map<String, serde_json::Value>, TranslationError> {
        use TranslationErrorKind::*;
        if !valid_code(code) {
            return Err(TranslationError::new(
                BadRequest,
                "invalid-language",
                format!("\"{code}\" isn't a language code."),
                "Use a code like da, fil or pt-BR: 2–3 lowercase letters, optionally a dash and 2 capitals.",
            ));
        }
        let file = format!("{code}.json");
        let path = self.dir.join(&file);
        let too_big = std::fs::metadata(&path).map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(false);
        if too_big {
            return Err(TranslationError::new(
                Unprocessable,
                "translation-invalid",
                format!("{file} is bigger than 1 MB, so it can't be a translation."),
                "Check that the right file was copied into config/translations.",
            ));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => TranslationError::new(
                NotFound,
                "unknown-language",
                format!("There is no translation for \"{code}\" on this machine."),
                format!("Put {file} in the data folder's config/translations folder."),
            ),
            _ => TranslationError::new(
                Unprocessable,
                "translation-invalid",
                format!("{file} can't be read: {e}"),
                "Save it again as UTF-8 text.",
            ),
        })?;
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(map)) => Ok(map),
            Ok(_) => Err(TranslationError::new(
                Unprocessable,
                "translation-invalid",
                format!("{file} must be one JSON object, {{ \"key\": \"text\", … }}."),
                "Start from the English list printed by npm run i18n:check, see docs/translations.md.",
            )),
            Err(e) => Err(TranslationError::new(
                Unprocessable,
                "translation-invalid",
                format!("{file} has a problem on line {}: {}", e.line(), without_position(&e)),
                "Check that line for a missing comma, quote or bracket.",
            )),
        }
    }
}

fn language_name(file: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let name = file.get(NAME_KEY)?.as_str()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// serde_json's message without its " at line 3 column 5", which the caller says in words.
fn without_position(e: &serde_json::Error) -> String {
    let text = e.to_string();
    let suffix = format!(" at line {} column {}", e.line(), e.column());
    text.strip_suffix(&suffix).unwrap_or(&text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> (tempfile::TempDir, Translations) {
        let dir = tempfile::tempdir().unwrap();
        let t = Translations::open(dir.path());
        (dir, t)
    }

    fn write(t: &Translations, file: &str, text: &str) {
        std::fs::write(t.dir().join(file), text).unwrap();
    }

    #[test]
    fn language_codes_are_strict() {
        for good in ["da", "fil", "pt-BR", "en-GB"] {
            assert!(valid_code(good), "{good}");
        }
        for bad in [
            "", "d", "dans", "DA", "da-br", "da-BRA", "da_BR", "da-", "-BR", "..", "../da", "da/x",
            "da.json", "da\\x", "æø",
        ] {
            assert!(!valid_code(bad), "{bad}");
        }
    }

    #[test]
    fn opening_creates_the_folder() {
        let (dir, t) = folder();
        assert!(dir.path().join("translations").is_dir());
        assert_eq!(t.list(), vec![]);
    }

    #[test]
    fn lists_each_language_with_its_own_name() {
        let (_d, t) = folder();
        write(&t, "da.json", r#"{ "_language": "Dansk", "parts.title": "Dele" }"#);
        write(&t, "pt-BR.json", r#"{ "parts.title": "Peças" }"#);
        write(&t, "notes.txt", "not a translation");
        write(&t, "Bad Name.json", "{}");
        assert_eq!(
            t.list(),
            vec![
                TranslationInfo { code: "da".into(), name: "Dansk".into(), problem: None },
                TranslationInfo { code: "pt-BR".into(), name: "pt-BR".into(), problem: None },
            ]
        );
    }

    #[test]
    fn serves_a_file_as_json() {
        let (_d, t) = folder();
        write(&t, "da.json", r#"{ "_language": "Dansk", "parts.count": { "one": "{count} del" } }"#);
        let file = t.get("da").unwrap();
        assert_eq!(file["parts.count"]["one"], "{count} del");
    }

    #[test]
    fn broken_json_names_the_line() {
        let (_d, t) = folder();
        write(&t, "da.json", "{\n  \"a\": \"b\"\n  \"c\": \"d\"\n}\n");
        let e = t.get("da").unwrap_err();
        assert_eq!(e.kind, TranslationErrorKind::Unprocessable);
        assert_eq!(e.code, "translation-invalid");
        assert!(e.message.starts_with("da.json has a problem on line 3: "), "{}", e.message);
        assert!(!e.message.contains("column"), "{}", e.message);
        let listed = t.list();
        assert_eq!(listed[0].name, "da");
        assert_eq!(listed[0].problem.as_deref(), Some(e.message.as_str()));
    }

    #[test]
    fn a_file_must_be_one_object() {
        let (_d, t) = folder();
        write(&t, "da.json", r#"["Dele"]"#);
        assert_eq!(t.get("da").unwrap_err().code, "translation-invalid");
    }

    #[test]
    fn unknown_and_invalid_codes_are_refused_without_touching_the_disk() {
        let (dir, t) = folder();
        std::fs::write(dir.path().join("secret.json"), "{}").unwrap();
        let e = t.get("de").unwrap_err();
        assert_eq!((e.kind, e.code), (TranslationErrorKind::NotFound, "unknown-language"));
        for sneaky in ["../secret", "..", "da/../../secret", "DA"] {
            let e = t.get(sneaky).unwrap_err();
            assert_eq!((e.kind, e.code), (TranslationErrorKind::BadRequest, "invalid-language"), "{sneaky}");
        }
    }

    #[test]
    fn a_huge_file_is_refused() {
        let (_d, t) = folder();
        let big = format!("{{\"a\": \"{}\"}}", "x".repeat(MAX_FILE_BYTES as usize));
        write(&t, "da.json", &big);
        assert!(t.get("da").unwrap_err().message.contains("bigger than 1 MB"));
    }
}
