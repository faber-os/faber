//! Layer 2: backups (D41). One `.zip` holds the machine config, the settings and the home
//! folder's own files, laid out like the data folder:
//!
//! ```text
//! config/parts.toml
//! config/settings.toml
//! files/Desktop/…
//! files/Documents/…
//! files/Recordings/…      only when asked for (recordings are big)
//! ```
//!
//! This module only packs and unpacks. Checking what is inside before anything changes, and
//! putting it back, is the machine's job.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const PARTS: &str = "config/parts.toml";
pub const SETTINGS: &str = "config/settings.toml";
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
            hint: "Pick a backup Faber made (home:/Backups/faber-backup-….zip).".into(),
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
    let config = [src.parts_toml, src.settings_toml]
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum::<u64>();
    config + home_entries(src).iter().map(|(_, _, size)| size).sum::<u64>()
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

/// `faber-backup-2026-10-15-1430.zip`: the minute it was made, in `zone` (UTC if unknown).
pub fn file_name(now: jiff::Timestamp, zone: &str) -> String {
    let tz = jiff::tz::TimeZone::get(zone).unwrap_or(jiff::tz::TimeZone::UTC);
    format!("faber-backup-{}.zip", now.to_zoned(tz).strftime("%Y-%m-%d-%H%M"))
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

/// An opened backup: its config, its settings and a list of everything else, read before
/// anything is unpacked.
pub struct Archive {
    zip: ZipArchive<File>,
    parts_toml: String,
    settings_toml: Option<String>,
    items: Vec<Item>,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Archive, BackupError> {
        let file = File::open(path).map_err(|e| BackupError::io("open the backup", &e))?;
        let mut zip = ZipArchive::new(file)
            .map_err(|e| BackupError::invalid(format!("This isn't a zip file Faber can read ({e}).")))?;
        let (mut parts_toml, mut settings_toml, mut items) = (None, None, Vec::new());
        let mut unexpected: Option<String> = None;
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| BackupError::invalid(format!("Part of the backup can't be read ({e}).")))?;
            let name = entry
                .name()
                .map_err(|e| BackupError::invalid(format!("A name in the backup can't be read ({e}).")))?
                .into_owned();
            if name == PARTS || name == SETTINGS {
                let text = read_config(&mut entry, &name)?;
                if name == PARTS {
                    parts_toml = Some(text)
                } else {
                    settings_toml = Some(text)
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
        Ok(Archive { zip, parts_toml, settings_toml, items })
    }

    pub fn parts_toml(&self) -> &str {
        &self.parts_toml
    }

    /// `None` for a backup made before any setting was changed.
    pub fn settings_toml(&self) -> Option<&str> {
        self.settings_toml.as_deref()
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Bytes the files take once unpacked.
    pub fn unpacked_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
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
            Data { dir }
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
        let src = Sources { parts_toml: &parts, settings_toml: &settings, home: &home, include_recordings };
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
        assert_eq!(a.unpacked_bytes(), 6 + 12);
    }

    #[test]
    fn recordings_only_when_asked_for() {
        let data = Data::new();
        let a = Archive::open(&make(&data, true)).unwrap();
        assert!(paths(&a).contains(&"Recordings/bench.fbcap".to_string()));
        let (parts, settings, home) = data.sources();
        let with =
            Sources { parts_toml: &parts, settings_toml: &settings, home: &home, include_recordings: true };
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
        assert_eq!(file_name(t, "Europe/Copenhagen"), "faber-backup-2026-10-15-1430.zip");
        assert_eq!(file_name(t, "UTC"), "faber-backup-2026-10-15-1230.zip");
        assert_eq!(file_name(t, "Not/AZone"), "faber-backup-2026-10-15-1230.zip", "unknown: UTC");
    }
}
