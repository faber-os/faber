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
    System,
    /// The default: Faber looks the same on every screen until someone picks otherwise.
    #[default]
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Appearance {
    pub theme: Theme,
    /// A built-in style id (colours and accent), e.g. `aluminium`. Every style has a light and a
    /// dark version; `theme` picks which.
    pub style: String,
    /// A built-in wallpaper id, e.g. `aluminium`.
    pub wallpaper: String,
    /// The shell's language (D44): `en`, or the code of an installed translation, e.g. `da`.
    /// A language without a translation (removed since) shows English.
    pub language: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme: Theme::Light,
            style: "aluminium".into(),
            wallpaper: "aluminium".into(),
            language: DEFAULT_LANGUAGE.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum TaskbarSize {
    /// Icons only.
    #[default]
    Small,
    /// Bigger buttons with labels (good for touch screens).
    Large,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Taskbar {
    /// Programs pinned to the taskbar, in order: `tool:<id>`, `accessory:<id>` or `app:<id>`
    /// (an installed app's window).
    pub pinned: Vec<String>,
    pub size: TaskbarSize,
}

impl Default for Taskbar {
    fn default() -> Self {
        Taskbar {
            pinned: ["tool:parts", "accessory:files", "accessory:notes", "tool:serial-monitor"]
                .map(String::from)
                .to_vec(),
            size: TaskbarSize::Small,
        }
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

/// A display preference that can follow the browser or screen, like the theme (D40).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Preference {
    /// Follow the browser or screen (`prefers-reduced-motion`, `prefers-contrast`).
    #[default]
    System,
    On,
    Off,
}

/// Text size in percent of the normal size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum TextSize {
    #[default]
    #[serde(rename = "100")]
    Normal,
    #[serde(rename = "115")]
    Larger115,
    #[serde(rename = "130")]
    Larger130,
    #[serde(rename = "150")]
    Larger150,
}

/// Accessibility (D40): machine-wide, so the attached screen can be set up from a laptop.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Accessibility {
    /// No window animations or smooth movement.
    pub reduce_motion: Preference,
    /// Stronger text, lines and focus colours in the current style.
    pub higher_contrast: Preference,
    pub text_size: TextSize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum LengthUnit {
    /// Metres and centimetres.
    #[default]
    Metric,
    /// Feet and inches.
    Imperial,
}

/// The units the shell shows (D43). Display only: the API, recordings and apps stay SI.
/// Angles get a setting once the shell shows an angle (no setting that does nothing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Units {
    pub length: LengthUnit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ClockFormat {
    /// 14:30
    #[default]
    #[serde(rename = "24h")]
    H24,
    /// 2:30 PM
    #[serde(rename = "12h")]
    H12,
}

/// Date and time (D42): clocks, file dates and log times show the machine's time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct TimeSettings {
    /// An IANA time zone, e.g. `Europe/Copenhagen`. Empty: the machine's own (`GET /api/v0/system`).
    pub time_zone: String,
    pub clock: ClockFormat,
}

/// Which machine this is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct MachineIdentity {
    /// The machine's name, e.g. `sennep`: shown in the browser tab, About and backup names, and
    /// its network name (`sennep.local`) once Faber runs on the Pi image. Lowercase letters,
    /// digits and single dashes, so it is a valid host name.
    pub name: String,
}

impl Default for MachineIdentity {
    fn default() -> Self {
        MachineIdentity { name: DEFAULT_MACHINE_NAME.into() }
    }
}

pub const DEFAULT_MACHINE_NAME: &str = "faber";

/// English: the catalogue built into the shell.
pub const DEFAULT_LANGUAGE: &str = "en";

/// A host name label: 1–63 of `a-z`, `0-9` and `-`, not starting or ending with `-`.
pub fn valid_machine_name(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Settings {
    pub machine: MachineIdentity,
    pub appearance: Appearance,
    pub accessibility: Accessibility,
    pub units: Units,
    pub time: TimeSettings,
    pub desktop: DesktopLayout,
    pub taskbar: Taskbar,
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
            Ok(text) => parse(&text).unwrap_or_else(|e| {
                tracing::warn!(target: "core::settings", "{} Using the defaults.", e.message);
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

    /// Replaces every setting except the developer settings, which must stay as they are.
    pub fn update(&self, new: Settings) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        if new.developer != cur.developer {
            return Err(SettingsError::new(
                "use-developer-endpoint",
                "Developer settings can't be changed together with the others.",
                "Change them in Settings → Developer.",
            ));
        }
        check(&new)?;
        self.save(&mut cur, new)
    }

    /// Pins `program` to the end of the taskbar, or unpins it. Nothing changes (and nothing is
    /// written) when it already is as asked.
    pub fn set_pinned(&self, program: &str, on: bool) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        let pinned = cur.taskbar.pinned.iter().any(|k| k == program);
        if pinned == on {
            return Ok(cur.clone());
        }
        let mut new = cur.clone();
        if on {
            new.taskbar.pinned.push(program.to_string());
        } else {
            new.taskbar.pinned.retain(|k| k != program);
        }
        self.save(&mut cur, new)
    }

    /// Back to the defaults: every setting, including the developer settings (so whole-machine
    /// files are off again). Parts and files are not settings and stay as they are.
    /// Everything back to its default, except the machine's name: resetting the desktop must
    /// not rename the machine (and, on the Pi, its network address).
    pub fn reset(&self) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        let fresh = Settings { machine: cur.machine.clone(), ..Settings::default() };
        self.save(&mut cur, fresh)
    }

    /// Takes settings from a backup. This machine's developer settings stay as they are: a
    /// backup never turns whole-machine access on behind the user's back.
    pub fn restore(&self, mut restored: Settings) -> Result<Settings, SettingsError> {
        let mut cur = self.current.lock().expect("settings lock");
        restored.developer = cur.developer.clone();
        // Anything this machine can't use falls back, one field at a time.
        if !valid_machine_name(&restored.machine.name) {
            restored.machine = cur.machine.clone();
        }
        if !crate::names::valid_language_code(&restored.appearance.language) {
            restored.appearance.language = DEFAULT_LANGUAGE.into();
        }
        if check(&restored).is_err() {
            // A zone this machine doesn't know (an older time-zone database): use its own.
            restored.time.time_zone.clear();
        }
        self.save(&mut cur, restored)
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

/// Reads the text of a settings.toml. Missing fields get their defaults, so older files load.
pub fn parse(text: &str) -> Result<Settings, SettingsError> {
    toml::from_str(text).map_err(|e: toml::de::Error| {
        SettingsError::new(
            "settings-invalid",
            format!("settings.toml can't be read: {}", e.message()),
            "Fix the file by hand, or reset the settings to their defaults.",
        )
    })
}

/// What `update` refuses: values the shell could never show.
fn check(s: &Settings) -> Result<(), SettingsError> {
    if !valid_machine_name(&s.machine.name) {
        return Err(SettingsError::new(
            "invalid-machine-name",
            format!("\"{}\" can't be a machine name.", s.machine.name),
            "Use lowercase letters, digits and dashes, like \"sennep\" or \"rover-2\" (at most 63).",
        ));
    }
    let language = &s.appearance.language;
    if !crate::names::valid_language_code(language) {
        return Err(SettingsError::new(
            "invalid-language",
            format!("\"{language}\" isn't a language code."),
            "Pick a language in Settings → Appearance, or use a code like en, da or pt-BR.",
        ));
    }
    let zone = &s.time.time_zone;
    if !zone.is_empty() && jiff::tz::TimeZone::get(zone).is_err() {
        return Err(SettingsError::new(
            "invalid-time-zone",
            format!("\"{zone}\" isn't a time zone Faber knows."),
            "Pick one from the list in Settings → Date and time, or the machine's own.",
        ));
    }
    Ok(())
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
    fn a_program_is_pinned_once_at_the_end_and_unpinned() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::load(dir.path());
        let rx = s.watch();
        s.set_pinned("app:wall-follower", true).unwrap();
        s.set_pinned("app:wall-follower", true).unwrap();
        assert_eq!(s.get().taskbar.pinned.last().map(String::as_str), Some("app:wall-follower"));
        assert_eq!(s.get().taskbar.pinned.iter().filter(|k| *k == "app:wall-follower").count(), 1);
        assert_eq!(*rx.borrow(), 1, "pinning twice is one change");
        s.set_pinned("app:wall-follower", false).unwrap();
        assert!(!s.get().taskbar.pinned.iter().any(|k| k == "app:wall-follower"));
        assert_eq!(SettingsStore::load(dir.path()).get(), s.get(), "saved");
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
    fn new_fields_default_sensibly_and_old_files_still_load() {
        let d = Settings::default();
        assert_eq!(d.appearance.style, "aluminium");
        assert_eq!(d.taskbar.size, TaskbarSize::Small);
        assert_eq!(
            d.taskbar.pinned,
            ["tool:parts", "accessory:files", "accessory:notes", "tool:serial-monitor"]
        );
        let dir = tempfile::tempdir().unwrap();
        // A settings.toml from before the style and taskbar existed.
        std::fs::write(
            dir.path().join(FILE_NAME),
            "[appearance]
theme = \"dark\"
wallpaper = \"graphite\"
",
        )
        .unwrap();
        let s = SettingsStore::load(dir.path()).get();
        assert_eq!(s.appearance.theme, Theme::Dark);
        assert_eq!(s.appearance.style, "aluminium");
        assert_eq!(s.taskbar, Taskbar::default());
    }

    #[test]
    fn pins_and_taskbar_size_are_saved() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let mut s = store.get();
        s.taskbar.pinned = vec!["accessory:files".into()];
        s.taskbar.size = TaskbarSize::Large;
        s.appearance.style = "cobalt".into();
        store.update(s.clone()).unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get(), s);
    }

    #[test]
    fn accessibility_units_and_time_default_and_old_files_still_load() {
        let d = Settings::default();
        assert_eq!(d.accessibility.reduce_motion, Preference::System);
        assert_eq!(d.accessibility.higher_contrast, Preference::System);
        assert_eq!(d.accessibility.text_size, TextSize::Normal);
        assert_eq!(d.units.length, LengthUnit::Metric);
        assert_eq!(d.time.time_zone, "", "empty = the machine's own zone");
        assert_eq!(d.time.clock, ClockFormat::H24);

        // A settings.toml from before accessibility, units and time existed.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE_NAME),
            "[appearance]
theme = \"dark\"

[taskbar]
size = \"large\"
",
        )
        .unwrap();
        let s = SettingsStore::load(dir.path()).get();
        assert_eq!(s.appearance.theme, Theme::Dark);
        assert_eq!(s.taskbar.size, TaskbarSize::Large);
        assert_eq!(s.accessibility, Accessibility::default());
        assert_eq!(s.units, Units::default());
        assert_eq!(s.time, TimeSettings::default());
    }

    #[test]
    fn new_settings_are_written_in_their_documented_shape() {
        let mut s = Settings::default();
        s.accessibility.text_size = TextSize::Larger130;
        s.accessibility.reduce_motion = Preference::On;
        s.time.clock = ClockFormat::H12;
        s.time.time_zone = "Europe/Copenhagen".into();
        s.units.length = LengthUnit::Imperial;
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(
            json["accessibility"],
            serde_json::json!({"reduceMotion": "on", "higherContrast": "system", "textSize": "130"})
        );
        assert_eq!(json["units"], serde_json::json!({"length": "imperial"}));
        assert_eq!(json["time"], serde_json::json!({"timeZone": "Europe/Copenhagen", "clock": "12h"}));
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        store.update(s.clone()).unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get(), s);
    }

    #[test]
    fn a_time_zone_must_be_one_faber_knows() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let mut s = store.get();
        s.time.time_zone = "Mars/Olympus_Mons".into();
        let e = store.update(s.clone()).unwrap_err();
        assert_eq!(e.code, "invalid-time-zone");
        assert!(e.message.contains("Mars/Olympus_Mons"), "{e:?}");
        s.time.time_zone = "America/New_York".into();
        assert_eq!(store.update(s).unwrap().time.time_zone, "America/New_York");
    }

    #[test]
    fn reset_restores_every_default_and_turns_whole_machine_files_off() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let mut s = store.get();
        s.appearance.theme = Theme::Dark;
        s.units.length = LengthUnit::Imperial;
        store.update(s).unwrap();
        store
            .change_developer(DeveloperChange { full_machine_files: true, acknowledged_risk: true })
            .unwrap();
        let rx = store.watch();
        assert_eq!(store.reset().unwrap(), Settings::default());
        assert!(rx.has_changed().unwrap());
        assert_eq!(SettingsStore::load(dir.path()).get(), Settings::default());
    }

    #[test]
    fn restoring_settings_text_keeps_this_machines_developer_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let restored = "[appearance]
theme = \"dark\"

[developer]
fullMachineFiles = true
";
        let parsed = parse(restored).unwrap();
        assert!(parsed.developer.full_machine_files);
        let s = store.restore(parsed).unwrap();
        assert_eq!(s.appearance.theme, Theme::Dark);
        assert!(!s.developer.full_machine_files, "a backup never turns whole-machine files on");
        assert_eq!(parse("appearance = 5").unwrap_err().code, "settings-invalid");
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), "appearance = 5").unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get(), Settings::default());
    }

    #[test]
    fn the_machine_has_a_name_that_is_a_valid_host_name() {
        assert_eq!(Settings::default().machine.name, "faber");
        for good in ["sennep", "rover-2", "a", &"x".repeat(63)] {
            assert!(valid_machine_name(good), "{good}");
        }
        for bad in ["", "Sennep", "-a", "a-", "my robot", "æble", &"x".repeat(64)] {
            assert!(!valid_machine_name(bad), "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let mut s = store.get();
        s.machine.name = "My Robot".into();
        assert_eq!(store.update(s.clone()).unwrap_err().code, "invalid-machine-name");
        s.machine.name = "sennep".into();
        store.update(s).unwrap();
        assert_eq!(store.reset().unwrap().machine.name, "sennep", "reset keeps the name");
    }

    #[test]
    fn the_language_defaults_to_english_and_old_files_still_load() {
        assert_eq!(Settings::default().appearance.language, "en");
        let dir = tempfile::tempdir().unwrap();
        // A settings.toml from before the language setting existed.
        std::fs::write(
            dir.path().join(FILE_NAME),
            "[appearance]
theme = \"dark\"
style = \"cobalt\"
",
        )
        .unwrap();
        let store = SettingsStore::load(dir.path());
        let s = store.get();
        assert_eq!(s.appearance.style, "cobalt");
        assert_eq!(s.appearance.language, "en");
        assert_eq!(serde_json::to_value(&s).unwrap()["appearance"]["language"], "en");
    }

    #[test]
    fn the_language_must_be_a_language_code() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        let mut s = store.get();
        s.appearance.language = "../da".into();
        let e = store.update(s.clone()).unwrap_err();
        assert_eq!(e.code, "invalid-language");
        assert!(e.message.contains("../da"), "{e:?}");
        // A code without an installed translation is fine: the shell shows English for it.
        s.appearance.language = "pt-BR".into();
        assert_eq!(store.update(s).unwrap().appearance.language, "pt-BR");
        assert_eq!(SettingsStore::load(dir.path()).get().appearance.language, "pt-BR");
        let backup = "[appearance]
language = \"Dansk\"

[time]
timeZone = \"Europe/Copenhagen\"
";
        let restored = store.restore(parse(backup).unwrap()).unwrap();
        assert_eq!(restored.appearance.language, "en", "a bad code in a backup becomes English");
        assert_eq!(restored.time.time_zone, "Europe/Copenhagen", "and nothing else is lost");
    }
}
