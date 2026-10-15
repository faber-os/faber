//! Layer 2: Faber's files (docs/api.md, "Files"). The home folder (Desktop, Documents,
//! Recordings, System, Trash) and, when the developer setting allows it, the whole machine.
//!
//! The one job that matters most here: **a path can never leave its root.** Every path is
//! parsed into plain name segments (no `..`, no separators, no drive tricks), then joined onto
//! the root, and the real location is checked again after symbolic links are resolved.
//!
//! `home:/System` is virtual: it shows the machine config file, which lives in `config/`. This
//! module reads it, but never writes it: writing the config is the machine's job (validate,
//! save, reconcile).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Biggest file that can be read or written through the API.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const TRASH_INDEX: &str = ".faber-trash.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    Desktop,
    Documents,
    Recordings,
    System,
    Trash,
}

impl Special {
    pub const ALL: [Special; 5] =
        [Special::Desktop, Special::Documents, Special::Recordings, Special::System, Special::Trash];

    pub fn name(self) -> &'static str {
        match self {
            Special::Desktop => "Desktop",
            Special::Documents => "Documents",
            Special::Recordings => "Recordings",
            Special::System => "System",
            Special::Trash => "Trash",
        }
    }

    fn from_name(name: &str) -> Option<Special> {
        Special::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A validated path: a root plus plain name segments.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VPath {
    pub root: RootKey,
    pub parts: Vec<String>,
}

/// Where a path lives.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RootKey {
    /// Faber's home folder.
    Home,
    /// The whole machine (developer setting).
    Machine,
    /// A removable disk, by id without the `disk-` prefix (`disk-e:/` → `e`).
    Disk(String),
}

impl VPath {
    pub fn home(parts: &[&str]) -> VPath {
        VPath { root: RootKey::Home, parts: parts.iter().map(|s| s.to_string()).collect() }
    }

    /// The root's id as written before `:/`: `home`, `machine` or `disk-<name>`.
    pub fn root_id(&self) -> String {
        match &self.root {
            RootKey::Home => "home".into(),
            RootKey::Machine => "machine".into(),
            RootKey::Disk(name) => format!("disk-{name}"),
        }
    }

    pub fn disk(name: &str) -> VPath {
        VPath { root: RootKey::Disk(name.to_string()), parts: vec![] }
    }

    /// Parses `home:/Desktop/notes.txt`. Refuses anything that could point outside its root.
    pub fn parse(s: &str) -> Result<VPath, FileError> {
        let bad = || FileError::bad_path(s);
        let (root_token, rest) = s.split_once(':').ok_or_else(bad)?;
        let root = match root_token {
            "home" => RootKey::Home,
            "machine" => RootKey::Machine,
            t => match t.strip_prefix("disk-") {
                // Same shape as disks::disk_id makes: lowercase letters, digits, single dashes.
                Some(name)
                    if !name.is_empty()
                        && name.len() <= 32
                        && name.split('-').all(|w| {
                            !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                        }) =>
                {
                    RootKey::Disk(name.to_string())
                }
                _ => return Err(bad()),
            },
        };
        let rest = rest.strip_prefix('/').ok_or_else(bad)?;
        let mut parts = Vec::new();
        for (i, seg) in rest.split('/').enumerate() {
            if seg.is_empty() {
                // Only a trailing slash is allowed ("home:/Desktop/").
                if i + 1 == rest.split('/').count() {
                    continue;
                }
                return Err(bad());
            }
            let drive = root == RootKey::Machine && i == 0 && is_drive(seg);
            if !drive && !is_plain_name(seg) {
                return Err(bad());
            }
            parts.push(seg.to_string());
        }
        Ok(VPath { root, parts })
    }

    pub fn name(&self) -> Option<&str> {
        self.parts.last().map(String::as_str)
    }

    pub fn parent(&self) -> Option<VPath> {
        (!self.parts.is_empty())
            .then(|| VPath { root: self.root.clone(), parts: self.parts[..self.parts.len() - 1].to_vec() })
    }

    pub fn join(&self, name: &str) -> VPath {
        let mut parts = self.parts.clone();
        parts.push(name.to_string());
        VPath { root: self.root.clone(), parts }
    }

    fn special(&self) -> Option<Special> {
        match (&self.root, self.parts.as_slice()) {
            (RootKey::Home, [one]) => Special::from_name(one),
            _ => None,
        }
    }

    /// True for `home:/<special>/…` (anything inside that special folder, or the folder itself).
    fn within(&self, special: Special) -> bool {
        self.root == RootKey::Home && self.parts.first().map(String::as_str) == Some(special.name())
    }

    fn is_root(&self) -> bool {
        self.parts.is_empty()
    }
}

impl fmt::Display for VPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root = self.root_id();
        write!(f, "{root}:/{}", self.parts.join("/"))
    }
}

/// A file or folder name that can't escape: not `.`/`..`, no separators, no `:`, no control chars.
fn is_plain_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 255
        && !s.chars().any(|c| c == '/' || c == '\\' || c == ':' || c.is_control())
}

/// `C:` style drive names (the first segment of a machine path on Windows).
fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// What writing to an existing file does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    /// Overwrite it (saving in Notes).
    Replace,
    /// Refuse with `already-exists` (uploads, so nothing is overwritten by surprise).
    CreateOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: VPath,
    pub kind: Kind,
    pub size: u64,
    pub modified_ms: Option<u64>,
    pub special: Option<Special>,
    pub original_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl FileError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> FileError {
        FileError { code, message: message.into(), hint: hint.into() }
    }

    /// For callers outside this module that need a file error of their own.
    pub fn new_public(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> FileError {
        FileError::new(code, message, hint)
    }

    fn bad_path(s: &str) -> FileError {
        FileError::new(
            "bad-path",
            format!("\"{s}\" isn't a valid file path."),
            "Paths look like home:/Documents/notes.txt. Names can't contain / \\ : or be . or ..",
        )
    }

    fn not_found(p: &VPath) -> FileError {
        FileError::new(
            "not-found",
            format!("{p} doesn't exist."),
            "Refresh the folder; it may have been moved or deleted.",
        )
    }

    fn protected(p: &VPath) -> FileError {
        FileError::new(
            "protected",
            format!("{p} is one of Faber's fixed folders."),
            "Desktop, Documents, Recordings, System and Trash can't be renamed, moved or deleted.",
        )
    }

    fn io(p: &VPath, e: &io::Error) -> FileError {
        FileError::new(
            "io-error",
            format!("Couldn't use {p}: {e}"),
            "Check the disk has space and the file isn't open elsewhere.",
        )
    }
}

/// Faber's files. `home` is `<data>/files`; `config_file` is `<data>/config/parts.toml`.
pub struct Files {
    home: PathBuf,
    config_file: PathBuf,
    /// Connected removable disks: name (without `disk-`) → where they are mounted.
    disks: std::sync::RwLock<BTreeMap<String, PathBuf>>,
}

const CONFIG_NAME: &str = "parts.toml";

impl Files {
    /// Creates the home folder and its fixed folders if they're missing.
    pub fn open(home: PathBuf, config_file: PathBuf) -> io::Result<Files> {
        for s in Special::ALL.into_iter().filter(|s| *s != Special::System) {
            fs::create_dir_all(home.join(s.name()))?;
        }
        Ok(Files { home, config_file, disks: Default::default() })
    }

    pub fn home_dir(&self) -> &Path {
        &self.home
    }

    /// Replaces the set of connected disks (`disk-<name>` id → mount). Returns true if it changed.
    pub fn set_disks(&self, disks: impl IntoIterator<Item = (String, PathBuf)>) -> bool {
        let new: BTreeMap<String, PathBuf> = disks
            .into_iter()
            .filter_map(|(id, mount)| id.strip_prefix("disk-").map(|n| (n.to_string(), mount)))
            .collect();
        let mut cur = self.disks.write().expect("disks lock");
        let changed = *cur != new;
        *cur = new;
        changed
    }

    /// True if `p` is the machine config shown in the System folder.
    pub fn is_config(p: &VPath) -> bool {
        p.within(Special::System) && p.parts.len() == 2 && p.parts[1] == CONFIG_NAME
    }

    /// The real location of `p`. Checks the root is allowed, and that symbolic links don't lead
    /// out of the home folder.
    fn resolve(&self, p: &VPath, machine_allowed: bool) -> Result<PathBuf, FileError> {
        match &p.root {
            RootKey::Disk(name) => {
                let mount = self.disks.read().expect("disks lock").get(name).cloned().ok_or_else(|| {
                    FileError::new(
                        "not-found",
                        format!("The disk {} isn't connected.", p.root_id()),
                        "Plug it in again; it shows up under Disks by itself.",
                    )
                })?;
                let real = p.parts.iter().fold(mount.clone(), |acc, s| acc.join(s));
                self.check_inside(p, &real, &mount, "the disk")?;
                Ok(real)
            }
            RootKey::Machine if !machine_allowed => Err(FileError::new(
                "machine-files-off",
                "Files can only see Faber's home folder.",
                "A developer can allow the whole machine in Settings → Developer.",
            )),
            RootKey::Machine => Ok(machine_path(&p.parts)),
            RootKey::Home => {
                if p.within(Special::System) {
                    return match p.parts.len() {
                        1 => Err(FileError::new(
                            "not-a-file",
                            "System is a virtual folder.",
                            "Open parts.toml inside it.",
                        )),
                        2 if p.parts[1] == CONFIG_NAME => Ok(self.config_file.clone()),
                        _ => Err(FileError::not_found(p)),
                    };
                }
                let real = p.parts.iter().fold(self.home.clone(), |acc, s| acc.join(s));
                self.check_inside(p, &real, &self.home, "Faber's home folder")?;
                Ok(real)
            }
        }
    }

    /// Follows symbolic links on the longest existing part of `real` and refuses if that leads
    /// outside `base` (the home folder or a disk).
    fn check_inside(&self, p: &VPath, real: &Path, base: &Path, what: &str) -> Result<(), FileError> {
        let home = fs::canonicalize(base).map_err(|e| FileError::io(p, &e))?;
        let mut probe = real.to_path_buf();
        while !probe.exists() {
            if !probe.pop() {
                return Ok(());
            }
        }
        let resolved = fs::canonicalize(&probe).map_err(|e| FileError::io(p, &e))?;
        if resolved.starts_with(&home) {
            Ok(())
        } else {
            Err(FileError::new(
                "bad-path",
                format!("{p} leads outside {what}."),
                "Links that point outside it can't be followed.",
            ))
        }
    }

    pub fn list(&self, p: &VPath, machine_allowed: bool) -> Result<Vec<Entry>, FileError> {
        if p.root == RootKey::Machine && p.is_root() && machine_allowed && cfg!(windows) {
            return Ok(windows_drives(p));
        }
        if p.within(Special::System) && p.parts.len() == 1 {
            let meta = fs::metadata(&self.config_file).ok();
            return Ok(vec![Entry {
                name: CONFIG_NAME.into(),
                path: p.join(CONFIG_NAME),
                kind: Kind::File,
                size: meta.as_ref().map_or(0, |m| m.len()),
                modified_ms: meta.as_ref().and_then(modified_ms),
                special: None,
                original_path: None,
            }]);
        }
        let dir = if p.root == RootKey::Home && p.is_root() {
            self.home.clone()
        } else {
            self.resolve(p, machine_allowed)?
        };
        let read = fs::read_dir(&dir).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => FileError::not_found(p),
            _ if dir.is_file() => {
                FileError::new("not-a-folder", format!("{p} is a file."), "Open it instead.")
            }
            _ => FileError::io(p, &e),
        })?;
        let trash =
            if p.within(Special::Trash) && p.parts.len() == 1 { self.trash_index() } else { BTreeMap::new() };
        let mut entries = Vec::new();
        for e in read.flatten() {
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            if !is_plain_name(&name) || (p.within(Special::Trash) && name == TRASH_INDEX) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let path = p.join(&name);
            entries.push(Entry {
                special: path.special(),
                original_path: trash.get(&name).cloned(),
                name,
                path,
                kind: if meta.is_dir() { Kind::Folder } else { Kind::File },
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified_ms: modified_ms(&meta),
            });
        }
        if p.root == RootKey::Home && p.is_root() {
            entries.push(Entry {
                name: Special::System.name().into(),
                path: VPath::home(&["System"]),
                kind: Kind::Folder,
                size: 0,
                modified_ms: None,
                special: Some(Special::System),
                original_path: None,
            });
            entries.sort_by_key(|e| e.special.map(|s| Special::ALL.iter().position(|x| *x == s)));
            entries.sort_by_key(|e| e.special.is_none());
            return Ok(entries);
        }
        entries.sort_by(|a, b| {
            (a.kind != Kind::Folder, a.name.to_lowercase())
                .cmp(&(b.kind != Kind::Folder, b.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub fn read(&self, p: &VPath, machine_allowed: bool) -> Result<Vec<u8>, FileError> {
        let real = self.resolve(p, machine_allowed)?;
        let meta = fs::metadata(&real).map_err(|_| FileError::not_found(p))?;
        if meta.is_dir() {
            return Err(FileError::new(
                "not-a-file",
                format!("{p} is a folder."),
                "Open it in Files instead.",
            ));
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err(too_large(p));
        }
        fs::read(&real).map_err(|e| FileError::io(p, &e))
    }

    /// Creates or overwrites a file. Refuses the System folder: the machine handles the config.
    pub fn write(
        &self,
        p: &VPath,
        bytes: &[u8],
        mode: WriteMode,
        machine_allowed: bool,
    ) -> Result<(), FileError> {
        if p.within(Special::System) {
            return Err(FileError::new(
                "protected",
                "Only parts.toml lives in System, and it is saved through the machine config.",
                "Save other files in Documents or Desktop.",
            ));
        }
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(too_large(p));
        }
        if p.is_root()
            || p.special().is_some()
            || p.parent().is_some_and(|parent| parent.is_root() && p.root == RootKey::Home)
        {
            return Err(FileError::new(
                "protected",
                "Files can't be saved directly in the home folder.",
                "Save in Desktop, Documents or Recordings.",
            ));
        }
        let real = self.resolve(p, machine_allowed)?;
        if real.is_dir() {
            return Err(FileError::new("not-a-file", format!("{p} is a folder."), "Pick another name."));
        }
        let parent = real.parent().ok_or_else(|| FileError::not_found(p))?;
        if !parent.is_dir() {
            return Err(FileError::not_found(&p.parent().unwrap_or_else(|| p.clone())));
        }
        match mode {
            // Written beside the target and renamed over it, so a reader never sees half a file.
            WriteMode::Replace => {
                let tmp = parent.join(format!(".{}.faber-tmp", p.name().unwrap_or("file")));
                fs::write(&tmp, bytes)
                    .and_then(|()| fs::rename(&tmp, &real))
                    .map_err(|e| FileError::io(p, &e))
            }
            // `create_new` fails if the file exists, even if it appeared a moment ago.
            WriteMode::CreateOnly => {
                use std::io::Write;
                let mut f = fs::OpenOptions::new().write(true).create_new(true).open(&real).map_err(|e| {
                    if e.kind() == io::ErrorKind::AlreadyExists {
                        already_exists(p)
                    } else {
                        FileError::io(p, &e)
                    }
                })?;
                f.write_all(bytes).map_err(|e| {
                    let _ = fs::remove_file(&real);
                    FileError::io(p, &e)
                })
            }
        }
    }

    pub fn create_folder(&self, p: &VPath, machine_allowed: bool) -> Result<(), FileError> {
        self.refuse_fixed(p)?;
        if p.within(Special::System) {
            return Err(FileError::protected(&VPath::home(&["System"])));
        }
        let real = self.resolve(p, machine_allowed)?;
        if real.exists() {
            return Err(already_exists(p));
        }
        fs::create_dir(&real).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => FileError::not_found(&p.parent().unwrap_or_else(|| p.clone())),
            _ => FileError::io(p, &e),
        })
    }

    /// Renames or moves within one root. Never overwrites.
    pub fn rename(&self, from: &VPath, to: &VPath, machine_allowed: bool) -> Result<(), FileError> {
        self.refuse_fixed(from)?;
        self.refuse_fixed(to)?;
        if from.root != to.root {
            return Err(FileError::new(
                "bad-path",
                "Files can only be moved within one place.",
                "Download and upload to copy between places.",
            ));
        }
        if from.within(Special::System) || to.within(Special::System) {
            return Err(FileError::protected(&VPath::home(&["System"])));
        }
        if to.parts.starts_with(&from.parts) {
            return Err(FileError::new(
                "bad-path",
                "A folder can't be moved into itself.",
                "Pick a folder outside it.",
            ));
        }
        let (src, dst) = (self.resolve(from, machine_allowed)?, self.resolve(to, machine_allowed)?);
        if !src.exists() {
            return Err(FileError::not_found(from));
        }
        if dst.exists() {
            return Err(already_exists(to));
        }
        fs::rename(&src, &dst).map_err(|e| FileError::io(from, &e))
    }

    /// Moves home files into the Trash and remembers where they came from.
    pub fn trash(&self, paths: &[VPath]) -> Result<(), FileError> {
        let mut index = self.trash_index();
        for p in paths {
            if p.root != RootKey::Home {
                return Err(FileError::new(
                    "no-trash",
                    format!("{p} is outside the home folder, which has no Trash."),
                    "Delete it for good instead, if you're sure.",
                ));
            }
            self.refuse_fixed(p)?;
            if p.within(Special::System) || p.within(Special::Trash) {
                return Err(FileError::new(
                    "protected",
                    format!("{p} can't be moved to the Trash."),
                    "Files in the Trash can be deleted for good.",
                ));
            }
            let src = self.resolve(p, false)?;
            if !src.exists() {
                return Err(FileError::not_found(p));
            }
            let name = unique_name(&self.home.join(Special::Trash.name()), p.name().unwrap_or("item"));
            fs::rename(&src, self.home.join(Special::Trash.name()).join(&name))
                .map_err(|e| FileError::io(p, &e))?;
            index.insert(name, p.to_string());
        }
        self.save_trash_index(&index, &paths[0])
    }

    /// Puts Trash items back where they came from (or into Documents if that's gone or taken).
    pub fn restore(&self, paths: &[VPath]) -> Result<(), FileError> {
        let mut index = self.trash_index();
        for p in paths {
            if !(p.within(Special::Trash) && p.parts.len() == 2) {
                return Err(FileError::new(
                    "bad-path",
                    format!("{p} isn't in the Trash."),
                    "Only items in the Trash can be restored.",
                ));
            }
            let name = &p.parts[1];
            let src = self.resolve(p, false)?;
            if !src.exists() {
                return Err(FileError::not_found(p));
            }
            let original = index.get(name).and_then(|o| VPath::parse(o).ok());
            let target = match original {
                Some(o)
                    if o.parent()
                        .and_then(|par| self.resolve(&par, false).ok())
                        .is_some_and(|d| d.is_dir())
                        && self.resolve(&o, false).is_ok_and(|r| !r.exists()) =>
                {
                    o
                }
                _ => {
                    let docs = self.home.join(Special::Documents.name());
                    VPath::home(&[
                        "Documents",
                        &unique_name(&docs, original.as_ref().and_then(|o| o.name()).unwrap_or(name)),
                    ])
                }
            };
            fs::rename(&src, self.resolve(&target, false)?).map_err(|e| FileError::io(p, &e))?;
            index.remove(name);
        }
        self.save_trash_index(&index, &paths[0])
    }

    /// Deletes for good. Only Trash items, or machine paths (no Trash there).
    pub fn delete(&self, paths: &[VPath], machine_allowed: bool) -> Result<(), FileError> {
        let mut index = self.trash_index();
        for p in paths {
            let in_trash = p.within(Special::Trash) && p.parts.len() >= 2;
            if p.root == RootKey::Home && !in_trash {
                return Err(FileError::new(
                    "protected",
                    format!("{p} isn't in the Trash."),
                    "Move it to the Trash first; delete it from there if you're sure.",
                ));
            }
            if matches!(p.root, RootKey::Disk(_)) && p.parts.is_empty() {
                return Err(FileError::new(
                    "protected",
                    format!("{p} is a whole disk."),
                    "Delete what is on it instead.",
                ));
            }
            if p.root == RootKey::Machine && p.parts.len() < 2 {
                return Err(FileError::new(
                    "protected",
                    format!("{p} is a drive or top-level folder."),
                    "Faber won't delete those.",
                ));
            }
            let real = self.resolve(p, machine_allowed)?;
            let meta = fs::symlink_metadata(&real).map_err(|_| FileError::not_found(p))?;
            let result = if meta.is_dir() { fs::remove_dir_all(&real) } else { fs::remove_file(&real) };
            result.map_err(|e| FileError::io(p, &e))?;
            if in_trash && p.parts.len() == 2 {
                index.remove(&p.parts[1]);
            }
        }
        match paths.first() {
            Some(first) if first.root == RootKey::Home => self.save_trash_index(&index, first),
            _ => Ok(()),
        }
    }

    pub fn empty_trash(&self) -> Result<(), FileError> {
        let trash = VPath::home(&["Trash"]);
        let items: Vec<VPath> = self.list(&trash, false)?.into_iter().map(|e| e.path).collect();
        if items.is_empty() {
            return Ok(());
        }
        self.delete(&items, false)
    }

    fn refuse_fixed(&self, p: &VPath) -> Result<(), FileError> {
        if p.is_root() || p.special().is_some() || (p.root == RootKey::Machine && p.parts.len() < 2) {
            return Err(FileError::protected(p));
        }
        Ok(())
    }

    fn trash_index(&self) -> BTreeMap<String, String> {
        fs::read_to_string(self.home.join(Special::Trash.name()).join(TRASH_INDEX))
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_trash_index(&self, index: &BTreeMap<String, String>, p: &VPath) -> Result<(), FileError> {
        let text = toml::to_string(index).expect("a string map serialises");
        fs::write(self.home.join(Special::Trash.name()).join(TRASH_INDEX), text)
            .map_err(|e| FileError::io(p, &e))
    }
}

fn too_large(p: &VPath) -> FileError {
    FileError::new(
        "too-large",
        format!("{p} is bigger than 16 MB."),
        "Faber's files handle files up to 16 MB.",
    )
}

fn already_exists(p: &VPath) -> FileError {
    FileError::new("already-exists", format!("{p} already exists."), "Pick another name.")
}

fn modified_ms(meta: &fs::Metadata) -> Option<u64> {
    meta.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// `name`, or `name (2)`, `name (3)`… — the first that doesn't exist in `dir`.
fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..).map(|n| format!("{stem} ({n}){ext}")).find(|c| !dir.join(c).exists()).expect("some name is free")
}

/// The real path for machine segments: `C:` + rest on Windows, `/` + rest elsewhere.
fn machine_path(parts: &[String]) -> PathBuf {
    let mut p = match parts.first() {
        Some(d) if is_drive(d) => PathBuf::from(format!("{d}\\")),
        _ => PathBuf::from("/"),
    };
    for s in parts.iter().skip(usize::from(parts.first().is_some_and(|d| is_drive(d)))) {
        p.push(s);
    }
    debug_assert!(p.components().all(|c| !matches!(c, Component::ParentDir)));
    p
}

fn windows_drives(root: &VPath) -> Vec<Entry> {
    (b'A'..=b'Z')
        .map(|l| format!("{}:", l as char))
        .filter(|d| Path::new(&format!("{d}\\")).exists())
        .map(|d| Entry {
            path: root.join(&d),
            name: d,
            kind: Kind::Folder,
            size: 0,
            modified_ms: None,
            special: None,
            original_path: None,
        })
        .collect()
}

/// When this file was written, for tests that need "now".
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Files) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config/parts.toml"), "# parts\n").unwrap();
        let files = Files::open(dir.path().join("files"), dir.path().join("config/parts.toml")).unwrap();
        (dir, files)
    }

    fn p(s: &str) -> VPath {
        VPath::parse(s).unwrap()
    }

    #[test]
    fn paths_parse_and_print() {
        assert_eq!(p("home:/Desktop/notes.txt").parts, ["Desktop", "notes.txt"]);
        assert_eq!(p("home:/").to_string(), "home:/");
        assert_eq!(p("home:/Documents/").to_string(), "home:/Documents");
        assert_eq!(p("machine:/C:/Users").parts, ["C:", "Users"]);
    }

    #[test]
    fn paths_that_could_escape_are_refused() {
        for bad in [
            "home:/../secret",
            "home:/Desktop/../../x",
            "home:/Desktop/./x",
            "home:/Desktop//x",
            "home:/a\\b",
            "home:/C:/x",
            "home:Desktop",
            "/Desktop",
            "home:/Desktop/x:y",
            "machine:/Users/C:",
            "other:/x",
        ] {
            assert_eq!(VPath::parse(bad).unwrap_err().code, "bad-path", "{bad}");
        }
    }

    #[test]
    fn home_lists_the_fixed_folders_in_order() {
        let (_d, f) = setup();
        let names: Vec<_> = f.list(&p("home:/"), false).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Desktop", "Documents", "Recordings", "System", "Trash"]);
    }

    #[test]
    fn system_shows_the_config_and_reads_it() {
        let (_d, f) = setup();
        let entries = f.list(&p("home:/System"), false).unwrap();
        assert_eq!(entries[0].path.to_string(), "home:/System/parts.toml");
        assert_eq!(f.read(&p("home:/System/parts.toml"), false).unwrap(), b"# parts\n");
        assert!(Files::is_config(&p("home:/System/parts.toml")));
        assert_eq!(
            f.write(&p("home:/System/parts.toml"), b"x", WriteMode::Replace, false).unwrap_err().code,
            "protected"
        );
        assert_eq!(
            f.write(&p("home:/System/other.txt"), b"x", WriteMode::Replace, false).unwrap_err().code,
            "protected"
        );
    }

    #[test]
    fn write_read_rename_and_folders() {
        let (_d, f) = setup();
        f.write(&p("home:/Documents/a.txt"), b"hello", WriteMode::Replace, false).unwrap();
        assert_eq!(f.read(&p("home:/Documents/a.txt"), false).unwrap(), b"hello");
        f.create_folder(&p("home:/Documents/notes"), false).unwrap();
        assert_eq!(f.create_folder(&p("home:/Documents/notes"), false).unwrap_err().code, "already-exists");
        f.rename(&p("home:/Documents/a.txt"), &p("home:/Documents/notes/b.txt"), false).unwrap();
        let listed = f.list(&p("home:/Documents/notes"), false).unwrap();
        assert_eq!(listed[0].name, "b.txt");
        assert_eq!(listed[0].size, 5);
        assert!(listed[0].modified_ms.is_some());
        assert_eq!(
            f.write(&p("home:/Nowhere/x.txt"), b"x", WriteMode::Replace, false).unwrap_err().code,
            "not-found"
        );
        assert_eq!(
            f.write(&p("home:/loose.txt"), b"x", WriteMode::Replace, false).unwrap_err().code,
            "protected"
        );
    }

    #[test]
    fn create_only_never_overwrites() {
        let (_d, f) = setup();
        f.write(&p("home:/Documents/a.txt"), b"first", WriteMode::CreateOnly, false).unwrap();
        let e = f.write(&p("home:/Documents/a.txt"), b"second", WriteMode::CreateOnly, false).unwrap_err();
        assert_eq!(e.code, "already-exists");
        assert_eq!(f.read(&p("home:/Documents/a.txt"), false).unwrap(), b"first");
        f.write(&p("home:/Documents/a.txt"), b"second", WriteMode::Replace, false).unwrap();
        assert_eq!(f.read(&p("home:/Documents/a.txt"), false).unwrap(), b"second");
    }

    #[test]
    fn folders_come_first() {
        let (_d, f) = setup();
        f.write(&p("home:/Desktop/a.txt"), b"", WriteMode::Replace, false).unwrap();
        f.create_folder(&p("home:/Desktop/z"), false).unwrap();
        let names: Vec<_> = f.list(&p("home:/Desktop"), false).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["z", "a.txt"]);
    }

    #[test]
    fn fixed_folders_are_protected() {
        let (_d, f) = setup();
        assert_eq!(
            f.rename(&p("home:/Desktop"), &p("home:/Documents/x"), false).unwrap_err().code,
            "protected"
        );
        assert_eq!(f.trash(&[p("home:/Recordings")]).unwrap_err().code, "protected");
        assert_eq!(f.create_folder(&p("home:/Trash"), false).unwrap_err().code, "protected");
        assert_eq!(
            f.rename(&p("home:/Documents/x"), &p("home:/Documents/x/y"), false).unwrap_err().code,
            "bad-path"
        );
    }

    #[test]
    fn trash_restore_and_delete() {
        let (_d, f) = setup();
        f.write(&p("home:/Desktop/a.txt"), b"1", WriteMode::Replace, false).unwrap();
        f.trash(&[p("home:/Desktop/a.txt")]).unwrap();
        let trash = f.list(&p("home:/Trash"), false).unwrap();
        assert_eq!(trash.len(), 1, "the index file is hidden");
        assert_eq!(trash[0].original_path.as_deref(), Some("home:/Desktop/a.txt"));

        // A second a.txt in the Trash gets a new name.
        f.write(&p("home:/Desktop/a.txt"), b"2", WriteMode::Replace, false).unwrap();
        f.trash(&[p("home:/Desktop/a.txt")]).unwrap();
        let names: Vec<_> = f.list(&p("home:/Trash"), false).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["a (2).txt", "a.txt"]);

        f.restore(&[p("home:/Trash/a.txt")]).unwrap();
        assert_eq!(f.read(&p("home:/Desktop/a.txt"), false).unwrap(), b"1");
        // The original spot is taken now, so the second one goes to Documents.
        f.restore(&[p("home:/Trash/a (2).txt")]).unwrap();
        assert_eq!(f.read(&p("home:/Documents/a.txt"), false).unwrap(), b"2");

        assert_eq!(
            f.delete(&[p("home:/Desktop/a.txt")], false).unwrap_err().code,
            "protected",
            "only from the Trash"
        );
        f.trash(&[p("home:/Desktop/a.txt")]).unwrap();
        f.empty_trash().unwrap();
        assert!(f.list(&p("home:/Trash"), false).unwrap().is_empty());
    }

    #[test]
    fn the_machine_is_off_unless_allowed() {
        let (_d, f) = setup();
        assert_eq!(f.list(&p("machine:/"), false).unwrap_err().code, "machine-files-off");
        assert!(f.list(&p("machine:/"), true).is_ok_and(|e| !e.is_empty()));
    }

    #[test]
    fn size_limit() {
        let (_d, f) = setup();
        let big = vec![0u8; (MAX_FILE_BYTES + 1) as usize];
        assert_eq!(
            f.write(&p("home:/Documents/big.bin"), &big, WriteMode::Replace, false).unwrap_err().code,
            "too-large"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_out_of_home_are_refused() {
        let (d, f) = setup();
        std::os::unix::fs::symlink(d.path().join("config"), d.path().join("files/Documents/escape")).unwrap();
        assert_eq!(f.read(&p("home:/Documents/escape/parts.toml"), false).unwrap_err().code, "bad-path");
    }

    #[test]
    fn unique_names() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(unique_name(d.path(), "a.txt"), "a.txt");
        std::fs::write(d.path().join("a.txt"), "").unwrap();
        std::fs::write(d.path().join("a (2).txt"), "").unwrap();
        assert_eq!(unique_name(d.path(), "a.txt"), "a (3).txt");
        std::fs::create_dir(d.path().join("box")).unwrap();
        assert_eq!(unique_name(d.path(), "box"), "box (2)");
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    fn p(s: &str) -> VPath {
        VPath::parse(s).unwrap()
    }

    #[test]
    fn disk_paths_parse_and_print() {
        assert_eq!(p("disk-e:/photos/a.jpg").root, RootKey::Disk("e".into()));
        assert_eq!(p("disk-sennep-sd:/").to_string(), "disk-sennep-sd:/");
        for bad in ["disk-:/x", "disk-E:/x", "disk-a--b:/x", "disk-a_b:/x", "disk-e:/../x"] {
            assert_eq!(VPath::parse(bad).unwrap_err().code, "bad-path", "{bad}");
        }
    }

    #[test]
    fn a_connected_disk_works_like_a_folder_and_has_no_trash() {
        let d = tempfile::tempdir().unwrap();
        let stick = tempfile::tempdir().unwrap();
        std::fs::write(stick.path().join("readme.txt"), "hi").unwrap();
        let f = Files::open(d.path().join("files"), d.path().join("parts.toml")).unwrap();
        assert_eq!(f.read(&p("disk-e:/readme.txt"), false).unwrap_err().code, "not-found");
        assert!(f.set_disks([("disk-e".to_string(), stick.path().to_path_buf())]));
        assert!(!f.set_disks([("disk-e".to_string(), stick.path().to_path_buf())]), "unchanged");
        assert_eq!(f.list(&p("disk-e:/"), false).unwrap()[0].name, "readme.txt");
        f.write(&p("disk-e:/new.txt"), b"x", WriteMode::CreateOnly, false).unwrap();
        assert_eq!(f.trash(&[p("disk-e:/new.txt")]).unwrap_err().code, "no-trash");
        f.delete(&[p("disk-e:/new.txt")], false).unwrap();
        assert_eq!(f.delete(&[p("disk-e:/")], false).unwrap_err().code, "protected");
        assert_eq!(
            f.rename(&p("disk-e:/readme.txt"), &p("home:/Documents/r.txt"), false).unwrap_err().code,
            "bad-path"
        );
        f.set_disks([]);
        assert_eq!(f.list(&p("disk-e:/"), false).unwrap_err().code, "not-found");
    }
}
