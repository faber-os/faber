//! Layer 2: the machine's settings (docs/api.md, "Settings"), kept in `config/settings.toml`.
//!
//! One shared settings object, so the browser and the attached screen always agree. The types
//! are also the wire format (re-exported by `api::dto`).

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::sync::watch;
use ts_rs::TS;

pub const FILE_NAME: &str = "settings.toml";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Theme {
    /// Follow the browser or screen's light/dark preference.
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Appearance {
    pub theme: Theme,
    /// A built-in wallpaper id, e.g. `aluminium`.
    pub wallpaper: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance { theme: Theme::System, wallpaper: "aluminium".into() }
    }
}

/// Where a desktop icon sits (pixels from the desktop's top-left). `key` is `tool:<id>`,
/// `accessory:<id>` or a file path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IconPosition {
    pub key: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct DesktopLayout {
    pub icons: Vec<IconPosition>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct DeveloperSettings {
    /// Files can browse the whole machine. Only changed through `POST /api/v0/settings/developer`.
    pub full_machine_files: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    pub appearance: Appearance,
    pub desktop: DesktopLayout,
    pub developer: DeveloperSettings,
}

/// Body of `POST /api/v0/settings/developer`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeveloperChange {
    pub full_machine_files: bool,
    /// Must be true to turn whole-machine access on: the user confirmed the warning.
    #[serde(default)]
    pub acknowledged_risk: bool,
}

/// Why a settings change was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl SettingsError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        SettingsError { code, message: message.into(), hint: hint.into() }
    }
}

const HEADER: &str =
    "# Faber settings. Changed from the shell (Settings); safe to edit by hand while Faber is stopped.\n";

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
    changed: watch::Sender<u64>,
}

impl SettingsStore {
    /// Loads `config/settings.toml`. A missing file means defaults; a broken file also means
    /// defaults (logged), and it is only overwritten when the user changes a setting.
    pub fn load(config_dir: &Path) -> SettingsStore {
        let path = config_dir.join(FILE_NAME);
        let current = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e: toml::de::Error| {
                tracing::warn!(target: "core::settings", "settings.toml can't be read ({}), using defaults", e.message());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        SettingsStore { path, current: Mutex::new(current), changed: watch::channel(0).0 }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings lock").clone()
    }

    /// Fires on every change.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// Replaces appearance and desktop. The developer settings must stay as they are.
    pub fn update(&self, new: Settings) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        if new.developer != cur.developer {
            return Err(SettingsError::new(
                "use-developer-endpoint",
                "Developer settings can't be changed together with the others.",
                "Change them in Settings → Developer.",
            ));
        }
        self.save(&mut cur, new)
    }

    pub fn change_developer(&self, change: DeveloperChange) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        if change.full_machine_files && !cur.developer.full_machine_files && !change.acknowledged_risk {
            return Err(SettingsError::new(
                "risk-not-acknowledged",
                "Whole-machine file access needs you to confirm the warning first.",
                "Read the warning in Settings → Developer and confirm it.",
            ));
        }
        let mut new = cur.clone();
        new.developer.full_machine_files = change.full_machine_files;
        self.save(&mut cur, new)
    }

    fn save(&self, cur: &mut Settings, new: Settings) -> Result<Settings, SettingsError> {
        let text = format!("{HEADER}\n{}", toml::to_string(&new).expect("settings serialise"));
        write_atomically(&self.path, &text).map_err(|e| {
            SettingsError::new(
                "settings-not-saved",
                format!("Could not save settings.toml: {e}"),
                "Check that the data folder is writable and the disk isn't full.",
            )
        })?;
        if *cur != new {
            *cur = new;
            self.changed.send_modify(|n| *n += 1);
        }
        Ok(cur.clone())
    }
}

fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_without_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        assert_eq!(s.get(), Settings::default());
        assert_eq!(s.get().appearance.wallpaper, "aluminium");
    }

    #[test]
    fn updates_are_saved_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        let rx = s.watch();
        let mut new = s.get();
        new.appearance.theme = Theme::Dark;
        new.desktop.icons.push(IconPosition { key: "accessory:files".into(), x: 10.0, y: 20.0 });
        s.update(new.clone()).unwrap();
        assert_eq!(*rx.borrow(), 1);
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.starts_with("# Faber settings"));
        assert_eq!(SettingsStore::load(dir.path()).get(), new);
    }

    #[test]
    fn developer_settings_only_change_through_their_own_path() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        let mut sneaky = s.get();
        sneaky.developer.full_machine_files = true;
        assert_eq!(s.update(sneaky).unwrap_err().code, "use-developer-endpoint");
    }

    #[test]
    fn whole_machine_access_needs_the_warning_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        let e = s
            .change_developer(DeveloperChange { full_machine_files: true, acknowledged_risk: false })
            .unwrap_err();
        assert_eq!(e.code, "risk-not-acknowledged");
        assert!(!s.get().developer.full_machine_files);
        let on = s
            .change_developer(DeveloperChange { full_machine_files: true, acknowledged_risk: true })
            .unwrap();
        assert!(on.developer.full_machine_files);
        let off = s
            .change_developer(DeveloperChange { full_machine_files: false, acknowledged_risk: false })
            .unwrap();
        assert!(!off.developer.full_machine_files, "turning it off needs no confirmation");
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), "appearance = 5").unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get(), Settings::default());
    }
}
