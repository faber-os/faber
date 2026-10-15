//! Layer 2: backups (D41). One `.zip` holds the machine config, the settings and the home
//! folder's own files, laid out like the data folder:
//!
//! ```text
//! config/parts.toml
//! config/settings.toml
//! config/packages.toml    the installed packages (connectors and apps)
//! config/apps.toml        which apps are on, and which parts fill their roles
//! connectors/<id>/…       each installed package's connector
//! apps/<id>/…             each installed app, with its saved state (state.toml)
//! files/Desktop/…
//! files/Documents/…
//! files/Recordings/…      only when asked for (recordings are big)
//! ```
//!
//! This module only packs and unpacks. Checking what is inside before anything changes (for a
//! connector or an app: the same checks and staging load as installing its package), and putting
//! it back, is the machine's job.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const PARTS: &str = "config/parts.toml";
pub const SETTINGS: &str = "config/settings.toml";
pub const PACKAGES: &str = "config/packages.toml";
pub const APPS_TOML: &str = "config/apps.toml";
const CONNECTORS: &str = "connectors/";
const APPS: &str = "apps/";
const FILES: &str = "files/";
/// The home folder's own folders a backup holds. Recordings only when asked for.
const FOLDERS: [&str; 3] = ["Desktop", "Documents", "Recordings"];
/// Config files are small; anything bigger isn't one.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Why a backup couldn't be made or read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl BackupError {
    fn invalid(message: impl Into<String>) -> BackupError {
        BackupError {
            code: "backup-invalid",
            message: message.into(),
            hint: "Pick a backup Faber made (in home:/Backups, named …-backup-….zip).".into(),
        }
    }

    fn io(what: &str, e: &io::Error) -> BackupError {
        BackupError {
            code: "io-error",
            message: format!("Couldn't {what}: {e}"),
            hint: "Check that the disk has space and isn't failing.".into(),
        }
    }
}

/// What goes into a backup.
pub struct Sources<'a> {
    pub parts_toml: &'a Path,
    pub settings_toml: &'a Path,
    /// The list of installed packages, copied as it is (if there is one).
    pub packages_toml: &'a Path,
    /// `<data>/connectors`, and which folders in it to back up: the installed packages.
    pub connectors: &'a Path,
    pub connector_ids: &'a [String],
    /// `config/apps.toml`, copied as it is (if there is one).
    pub apps_toml: &'a Path,
    /// `<data>/apps`, and which folders in it to back up: the installed apps.
    pub apps: &'a Path,
    pub app_ids: &'a [String],
    /// The home folder (`<data>/files`).
    pub home: &'a Path,
    pub include_recordings: bool,
}

impl Sources<'_> {
    fn folders(&self) -> impl Iterator<Item = &'static str> + '_ {
        FOLDERS.into_iter().filter(|f| self.include_recordings || *f != "Recordings")
    }
}

/// Every file under `dir` (relative, `/`-separated) and every folder (ending in `/`), with
/// sizes. Symbolic links are left out: they could point outside the home folder.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf, u64)>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let (Ok(kind), Some(name)) = (e.file_type(), e.file_name().to_str().map(str::to_string)) else {
            continue;
        };
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            out.push((format!("{rel}/"), e.path(), 0));
            walk(&e.path(), &format!("{rel}/"), out);
        } else if kind.is_file() && !name.ends_with(".faber-tmp") {
            let size = e.metadata().map_or(0, |m| m.len());
            out.push((rel, e.path(), size));
        }
    }
}

/// Every file of the connectors and apps backed up, as `connectors/<id>/…` and `apps/<id>/…`.
/// An app's folder is backed up with its saved state, but without the files Faber leaves while
/// saving it (`*.tmp`) or sets aside when it can't be read (`*.broken`).
fn package_files(src: &Sources) -> Vec<(String, PathBuf, u64)> {
    let mut out = Vec::new();
    for id in src.connector_ids {
        walk(&src.connectors.join(id), &format!("{CONNECTORS}{id}/"), &mut out);
    }
    let mut apps = Vec::new();
    for id in src.app_ids {
        walk(&src.apps.join(id), &format!("{APPS}{id}/"), &mut apps);
    }
    apps.retain(|(rel, _, _)| !rel.ends_with(".tmp") && !rel.ends_with(".broken"));
    out.extend(apps);
    out.retain(|(rel, _, _)| !rel.ends_with('/'));
    out
}

fn home_entries(src: &Sources) -> Vec<(String, PathBuf, u64)> {
    let mut out = Vec::new();
    for folder in src.folders() {
        out.push((format!("{folder}/"), src.home.join(folder), 0));
        walk(&src.home.join(folder), &format!("{folder}/"), &mut out);
    }
    out
}

/// Bytes the backup will hold before compression, to check there is room for it first.
pub fn source_bytes(src: &Sources) -> u64 {
    let config = [src.parts_toml, src.settings_toml, src.packages_toml, src.apps_toml]
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum::<u64>();
    let files = package_files(src).into_iter().chain(home_entries(src));
    config + files.map(|(_, _, size)| size).sum::<u64>()
}

/// Writes the backup to `dest` and returns its size. It is written beside `dest` first and
/// renamed into place, so a half-written backup never looks like a real one.
pub fn create(src: &Sources, dest: &Path) -> Result<u64, BackupError> {
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("backup.zip");
    let tmp = dest.with_file_name(format!(".{name}.faber-tmp"));
    let result = write_zip(src, &tmp).and_then(|()| {
        fs::rename(&tmp, dest).map_err(|e| BackupError::io("save the backup", &e))?;
        fs::metadata(dest).map(|m| m.len()).map_err(|e| BackupError::io("read the backup", &e))
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_zip(src: &Sources, out: &Path) -> Result<(), BackupError> {
    let io_err = |e: io::Error| BackupError::io("write the backup", &e);
    let zip_err = |e: zip::result::ZipError| BackupError::io("write the backup", &io::Error::other(e));
    let mut zip = ZipWriter::new(File::create(out).map_err(io_err)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let add_file = |zip: &mut ZipWriter<File>, name: &str, path: &Path, size: u64| {
        zip.start_file(name, options.large_file(size >= u64::from(u32::MAX))).map_err(zip_err)?;
        let mut f = File::open(path).map_err(io_err)?;
        io::copy(&mut f, zip).map_err(io_err)?;
        Ok::<(), BackupError>(())
    };
    let parts = fs::metadata(src.parts_toml).map_err(|e| BackupError::io("read parts.toml", &e))?;
    add_file(&mut zip, PARTS, src.parts_toml, parts.len())?;
    if let Ok(meta) = fs::metadata(src.settings_toml) {
        add_file(&mut zip, SETTINGS, src.settings_toml, meta.len())?;
    }
    if let Ok(meta) = fs::metadata(src.packages_toml) {
        add_file(&mut zip, PACKAGES, src.packages_toml, meta.len())?;
    }
    if let Ok(meta) = fs::metadata(src.apps_toml) {
        add_file(&mut zip, APPS_TOML, src.apps_toml, meta.len())?;
    }
    for (name, path, size) in package_files(src) {
        add_file(&mut zip, &name, &path, size)?;
    }
    for (rel, path, size) in home_entries(src) {
        let name = format!("{FILES}{rel}");
        if rel.ends_with('/') {
            zip.add_directory(name, options).map_err(zip_err)?;
        } else {
            add_file(&mut zip, &name, &path, size)?;
        }
    }
    let file = zip.finish().map_err(zip_err)?;
    file.sync_all().map_err(io_err)
}

/// `sennep-backup-2026-10-15-1430.zip`: the machine's name and the minute it was made, in `zone`
/// (UTC if unknown).
pub fn file_name(now: jiff::Timestamp, zone: &str, machine: &str) -> String {
    let tz = jiff::tz::TimeZone::get(zone).unwrap_or(jiff::tz::TimeZone::UTC);
    format!("{machine}-backup-{}.zip", now.to_zoned(tz).strftime("%Y-%m-%d-%H%M"))
}

/// One file or folder from the home folder, inside a backup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    index: usize,
    /// Where it goes in the home folder, e.g. `Documents/robot/notes.txt`. Only its first folder
    /// is checked here; the machine checks the whole path before using it.
    pub path: String,
    pub folder: bool,
    pub size: u64,
}

/// One file of a connector or an app inside a backup.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageFile {
    index: usize,
    /// Its path inside the package's folder, e.g. `faber.toml` or `captures/a.fbcap`.
    path: String,
    size: u64,
}

/// Which kind of package folder a backup holds: `connectors/<id>/` or `apps/<id>/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    Connectors,
    Apps,
}

impl Folder {
    fn prefix(self) -> &'static str {
        match self {
            Folder::Connectors => CONNECTORS,
            Folder::Apps => APPS,
        }
    }

    fn what(self) -> &'static str {
        match self {
            Folder::Connectors => "connector",
            Folder::Apps => "app",
        }
    }
}

/// An opened backup: its config, its settings and a list of everything else, read before
/// anything is unpacked.
pub struct Archive {
    zip: ZipArchive<File>,
    parts_toml: String,
    settings_toml: Option<String>,
    packages_toml: Option<String>,
    apps_toml: Option<String>,
    /// By the connector folder's name, as the backup has it (not checked yet).
    connectors: BTreeMap<String, Vec<PackageFile>>,
    /// By the app folder's name, as the backup has it (not checked yet).
    apps: BTreeMap<String, Vec<PackageFile>>,
    items: Vec<Item>,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Archive, BackupError> {
        let file = File::open(path).map_err(|e| BackupError::io("open the backup", &e))?;
        let mut zip = ZipArchive::new(file)
            .map_err(|e| BackupError::invalid(format!("This isn't a zip file Faber can read ({e}).")))?;
        let (mut parts_toml, mut settings_toml, mut packages_toml, mut apps_toml) = (None, None, None, None);
        let mut items = Vec::new();
        let mut connectors = BTreeMap::<String, Vec<PackageFile>>::new();
        let mut apps = BTreeMap::<String, Vec<PackageFile>>::new();
        let mut unexpected: Option<String> = None;
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| BackupError::invalid(format!("Part of the backup can't be read ({e}).")))?;
            let name = entry
                .name()
                .map_err(|e| BackupError::invalid(format!("A name in the backup can't be read ({e}).")))?
                .into_owned();
            if name == PARTS || name == SETTINGS || name == PACKAGES || name == APPS_TOML {
                let text = Some(read_config(&mut entry, &name)?);
                match name.as_str() {
                    PARTS => parts_toml = text,
                    SETTINGS => settings_toml = text,
                    APPS_TOML => apps_toml = text,
                    _ => packages_toml = text,
                }
                continue;
            }
            // connectors/<id>/<path> and apps/<id>/<path>: only listed here; the machine checks
            // each as a package.
            let package = if let Some(rest) = name.strip_prefix(CONNECTORS) {
                Some((rest, &mut connectors))
            } else {
                name.strip_prefix(APPS).map(|rest| (rest, &mut apps))
            };
            if let Some((rest, list)) = package {
                match rest.split_once('/') {
                    Some((id, path)) if !id.is_empty() => {
                        if !entry.is_dir() && !path.is_empty() {
                            let file = PackageFile { index, path: path.to_string(), size: entry.size() };
                            list.entry(id.to_string()).or_default().push(file);
                        }
                    }
                    _ => {
                        unexpected.get_or_insert(name);
                    }
                }
                continue;
            }
            let rest =
                name.strip_prefix(FILES).filter(|r| FOLDERS.iter().any(|f| r.split('/').next() == Some(*f)));
            let Some(rest) = rest else {
                unexpected.get_or_insert(name);
                continue;
            };
            let folder = entry.is_dir();
            items.push(Item {
                index,
                path: rest.trim_end_matches('/').to_string(),
                folder,
                size: if folder { 0 } else { entry.size() },
            });
        }
        let parts_toml = parts_toml.ok_or_else(|| {
            BackupError::invalid(format!("The backup has no {PARTS}, so it isn't a Faber backup."))
        })?;
        if let Some(name) = unexpected {
            return Err(BackupError::invalid(format!(
                "The backup holds {name}, which a Faber backup never does."
            )));
        }
        Ok(Archive { zip, parts_toml, settings_toml, packages_toml, apps_toml, connectors, apps, items })
    }

    pub fn parts_toml(&self) -> &str {
        &self.parts_toml
    }

    /// `None` for a backup made before any setting was changed.
    pub fn settings_toml(&self) -> Option<&str> {
        self.settings_toml.as_deref()
    }

    /// `config/packages.toml`; `None` in a backup without connectors (or one made before
    /// connectors were backed up).
    pub fn packages_toml(&self) -> Option<&str> {
        self.packages_toml.as_deref()
    }

    /// `config/apps.toml`; `None` in a backup made before apps were backed up, or on a machine
    /// that never had an app.
    pub fn apps_toml(&self) -> Option<&str> {
        self.apps_toml.as_deref()
    }

    fn folders(&self, folder: Folder) -> &BTreeMap<String, Vec<PackageFile>> {
        match folder {
            Folder::Connectors => &self.connectors,
            Folder::Apps => &self.apps,
        }
    }

    /// The connector folders in the backup, by name.
    pub fn connector_ids(&self) -> Vec<String> {
        self.package_ids(Folder::Connectors)
    }

    /// The connector (or app) folders in the backup, by name.
    pub fn package_ids(&self, folder: Folder) -> Vec<String> {
        self.folders(folder).keys().cloned().collect()
    }

    /// Every file of one connector folder (path inside it → contents), read into memory. Fails
    /// past `max_bytes` in all, counting what really comes out of the zip, not what it claims.
    pub fn connector_files(
        &mut self,
        id: &str,
        max_bytes: u64,
    ) -> Result<BTreeMap<String, Vec<u8>>, BackupError> {
        self.package_files(Folder::Connectors, id, max_bytes)
    }

    /// Every file of one connector or app folder, as [`Archive::connector_files`].
    pub fn package_files(
        &mut self,
        folder: Folder,
        id: &str,
        max_bytes: u64,
    ) -> Result<BTreeMap<String, Vec<u8>>, BackupError> {
        let files = self.folders(folder).get(id).cloned().unwrap_or_default();
        let prefix = folder.prefix();
        let unreadable = |path: &str, e: &dyn std::fmt::Display| {
            BackupError::invalid(format!("{prefix}{id}/{path} in the backup can't be read ({e})."))
        };
        let mut out = BTreeMap::new();
        let mut left = max_bytes;
        for f in files {
            let entry = self.zip.by_index(f.index).map_err(|e| unreadable(&f.path, &e))?;
            let mut data = Vec::new();
            entry.take(left + 1).read_to_end(&mut data).map_err(|e| unreadable(&f.path, &e))?;
            if data.len() as u64 > left {
                return Err(BackupError::invalid(format!(
                    "The {} \"{id}\" in the backup is larger than a package may be.",
                    folder.what()
                )));
            }
            left -= data.len() as u64;
            out.insert(f.path, data);
        }
        Ok(out)
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Bytes the files, connectors and apps take once unpacked.
    pub fn unpacked_bytes(&self) -> u64 {
        let packages =
            self.connectors.values().chain(self.apps.values()).flatten().map(|f| f.size).sum::<u64>();
        self.items.iter().map(|i| i.size).sum::<u64>() + packages
    }

    /// Writes a file item's contents to `dest` (which must not be a folder).
    pub fn extract(&mut self, item: &Item, dest: &Path) -> io::Result<()> {
        let entry = self.zip.by_index(item.index).map_err(io::Error::other)?;
        let mut out = File::create(dest)?;
        // Never more than the backup said: a damaged or crafted zip can't fill the disk.
        let copied = io::copy(&mut entry.take(item.size), &mut out)?;
        if copied != item.size {
            return Err(io::Error::other(format!("{} is shorter than the backup says", item.path)));
        }
        out.flush()
    }
}

fn read_config(entry: &mut impl Read, name: &str) -> Result<String, BackupError> {
    let mut bytes = Vec::new();
    entry
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| BackupError::invalid(format!("{name} in the backup can't be read ({e}).")))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(BackupError::invalid(format!("{name} in the backup is too big to be a config file.")));
    }
    String::from_utf8(bytes).map_err(|_| BackupError::invalid(format!("{name} in the backup isn't text.")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    struct Data {
        dir: tempfile::TempDir,
    }

    impl Data {
        fn new() -> Data {
            let dir = tempfile::tempdir().unwrap();
            let d = dir.path();
            fs::create_dir_all(d.join("config")).unwrap();
            fs::write(d.join("config/parts.toml"), "# my parts\n").unwrap();
            fs::write(d.join("config/settings.toml"), "[appearance]\ntheme = \"dark\"\n").unwrap();
            for f in ["Desktop", "Documents/robot", "Recordings", "Trash", "Backups"] {
                fs::create_dir_all(d.join("files").join(f)).unwrap();
            }
            fs::create_dir_all(d.join("files/Documents/empty")).unwrap();
            fs::write(d.join("files/Desktop/todo.txt"), "solder").unwrap();
            fs::write(d.join("files/Documents/robot/notes.txt"), "wheel torque").unwrap();
            fs::write(d.join("files/Recordings/bench.fbcap"), vec![7u8; 5000]).unwrap();
            fs::write(d.join("files/Trash/old.txt"), "gone").unwrap();
            fs::write(d.join("config/packages.toml"), "[packages.my-sensor]\n").unwrap();
            for c in ["my-sensor/captures", "not-a-package"] {
                fs::create_dir_all(d.join("connectors").join(c)).unwrap();
            }
            fs::write(d.join("connectors/my-sensor/faber.toml"), "toml").unwrap();
            fs::write(d.join("connectors/my-sensor/captures/a.fbcap"), "cap").unwrap();
            fs::write(d.join("connectors/not-a-package/faber.toml"), "toml").unwrap();
            fs::write(d.join("config/apps.toml"), "[apps.gauges]\nenabled = true\n").unwrap();
            fs::create_dir_all(d.join("apps/gauges/ui")).unwrap();
            fs::write(d.join("apps/gauges/faber.toml"), "app").unwrap();
            fs::write(d.join("apps/gauges/ui/index.html"), "<p>").unwrap();
            fs::write(d.join("apps/gauges/state.toml"), "laps = \"3\"\n").unwrap();
            fs::write(d.join("apps/gauges/state.toml.tmp"), "half-written").unwrap();
            fs::write(d.join("apps/gauges/state.toml.broken"), "laps = [").unwrap();
            Data { dir }
        }

        /// packages.toml and the connectors folder.
        fn packages(&self) -> (PathBuf, PathBuf) {
            let d = self.dir.path();
            (d.join("config/packages.toml"), d.join("connectors"))
        }

        /// apps.toml and the apps folder.
        fn apps(&self) -> (PathBuf, PathBuf) {
            let d = self.dir.path();
            (d.join("config/apps.toml"), d.join("apps"))
        }

        /// parts.toml, settings.toml and the home folder.
        fn sources(&self) -> (PathBuf, PathBuf, PathBuf) {
            let d = self.dir.path();
            (d.join("config/parts.toml"), d.join("config/settings.toml"), d.join("files"))
        }
    }

    fn make(data: &Data, include_recordings: bool) -> PathBuf {
        let (parts, settings, home) = data.sources();
        let dest = data.dir.path().join("files/Backups/b.zip");
        let (packages, connectors) = data.packages();
        let (apps_toml, apps) = data.apps();
        let ids = ["my-sensor".to_string()];
        let app_ids = ["gauges".to_string()];
        let src = Sources {
            parts_toml: &parts,
            settings_toml: &settings,
            packages_toml: &packages,
            connectors: &connectors,
            connector_ids: &ids,
            apps_toml: &apps_toml,
            apps: &apps,
            app_ids: &app_ids,
            home: &home,
            include_recordings,
        };
        let bytes = create(&src, &dest).unwrap();
        assert_eq!(bytes, fs::metadata(&dest).unwrap().len());
        dest
    }

    fn paths(a: &Archive) -> Vec<String> {
        a.items().iter().map(|i| format!("{}{}", i.path, if i.folder { "/" } else { "" })).collect()
    }

    #[test]
    fn a_backup_holds_the_config_settings_desktop_and_documents() {
        let data = Data::new();
        let zip = make(&data, false);
        let a = Archive::open(&zip).unwrap();
        assert_eq!(a.parts_toml(), "# my parts\n");
        assert_eq!(a.settings_toml(), Some("[appearance]\ntheme = \"dark\"\n"));
        let mut p = paths(&a);
        p.sort();
        assert_eq!(
            p,
            [
                "Desktop/",
                "Desktop/todo.txt",
                "Documents/",
                "Documents/empty/",
                "Documents/robot/",
                "Documents/robot/notes.txt"
            ]
        );
        assert_eq!(a.unpacked_bytes(), 6 + 12 + 4 + 3 + 3 + 3 + 11);
    }

    #[test]
    fn a_backup_holds_the_installed_apps_with_their_state_and_apps_toml() {
        let data = Data::new();
        let mut a = Archive::open(&make(&data, false)).unwrap();
        assert_eq!(a.apps_toml(), Some("[apps.gauges]\nenabled = true\n"));
        assert_eq!(a.package_ids(Folder::Apps), ["gauges"]);
        let files = a.package_files(Folder::Apps, "gauges", 100).unwrap();
        let names: Vec<_> = files.keys().map(String::as_str).collect();
        assert_eq!(names, ["faber.toml", "state.toml", "ui/index.html"], "no .tmp or .broken files");
        assert_eq!(files["state.toml"], b"laps = \"3\"\n");
        let e = a.package_files(Folder::Apps, "gauges", 5).unwrap_err();
        assert!(e.message.contains("The app \"gauges\"") && e.message.contains("larger"), "{e:?}");
        assert_eq!(a.connector_ids(), ["my-sensor"], "apps aren't connectors");
    }

    #[test]
    fn a_backup_holds_the_installed_connectors_and_their_list() {
        let data = Data::new();
        let mut a = Archive::open(&make(&data, false)).unwrap();
        assert_eq!(a.packages_toml(), Some("[packages.my-sensor]\n"));
        assert_eq!(a.connector_ids(), ["my-sensor"], "only the folders asked for");
        let files = a.connector_files("my-sensor", 100).unwrap();
        let names: Vec<_> = files.keys().map(String::as_str).collect();
        assert_eq!(names, ["captures/a.fbcap", "faber.toml"]);
        assert_eq!(files["faber.toml"], b"toml");
        let e = a.connector_files("my-sensor", 5).unwrap_err();
        assert!(e.message.contains("larger than a package"), "{e:?}");
        assert!(a.connector_files("other", 100).unwrap().is_empty());
    }

    #[test]
    fn recordings_only_when_asked_for() {
        let data = Data::new();
        let a = Archive::open(&make(&data, true)).unwrap();
        assert!(paths(&a).contains(&"Recordings/bench.fbcap".to_string()));
        let (parts, settings, home) = data.sources();
        let (packages, connectors) = data.packages();
        let (apps_toml, apps) = data.apps();
        let with = Sources {
            parts_toml: &parts,
            settings_toml: &settings,
            packages_toml: &packages,
            connectors: &connectors,
            connector_ids: &[],
            apps_toml: &apps_toml,
            apps: &apps,
            app_ids: &[],
            home: &home,
            include_recordings: true,
        };
        let without = Sources { include_recordings: false, ..with };
        assert_eq!(source_bytes(&with) - source_bytes(&without), 5000);
    }

    #[test]
    fn items_unpack_to_where_they_are_asked_to() {
        let data = Data::new();
        let mut a = Archive::open(&make(&data, false)).unwrap();
        let item = a.items().iter().find(|i| i.path == "Documents/robot/notes.txt").unwrap().clone();
        let out = data.dir.path().join("out.txt");
        a.extract(&item, &out).unwrap();
        assert_eq!(fs::read_to_string(out).unwrap(), "wheel torque");
    }

    #[test]
    fn a_backup_is_never_left_half_written() {
        let data = Data::new();
        let zip = make(&data, false);
        let backups: Vec<_> =
            fs::read_dir(zip.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(backups, ["b.zip"], "no temporary file is left behind");
    }

    #[test]
    fn something_that_isnt_a_faber_backup_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let not_zip = dir.path().join("a.zip");
        fs::write(&not_zip, "hello").unwrap();
        let e = Archive::open(&not_zip).err().unwrap();
        assert_eq!(e.code, "backup-invalid");

        let other = dir.path().join("other.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&other).unwrap());
        w.start_file("photo.jpg", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        let e = Archive::open(&other).err().unwrap();
        assert_eq!(e.code, "backup-invalid");
        assert!(e.message.contains("parts.toml"), "{e:?}");

        let odd = dir.path().join("odd.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&odd).unwrap());
        w.start_file(PARTS, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"").unwrap();
        w.start_file("files/System/x", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        let e = Archive::open(&odd).err().unwrap();
        assert_eq!(e.code, "backup-invalid");
        assert!(e.message.contains("files/System/x"), "{e:?}");
    }

    #[test]
    fn backups_are_named_after_the_minute_in_the_machines_time_zone() {
        let t: jiff::Timestamp = "2026-10-15T12:30:59Z".parse().unwrap();
        assert_eq!(file_name(t, "Europe/Copenhagen", "faber"), "faber-backup-2026-10-15-1430.zip");
        assert_eq!(file_name(t, "UTC", "sennep"), "sennep-backup-2026-10-15-1230.zip");
        assert_eq!(file_name(t, "Not/AZone", "faber"), "faber-backup-2026-10-15-1230.zip", "unknown: UTC");
    }
}
