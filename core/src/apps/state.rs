//! An app's small key-value store (the `state` interface), saved in `<data>/apps/<id>/state.toml`
//! so it outlives restarts and updates.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

/// The most an app may store: its keys and values together, in bytes.
pub const MAX_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct StateStore {
    path: Option<PathBuf>,
    values: BTreeMap<String, String>,
    dirty: bool,
}

fn size(values: &BTreeMap<String, String>) -> usize {
    values.iter().map(|(k, v)| k.len() + v.len()).sum()
}

impl StateStore {
    /// A store kept only in memory (tests, and apps whose folder can't be written).
    pub fn in_memory() -> Self {
        StateStore { path: None, values: BTreeMap::new(), dirty: false }
    }

    /// The store saved at `path`. A missing file is an empty store; a file that can't be read
    /// is logged and set aside as `state.toml.broken`, so the app starts empty instead of not at all.
    pub fn open(path: PathBuf, app: &str) -> Self {
        let values = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<BTreeMap<String, String>>(&text) {
                Ok(v) if size(&v) <= MAX_BYTES => v,
                outcome => {
                    let why = match outcome {
                        Err(e) => e.message().trim().to_string(),
                        Ok(_) => format!("it holds more than {} KB", MAX_BYTES / 1024),
                    };
                    tracing::error!(
                        target: "core::apps",
                        app,
                        "{}'s saved state can't be used ({why}); it starts empty, and the old file is kept as state.toml.broken",
                        app
                    );
                    let _ = std::fs::rename(&path, path.with_extension("toml.broken"));
                    BTreeMap::new()
                }
            },
            Err(_) => BTreeMap::new(),
        };
        StateStore { path: Some(path), values, dirty: false }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    /// Sets `key`, unless the store would grow past [`MAX_BYTES`]; then it says so.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let old = self.values.get(key).map_or(0, |v| key.len() + v.len());
        let new = size(&self.values) - old + key.len() + value.len();
        if new > MAX_BYTES {
            return Err(format!(
                "The app's saved state would be {} KB; it may be at most {} KB. Store less, or remove keys it no longer needs.",
                new.div_ceil(1024),
                MAX_BYTES / 1024
            ));
        }
        if self.values.get(key).map(String::as_str) != Some(value) {
            self.values.insert(key.to_string(), value.to_string());
            self.dirty = true;
        }
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        if self.values.remove(key).is_some() {
            self.dirty = true;
        }
    }

    pub fn keys(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }

    /// Writes the store if it changed since it was last saved.
    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if !self.dirty {
            return Ok(());
        }
        let text = toml::to_string(&self.values).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, format!("# What this app saved (Faber writes this file).\n{text}"))?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_outlive_the_store_and_only_changes_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let mut s = StateStore::open(path.clone(), "a");
        assert_eq!(s.get("laps"), None);
        s.set("laps", "3").unwrap();
        s.set("odd key = \"x\"", "multi\nline").unwrap();
        s.save().unwrap();
        let s2 = StateStore::open(path.clone(), "a");
        assert_eq!((s2.get("laps").as_deref(), s2.keys().len()), (Some("3"), 2));
        assert_eq!(s2.get("odd key = \"x\"").as_deref(), Some("multi\nline"));
        std::fs::remove_file(&path).unwrap();
        let mut s3 = StateStore::open(path.clone(), "a");
        s3.save().unwrap();
        assert!(!path.exists(), "nothing changed, nothing written");
        s3.set("x", "1").unwrap();
        s3.remove("x");
        s3.save().unwrap();
        assert!(path.exists());
    }

    #[test]
    fn the_store_is_limited_to_64_kb() {
        let mut s = StateStore::in_memory();
        s.set("big", &"x".repeat(MAX_BYTES - 3)).unwrap();
        let e = s.set("more", "y").unwrap_err();
        assert!(e.contains("at most 64 KB"), "{e}");
        s.set("big", "small").unwrap();
        s.set("more", "y").unwrap();
    }

    #[test]
    fn a_broken_file_is_set_aside_and_the_app_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "laps = [").unwrap();
        let s = StateStore::open(path.clone(), "a");
        assert!(s.keys().is_empty());
        assert!(dir.path().join("state.toml.broken").exists() && !path.exists());
    }
}
