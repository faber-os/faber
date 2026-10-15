//! Layer 2: connector packages on disk (docs/packages.md §5).
//!
//! A `.faberpkg` is a zip holding `faber.toml` at its root, the connector's component (wasm),
//! `README.md`, `LICENSE`, and optionally `icon.svg` and recordings in `captures/`. This module:
//! - **reads and checks a package file** ([`open`]): sizes, paths, which files it holds, and its
//!   `[package]` header, all before anything is written;
//! - **installs it** ([`Store::prepare`], then [`Prepared::swap_in`]): unpacked into a staging
//!   folder, checked there by the loader for its kind (which the caller passes in, so this layer
//!   needs no engine), then moved to `<data>/connectors/<id>/`, keeping the previous version
//!   until the caller has seen the new one load;
//! - **remembers what is installed** in `<data>/config/packages.toml`;
//! - **writes package files** ([`pack`]) for the `faber` CLI.

use faber_connector::manifest_file::{self, PackageHeader};
use faber_connector::{Problem, is_kebab_name};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// The extension of a package file.
pub const EXTENSION: &str = "faberpkg";

/// The largest package file accepted, and the most its files may unpack to.
pub const MAX_BYTES: u64 = 50 << 20;

/// The file listing installed packages, in `<data>/config/`.
pub const LIST_FILE: &str = "packages.toml";

const MANIFEST: &str = manifest_file::FILE_NAME;
const README: &str = "README.md";
const LICENSE: &str = "LICENSE";
const ICON: &str = "icon.svg";
const CAPTURES: &str = "captures/";

/// Why a package can't be read, installed or removed, in words the user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageError {
    /// `package-too-big`, `package-invalid`, `package-not-loadable`, `not-installed` or `io-error`.
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl PackageError {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        PackageError { code, message: message.into(), hint: hint.into() }
    }

    fn invalid(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new("package-invalid", message, hint)
    }

    fn io(doing: &str, e: &io::Error) -> Self {
        Self::new(
            "io-error",
            format!("Faber couldn't {doing}: {e}"),
            "Check that the data folder is writable and the disk isn't full, then try again.",
        )
    }
}

fn too_big() -> PackageError {
    PackageError::new(
        "package-too-big",
        format!("The package is larger than {} MB, the most Faber installs.", MAX_BYTES >> 20),
        "Leave large recordings out of captures/; a few seconds of data is enough to test with.",
    )
}

fn rebuild_hint() -> &'static str {
    "Make the package again with faber pack, or ask its author for a new one."
}

/// A package file, read and checked, not installed.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    pub header: PackageHeader,
    /// `README.md`, as text.
    pub readme: String,
    /// The recordings in `captures/`, by file name.
    pub captures: Vec<String>,
    /// Every file, by its path inside the package (`faber.toml`, `captures/x.fbcap`, …).
    files: BTreeMap<String, Vec<u8>>,
}

/// Reads a package file and checks everything that can be checked without running it: its size,
/// that every path stays inside the package, which files it holds, and its `[package]` header.
pub fn open(bytes: &[u8]) -> Result<Opened, PackageError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(too_big());
    }
    let mut zip = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(|_| {
        PackageError::invalid(
            "This file isn't a Faber package: it isn't a zip file.",
            "A .faberpkg is made by faber pack. Check that you picked the right file and that it downloaded completely.",
        )
    })?;
    let mut files = BTreeMap::new();
    let mut left = MAX_BYTES;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| PackageError::invalid(format!("The package is damaged ({e})."), rebuild_hint()))?;
        let name = entry
            .name()
            .map_err(|e| {
                PackageError::invalid(format!("A name in the package can't be read ({e})."), rebuild_hint())
            })?
            .into_owned();
        check_path(&name)?;
        if entry.is_dir() {
            continue;
        }
        // Count what really comes out, not what the zip claims: a small file can unpack huge.
        let mut data = Vec::new();
        (&mut entry).take(left + 1).read_to_end(&mut data).map_err(|e| {
            PackageError::invalid(format!("{name} in the package can't be read ({e})."), rebuild_hint())
        })?;
        if data.len() as u64 > left {
            return Err(too_big());
        }
        left -= data.len() as u64;
        if files.insert(name.clone(), data).is_some() {
            return Err(PackageError::invalid(format!("The package holds {name} twice."), rebuild_hint()));
        }
    }
    check_files(files)
}

/// Checks a package that is already unpacked (path inside it → contents), e.g. a connector
/// folder from a backup, exactly as [`open`] checks a package file: sizes, paths, which files it
/// holds and its `[package]` header.
pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> Result<Opened, PackageError> {
    let mut total = 0u64;
    for (name, data) in &files {
        check_path(name)?;
        total += data.len() as u64;
    }
    if total > MAX_BYTES {
        return Err(too_big());
    }
    check_files(files)
}

/// Every path must be a plain relative path inside the package.
fn check_path(name: &str) -> Result<(), PackageError> {
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.split('/').any(|p| p == ".." || p == ".");
    if bad {
        return Err(PackageError::invalid(
            format!("The package has a file outside its own folder: \"{name}\". Faber doesn't install it."),
            "Packages may only hold plain paths like captures/sample.fbcap. Make it with faber pack.",
        ));
    }
    Ok(())
}

fn check_files(files: BTreeMap<String, Vec<u8>>) -> Result<Opened, PackageError> {
    let missing = |file: &str, why: &str| {
        PackageError::invalid(format!("The package has no {file}. {why}"), rebuild_hint())
    };
    let manifest =
        files.get(MANIFEST).ok_or_else(|| missing(MANIFEST, "Every package needs one at its root."))?;
    let manifest = std::str::from_utf8(manifest).map_err(|_| {
        PackageError::invalid("faber.toml in the package isn't text (UTF-8).", rebuild_hint())
    })?;
    let header = manifest_file::header(manifest)
        .map_err(|p| PackageError::invalid(format!("faber.toml: {}", p.message), p.hint))?;
    let readme = files.get(README).ok_or_else(|| missing(README, "It is shown before installing."))?;
    let readme = String::from_utf8(readme.clone())
        .map_err(|_| PackageError::invalid("README.md in the package isn't text (UTF-8).", rebuild_hint()))?;
    if !files.contains_key(LICENSE) {
        return Err(missing(LICENSE, "It says what you may do with the connector."));
    }
    let wasm = matches!(header.kind.as_str(), "wasm");
    if !wasm && header.kind != "declarative" {
        return Err(PackageError::invalid(
            format!("This package holds a connector of kind \"{}\", which Faber can't install.", header.kind),
            "Faber installs kind = \"wasm\" and kind = \"declarative\" connectors.",
        ));
    }
    let mut modules = 0;
    let mut captures = Vec::new();
    let mut unknown = Vec::new();
    for name in files.keys() {
        match name.as_str() {
            MANIFEST | README | LICENSE | ICON => {}
            n if n.ends_with(".wasm") && !n.contains('/') => modules += 1,
            n if n.starts_with(CAPTURES) && n.ends_with(".fbcap") && !n[CAPTURES.len()..].contains('/') => {
                captures.push(n[CAPTURES.len()..].to_string());
            }
            n => unknown.push(n.to_string()),
        }
    }
    if !unknown.is_empty() {
        return Err(PackageError::invalid(
            format!("The package holds files Faber doesn't expect: {}.", unknown.join(", ")),
            "A package holds faber.toml, README.md, LICENSE, the connector's .wasm, and optionally icon.svg and captures/*.fbcap.",
        ));
    }
    match (wasm, modules) {
        (true, 0) => return Err(missing("connector.wasm", "A wasm connector is its component.")),
        (true, 1) | (false, 0) => {}
        (true, _) => {
            return Err(PackageError::invalid("The package holds more than one .wasm file.", rebuild_hint()));
        }
        (false, _) => {
            return Err(PackageError::invalid(
                "A declarative connector is only its faber.toml, but this package also holds a .wasm file.",
                "Use kind = \"wasm\" in faber.toml if it is a wasm connector.",
            ));
        }
    }
    Ok(Opened { header, readme, captures, files })
}

/// Writes a package file from its files (path inside the package → contents). Checks it the way
/// [`open`] does, so a package that packs also opens.
pub fn pack(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, PackageError> {
    let mut out = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    // The same files give the same bytes on every OS and every day: a fixed time and fixed
    // permissions and "made by", since the zip defaults record the clock and the OS.
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644)
        .system(zip::System::Unix);
    let fail = |e: zip::result::ZipError| PackageError::io("write the package", &io::Error::other(e));
    for (name, data) in files {
        out.start_file(name.as_str(), options).map_err(fail)?;
        out.write_all(data).map_err(|e| PackageError::io("write the package", &e))?;
    }
    let bytes = out.finish().map_err(fail)?.into_inner();
    open(&bytes)?;
    Ok(bytes)
}

/// One installed package, as `packages.toml` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Installed {
    #[serde(skip)]
    pub id: String,
    pub version: String,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// When it was installed, as an RFC 3339 UTC time.
    pub installed_at: String,
    /// The package file it came from.
    pub source: String,
    /// Whether the Faber registry checked it. Always `false` until the store exists (Phase 7).
    pub checked: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct ListFile {
    #[serde(default)]
    packages: BTreeMap<String, Installed>,
}

/// Reads a `packages.toml` (this machine's, or one from a backup): the packages by id.
pub fn parse_list(text: &str) -> Result<BTreeMap<String, Installed>, PackageError> {
    let mut file: ListFile = toml::from_str(text).map_err(|e| {
        PackageError::invalid(
            format!("config/packages.toml can't be read: {}", e.message().trim()),
            "It is the list of installed packages that Faber writes.",
        )
    })?;
    for (id, p) in file.packages.iter_mut() {
        p.id = id.clone();
    }
    Ok(file.packages)
}

const LIST_HEADER: &str = "# Connector packages installed on this machine (docs/packages.md). Faber writes this file\n\
# when a package is installed or uninstalled. checked = false: not checked by the Faber registry.\n\n";

/// The packages on one machine: `<data>/connectors/<id>/` and `<data>/config/packages.toml`.
#[derive(Clone, Debug)]
pub struct Store {
    connectors: PathBuf,
    work: PathBuf,
    list: PathBuf,
}

impl Store {
    pub fn new(data_dir: &Path) -> Store {
        Store {
            connectors: data_dir.join("connectors"),
            work: data_dir.join("packages"),
            list: data_dir.join("config").join(LIST_FILE),
        }
    }

    /// Where installed connectors live; the engines load every folder in it.
    pub fn connectors_dir(&self) -> &Path {
        &self.connectors
    }

    /// Every installed package, by id.
    pub fn list(&self) -> Result<Vec<Installed>, PackageError> {
        Ok(self.read_list()?.packages.into_values().collect())
    }

    pub fn get(&self, id: &str) -> Result<Option<Installed>, PackageError> {
        Ok(self.read_list()?.packages.remove(id))
    }

    fn read_list(&self) -> Result<ListFile, PackageError> {
        let text = match std::fs::read_to_string(&self.list) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ListFile::default()),
            Err(e) => return Err(PackageError::io("read config/packages.toml", &e)),
        };
        let packages = parse_list(&text).map_err(|e| {
            PackageError::new(
                "io-error",
                e.message,
                "Fix the file by hand, or delete it: the installed connectors keep working, only the list of packages is lost.",
            )
        })?;
        Ok(ListFile { packages })
    }

    fn write_list(&self, file: &ListFile) -> Result<(), PackageError> {
        let text = toml::to_string(file)
            .map_err(|e| PackageError::io("write config/packages.toml", &io::Error::other(e)))?;
        let write = || -> io::Result<()> {
            if let Some(dir) = self.list.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = self.list.with_extension("toml.tmp");
            std::fs::write(&tmp, format!("{LIST_HEADER}{text}"))?;
            std::fs::rename(&tmp, &self.list)
        };
        write().map_err(|e| PackageError::io("write config/packages.toml", &e))
    }

    /// Unpacks `opened` into a staging folder and has `load` check it there exactly as the core
    /// will load it (for wasm: the component loads and imports only the connector world; for
    /// declarative: every rule reads). Nothing in `<data>/connectors/` changes. The staging
    /// folder is named after the id, as the loaders expect.
    pub fn prepare<T>(
        &self,
        opened: Opened,
        load: impl FnOnce(&Path) -> Result<T, Problem>,
    ) -> Result<Prepared<T>, PackageError> {
        let id = opened.header.id.clone();
        debug_assert!(is_kebab_name(&id), "the header checked the id");
        let staged = self.work.join("staging").join(&id);
        remove_dir(&staged).map_err(|e| PackageError::io("clear the staging folder", &e))?;
        let unpack = || -> io::Result<()> {
            for (name, data) in &opened.files {
                let path = staged.join(name);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(path, data)?;
            }
            Ok(())
        };
        if let Err(e) = unpack() {
            let _ = remove_dir(&staged);
            return Err(PackageError::io("unpack the package", &e));
        }
        match load(&staged) {
            Ok(loaded) => Ok(Prepared { store: self.clone(), opened, staged, loaded }),
            Err(p) => {
                let _ = remove_dir(&staged);
                Err(PackageError::new("package-not-loadable", p.message, p.hint))
            }
        }
    }

    /// Removes an installed package: its folder and its line in `packages.toml`. The caller makes
    /// sure no part uses it.
    pub fn uninstall(&self, id: &str) -> Result<Installed, PackageError> {
        let mut list = self.read_list()?;
        let Some(installed) = list.packages.remove(id) else {
            return Err(PackageError::new(
                "not-installed",
                format!("There is no installed package called \"{id}\"."),
                "Refresh the list; it may have been uninstalled already. Built-in connectors can't be uninstalled.",
            ));
        };
        let folder = self.connectors.join(id);
        let removing = self.work.join("removing").join(id);
        let gone = || -> io::Result<()> {
            remove_dir(&removing)?;
            if folder.exists() {
                std::fs::create_dir_all(removing.parent().expect("has a parent"))?;
                std::fs::rename(&folder, &removing)?;
            }
            remove_dir(&removing)
        };
        gone().map_err(|e| PackageError::io(&format!("remove connectors/{id}"), &e))?;
        self.write_list(&list)?;
        Ok(installed)
    }
}

/// A package unpacked and checked, waiting to be swapped in.
pub struct Prepared<T> {
    store: Store,
    opened: Opened,
    staged: PathBuf,
    /// What the loader made of it.
    pub loaded: T,
}

impl<T> Prepared<T> {
    pub fn header(&self) -> &PackageHeader {
        &self.opened.header
    }

    pub fn opened(&self) -> &Opened {
        &self.opened
    }

    /// Throws the staged copy away (a preview).
    pub fn discard(self) {
        let _ = remove_dir(&self.staged);
    }

    /// Moves the package into `<data>/connectors/<id>/`. The version it replaces is kept aside
    /// until [`Swapped::keep`] (the new one loaded) or [`Swapped::roll_back`].
    pub fn swap_in(self) -> Result<Swapped, PackageError> {
        let id = self.opened.header.id.clone();
        let folder = self.store.connectors.join(&id);
        let previous = self.store.work.join("previous").join(&id);
        let swap = || -> io::Result<bool> {
            std::fs::create_dir_all(&self.store.connectors)?;
            remove_dir(&previous)?;
            let had_previous = folder.exists();
            if had_previous {
                std::fs::create_dir_all(previous.parent().expect("has a parent"))?;
                std::fs::rename(&folder, &previous)?;
            }
            if let Err(e) = std::fs::rename(&self.staged, &folder) {
                if had_previous {
                    let _ = std::fs::rename(&previous, &folder);
                }
                return Err(e);
            }
            Ok(had_previous)
        };
        let had_previous = match swap() {
            Ok(h) => h,
            Err(e) => {
                let _ = remove_dir(&self.staged);
                return Err(PackageError::io(&format!("install into connectors/{id}"), &e));
            }
        };
        Ok(Swapped {
            store: self.store,
            opened: self.opened,
            folder,
            previous: had_previous.then_some(previous),
        })
    }
}

/// A package in place, with the version it replaced kept aside.
pub struct Swapped {
    store: Store,
    opened: Opened,
    folder: PathBuf,
    previous: Option<PathBuf>,
}

impl Swapped {
    /// The previous version this replaced, if any.
    pub fn replaced(&self) -> bool {
        self.previous.is_some()
    }

    /// The new version loaded: forget the old one and list the new one in `packages.toml`.
    pub fn keep(self, installed_at: &str, source: &str) -> Result<Installed, PackageError> {
        if let Some(p) = &self.previous {
            let _ = remove_dir(p);
        }
        let h = &self.opened.header;
        let installed = Installed {
            id: h.id.clone(),
            version: h.version.clone(),
            kind: h.kind.clone(),
            name: h.name.clone(),
            publisher: h.publisher.clone(),
            installed_at: installed_at.to_string(),
            source: source.to_string(),
            checked: false,
        };
        let mut list = self.store.read_list().unwrap_or_default();
        list.packages.insert(h.id.clone(), installed.clone());
        self.store.write_list(&list)?;
        Ok(installed)
    }

    /// The new version didn't load after all: put the previous one back (or nothing).
    pub fn roll_back(self) -> Result<(), PackageError> {
        let back = || -> io::Result<()> {
            remove_dir(&self.folder)?;
            if let Some(p) = &self.previous {
                std::fs::rename(p, &self.folder)?;
            }
            Ok(())
        };
        back().map_err(|e| PackageError::io("put the previous version back", &e))
    }
}

/// Removes a folder and everything in it; a missing folder is fine.
fn remove_dir(dir: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = "faber-manifest = 1\n[package]\nid = \"my-sensor\"\ntype = \"connector\"\n\
        kind = \"declarative\"\nversion = \"0.1.0\"\nname = \"My sensor\"\npublisher = \"Ada\"\n\
        [ports]\nserial = { baud = 9600 }\n[[provides]]\ncapability = \"distance@1\"\n\
        line = { pattern = \"D:{meters:f32}\" }\n";

    fn files(extra: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
        let mut f: BTreeMap<String, Vec<u8>> = [
            (MANIFEST, TOML.as_bytes()),
            (README, b"# My sensor\nReads lines.".as_slice()),
            (LICENSE, b"MIT".as_slice()),
        ]
        .into_iter()
        .map(|(n, d)| (n.to_string(), d.to_vec()))
        .collect();
        for (n, d) in extra {
            f.insert(n.to_string(), d.to_vec());
        }
        f
    }

    /// A zip with exactly these entries, written without any checks.
    fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        for (n, d) in entries {
            out.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            out.write_all(d).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn without(name: &str) -> Vec<u8> {
        let f = files(&[]);
        let entries: Vec<(&str, &[u8])> =
            f.iter().filter(|(n, _)| n.as_str() != name).map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        raw_zip(&entries)
    }

    #[test]
    fn a_packed_package_opens_with_its_header_readme_and_captures() {
        let bytes =
            pack(&files(&[("captures/walk.fbcap", b"FABERCAP1\n"), ("icon.svg", b"<svg/>")])).unwrap();
        let p = open(&bytes).unwrap();
        assert_eq!(p.header.id, "my-sensor");
        assert_eq!(p.header.publisher.as_deref(), Some("Ada"));
        assert_eq!(p.readme, "# My sensor\nReads lines.");
        assert_eq!(p.captures, ["walk.fbcap"]);
    }

    #[test]
    fn paths_outside_the_package_are_refused() {
        for name in ["../evil.txt", "/etc/passwd", "captures/../../x", "C:/x", "a\\b", "./faber.toml"] {
            let mut entries = files(&[]);
            entries.insert(name.to_string(), b"x".to_vec());
            let list: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
            let e = open(&raw_zip(&list)).unwrap_err();
            assert_eq!(e.code, "package-invalid", "{name}");
            assert!(e.message.contains("outside its own folder"), "{name}: {e:?}");
        }
    }

    #[test]
    fn too_big_is_refused_by_what_it_unpacks_to() {
        let e = open(&vec![0u8; MAX_BYTES as usize + 1]).unwrap_err();
        assert_eq!(e.code, "package-too-big");
        // Zeros compress to almost nothing, so the file is small but unpacks past the limit.
        let huge = vec![0u8; MAX_BYTES as usize + 10];
        let mut out = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(true);
        out.start_file("captures/big.fbcap", o).unwrap();
        out.write_all(&huge).unwrap();
        let bytes = out.finish().unwrap().into_inner();
        assert!((bytes.len() as u64) < MAX_BYTES);
        assert_eq!(open(&bytes).unwrap_err().code, "package-too-big");
    }

    #[test]
    fn unpacked_files_get_the_same_checks_as_a_package_file() {
        assert_eq!(from_files(files(&[])).unwrap(), open(&pack(&files(&[])).unwrap()).unwrap());
        let e = from_files(files(&[("../evil.txt", b"x")])).unwrap_err();
        assert!(e.message.contains("outside its own folder"), "{e:?}");
        let e = from_files(files(&[("run.sh", b"x")])).unwrap_err();
        assert!(e.message.contains("run.sh"), "{e:?}");
        let huge = vec![0u8; MAX_BYTES as usize + 1];
        assert_eq!(from_files(files(&[("captures/a.fbcap", &huge)])).unwrap_err().code, "package-too-big");
    }

    #[test]
    fn a_list_from_elsewhere_reads_with_its_ids() {
        let list = parse_list(
            "[packages.my-sensor]\nversion = \"0.1.0\"\nkind = \"declarative\"\n\
            name = \"My sensor\"\ninstalled-at = \"t\"\nsource = \"a.faberpkg\"\nchecked = false\n",
        )
        .unwrap();
        assert_eq!(
            (list["my-sensor"].id.as_str(), list["my-sensor"].source.as_str()),
            ("my-sensor", "a.faberpkg")
        );
        assert_eq!(parse_list("[packages").unwrap_err().code, "package-invalid");
    }

    #[test]
    fn missing_and_unexpected_files_are_explained() {
        assert!(open(b"not a zip").unwrap_err().message.contains("isn't a zip"));
        for (name, words) in [(MANIFEST, "no faber.toml"), (README, "no README.md"), (LICENSE, "no LICENSE")]
        {
            let e = open(&without(name)).unwrap_err();
            assert!(e.message.contains(words), "{e:?}");
            assert!(!e.hint.is_empty());
        }
        let e = pack(&files(&[("notes.txt", b"x")])).unwrap_err();
        assert!(e.message.contains("notes.txt"), "{e:?}");
        let e = pack(&files(&[("connector.wasm", b"x")])).unwrap_err();
        assert!(e.message.contains("only its faber.toml"), "{e:?}");
        let mut wasm = files(&[]);
        wasm.insert(MANIFEST.into(), TOML.replace("declarative", "wasm").into_bytes());
        assert!(pack(&wasm).unwrap_err().message.contains("no connector.wasm"));
        let mut native = files(&[]);
        native.insert(MANIFEST.into(), TOML.replace("declarative", "native").into_bytes());
        assert!(pack(&native).unwrap_err().message.contains("\"native\""));
        let mut bad = files(&[]);
        bad.insert(MANIFEST.into(), TOML.replace("my-sensor", "My Sensor").into_bytes());
        assert!(pack(&bad).unwrap_err().message.starts_with("faber.toml: id = \"My Sensor\""));
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        (dir, store)
    }

    fn opened(version: &str) -> Opened {
        let mut f = files(&[]);
        f.insert(MANIFEST.into(), TOML.replace("0.1.0", version).into_bytes());
        open(&pack(&f).unwrap()).unwrap()
    }

    #[test]
    fn the_loader_sees_the_staged_folder_and_its_problem_stops_the_install() {
        let (dir, store) = store();
        let e = store
            .prepare(opened("0.1.0"), |folder| -> Result<(), Problem> {
                assert!(folder.ends_with("staging/my-sensor") || folder.ends_with("staging\\my-sensor"));
                assert!(folder.join("faber.toml").exists() && folder.join("README.md").exists());
                Err(Problem::new("line 3: no", "fix it"))
            })
            .err()
            .unwrap();
        assert_eq!(
            (e.code, e.message.as_str(), e.hint.as_str()),
            ("package-not-loadable", "line 3: no", "fix it")
        );
        assert!(!dir.path().join("packages/staging/my-sensor").exists(), "cleaned up");
        assert!(!dir.path().join("connectors/my-sensor").exists(), "nothing installed");
        assert_eq!(store.list().unwrap(), []);
    }

    #[test]
    fn installing_lists_it_and_an_update_keeps_the_old_version_until_told() {
        let (dir, store) = store();
        let folder = dir.path().join("connectors/my-sensor");
        let s = store.prepare(opened("0.1.0"), |_| Ok(())).unwrap().swap_in().unwrap();
        assert!(!s.replaced());
        let first = s.keep("2026-10-15T10:00:00Z", "my-sensor-0.1.0.faberpkg").unwrap();
        assert!(folder.join("faber.toml").exists());
        assert_eq!(store.list().unwrap(), std::slice::from_ref(&first));
        assert!(!first.checked);
        let text = std::fs::read_to_string(dir.path().join("config/packages.toml")).unwrap();
        assert!(text.starts_with("# Connector packages"), "{text}");
        assert!(text.contains("[packages.my-sensor]") && text.contains("checked = false"), "{text}");

        // An update that fails to load once in place goes back to the old version.
        let s = store.prepare(opened("0.2.0"), |_| Ok(())).unwrap().swap_in().unwrap();
        assert!(s.replaced());
        assert!(std::fs::read_to_string(folder.join("faber.toml")).unwrap().contains("0.2.0"));
        s.roll_back().unwrap();
        assert!(std::fs::read_to_string(folder.join("faber.toml")).unwrap().contains("0.1.0"));
        assert_eq!(store.get("my-sensor").unwrap(), Some(first));

        let s = store.prepare(opened("0.2.0"), |_| Ok(())).unwrap().swap_in().unwrap();
        let second = s.keep("2026-10-16T10:00:00Z", "b.faberpkg").unwrap();
        assert_eq!(store.list().unwrap(), [second]);
        assert!(!dir.path().join("packages/previous/my-sensor").exists());
    }

    #[test]
    fn a_preview_leaves_nothing_behind() {
        let (dir, store) = store();
        store.prepare(opened("0.1.0"), |_| Ok(())).unwrap().discard();
        assert!(!dir.path().join("packages/staging/my-sensor").exists());
        assert!(!dir.path().join("connectors").exists());
    }

    #[test]
    fn uninstalling_removes_the_folder_and_the_line() {
        let (dir, store) = store();
        store.prepare(opened("0.1.0"), |_| Ok(())).unwrap().swap_in().unwrap().keep("t", "s").unwrap();
        let gone = store.uninstall("my-sensor").unwrap();
        assert_eq!(gone.version, "0.1.0");
        assert!(!dir.path().join("connectors/my-sensor").exists());
        assert_eq!(store.list().unwrap(), []);
        let e = store.uninstall("my-sensor").unwrap_err();
        assert_eq!(e.code, "not-installed");
    }

    #[test]
    fn a_broken_list_says_how_to_recover() {
        let (dir, store) = store();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config/packages.toml"), "[packages").unwrap();
        let e = store.list().unwrap_err();
        assert!(e.message.contains("packages.toml can't be read") && e.hint.contains("delete it"), "{e:?}");
    }

    #[test]
    fn a_package_is_the_same_bytes_on_every_os_and_every_day() {
        // CI packs on Linux what was packed and committed on Windows; the zip must not record
        // the OS or the clock.
        let files = files(&[]);
        let bytes = pack(&files).unwrap();
        let mut zip = zip::ZipArchive::new(io::Cursor::new(&bytes)).unwrap();
        for i in 0..zip.len() {
            let f = zip.by_index(i).unwrap();
            use zip::read::HasZipMetadata;
            assert_eq!(f.last_modified(), Some(zip::DateTime::default()), "entry {i}");
            assert_eq!(f.unix_mode(), Some(0o100644), "entry {i}");
            assert_eq!(f.get_metadata().system, zip::System::Unix, "entry {i}");
        }
        assert_eq!(pack(&files).unwrap(), bytes, "packing twice gives the same bytes");
    }
}
