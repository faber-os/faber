//! An app's small key-value store, saved in `<data>/apps/<id>/state.toml` so it outlives
//! restarts and updates.
//!
//! One [`SharedState`] per app, held by the machine: its logic (the `state` interface, every
//! run) and its windows (through the API) use the same store, and every change is announced, so
//! a slider in the window and the logic that reads it always agree.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

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

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    /// Its keys and values together, in bytes (at most [`MAX_BYTES`]).
    pub fn bytes(&self) -> usize {
        size(&self.values)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
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

/// One change to an app's store: `value` is `None` when the key was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateChange {
    pub key: String,
    pub value: Option<String>,
}

struct Kept {
    store: StateStore,
    saved: Option<Instant>,
    /// A restore is about to write the file: nothing may write it until [`SharedState::reload`].
    suspended: bool,
}

/// An app's store, shared by its logic and its windows, announcing every change.
pub struct SharedState {
    app: String,
    kept: Mutex<Kept>,
    tx: broadcast::Sender<StateChange>,
}

impl SharedState {
    fn with(app: &str, store: StateStore) -> SharedState {
        SharedState {
            app: app.to_string(),
            kept: Mutex::new(Kept { store, saved: None, suspended: false }),
            tx: broadcast::channel(256).0,
        }
    }

    /// The store saved at `path` (see [`StateStore::open`]).
    pub fn open(path: PathBuf, app: &str) -> SharedState {
        SharedState::with(app, StateStore::open(path, app))
    }

    /// A store kept only in memory (tests).
    pub fn in_memory(app: &str) -> SharedState {
        SharedState::with(app, StateStore::in_memory())
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().expect("app state lock")
    }

    /// Every change from now on, whoever made it.
    pub fn subscribe(&self) -> broadcast::Receiver<StateChange> {
        self.tx.subscribe()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.kept().store.get(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.kept().store.keys()
    }

    /// Every key and value now.
    pub fn values(&self) -> BTreeMap<String, String> {
        self.kept().store.values().clone()
    }

    pub fn bytes(&self) -> usize {
        self.kept().store.bytes()
    }

    /// Sets `key` (see [`StateStore::set`]) and announces it if the value changed.
    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let changed = {
            let mut k = self.kept();
            let changed = k.store.get(key).as_deref() != Some(value);
            k.store.set(key, value)?;
            changed
        };
        if changed {
            let _ = self.tx.send(StateChange { key: key.to_string(), value: Some(value.to_string()) });
        }
        Ok(())
    }

    /// Removes `key` and announces it, if it was there.
    pub fn remove(&self, key: &str) {
        let was = {
            let mut k = self.kept();
            let was = k.store.get(key).is_some();
            k.store.remove(key);
            was
        };
        if was {
            let _ = self.tx.send(StateChange { key: key.to_string(), value: None });
        }
    }

    /// Writes the store now if it changed.
    pub fn save(&self) -> io::Result<()> {
        let mut k = self.kept();
        if k.suspended {
            return Ok(());
        }
        let dirty = k.store.is_dirty();
        k.store.save()?;
        if dirty {
            k.saved = Some(Instant::now());
        }
        Ok(())
    }

    /// Writes the store if it changed and wasn't written in the last `every`: at most once a
    /// second, however often a slider moves.
    pub fn save_if_due(&self, every: Duration) {
        let due = {
            let k = self.kept();
            k.store.is_dirty() && k.saved.is_none_or(|t| t.elapsed() >= every)
        };
        if due && let Err(e) = self.save() {
            tracing::error!(target: "core::apps", app = self.app.as_str(), "its state couldn't be saved: {e}");
        }
    }

    /// Stops writing the file until [`SharedState::reload`]: a restore is about to replace it,
    /// and a change waiting to be saved must not overwrite what the restore wrote.
    pub fn suspend_saving(&self) {
        self.kept().suspended = true;
    }

    /// Reads the file again (a restore wrote it), announcing every key that differs.
    pub fn reload(&self, path: PathBuf) {
        let fresh = StateStore::open(path, &self.app);
        let changes: Vec<StateChange> = {
            let mut k = self.kept();
            let (old, new) = (k.store.values().clone(), fresh.values().clone());
            k.store = fresh;
            k.suspended = false;
            let removed = old
                .keys()
                .filter(|key| !new.contains_key(*key))
                .map(|key| StateChange { key: key.clone(), value: None });
            let set = new
                .iter()
                .filter(|(key, v)| old.get(*key) != Some(v))
                .map(|(key, v)| StateChange { key: key.clone(), value: Some(v.clone()) });
            removed.chain(set).collect()
        };
        for c in changes {
            let _ = self.tx.send(c);
        }
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

    #[test]
    fn a_shared_store_announces_every_change_once() {
        let s = SharedState::in_memory("a");
        let mut rx = s.subscribe();
        s.set("speed", "0.3").unwrap();
        s.set("speed", "0.3").unwrap();
        s.remove("nothing");
        s.remove("speed");
        assert_eq!(rx.try_recv().unwrap(), StateChange { key: "speed".into(), value: Some("0.3".into()) });
        assert_eq!(rx.try_recv().unwrap(), StateChange { key: "speed".into(), value: None });
        assert!(rx.try_recv().is_err(), "an unchanged value and a missing key say nothing");
        assert!(s.set("big", &"x".repeat(MAX_BYTES + 1)).is_err());
        assert!(rx.try_recv().is_err(), "a refused value says nothing");
    }

    #[test]
    fn a_shared_store_saves_at_most_once_a_second_and_reloads_what_a_restore_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let s = SharedState::open(path.clone(), "a");
        s.set("speed", "0.3").unwrap();
        s.save_if_due(Duration::from_secs(1));
        assert!(path.exists(), "the first change is written at once");
        s.set("speed", "0.4").unwrap();
        s.save_if_due(Duration::from_secs(1));
        assert!(std::fs::read_to_string(&path).unwrap().contains("0.3"), "not again within a second");
        s.save_if_due(Duration::ZERO);
        assert!(std::fs::read_to_string(&path).unwrap().contains("0.4"));
        s.set("speed", "0.45").unwrap();
        s.suspend_saving();
        std::fs::write(&path, "speed = \"0.5\"\nlaps = \"2\"\n").unwrap();
        s.save_if_due(Duration::ZERO);
        assert!(std::fs::read_to_string(&path).unwrap().contains("0.5"), "a restore's file is kept");
        let mut rx = s.subscribe();
        s.reload(path);
        assert_eq!(s.get("speed").as_deref(), Some("0.5"));
        let mut seen = [rx.try_recv().unwrap(), rx.try_recv().unwrap()];
        seen.sort_by(|a, b| a.key.cmp(&b.key));
        assert_eq!(seen[0], StateChange { key: "laps".into(), value: Some("2".into()) });
        assert_eq!(seen[1], StateChange { key: "speed".into(), value: Some("0.5".into()) });
    }
}
