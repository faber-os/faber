//! Wasm connectors installed on this machine: one folder per connector in `<data>/connectors/`,
//! named after its id, holding its `faber.toml` and its component (docs/packages.md §5). The
//! package installer (Phase 3) writes these folders; until then they can be copied in by hand.

use super::{Limits, definition};
use crate::connectors::LoadProblem;
use faber_connector::{Definition, Problem, manifest_file};
use std::path::Path;

/// Loads every wasm connector in `dir`. A missing `dir` means none are installed. Only folders
/// whose `[package] kind` is `wasm` are ours: others (declarative), and files with no readable
/// kind, are left to the declarative engine, which explains what is wrong with them. So no folder
/// is reported twice.
pub fn load_installed(dir: &Path, limits: Limits) -> (Vec<Definition>, Vec<LoadProblem>) {
    let mut defs = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return (defs, problems) };
    let mut folders: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    folders.sort();
    for folder in folders {
        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match load_one(&folder, &name, limits) {
            Ok(Some(d)) => {
                tracing::info!(target: "core::wasm", "loaded the wasm connector {} {}", d.manifest.id, d.manifest.version);
                defs.push(d);
            }
            Ok(None) => {}
            Err(problem) => {
                tracing::error!(
                    target: "core::wasm",
                    "the connector in {} can't be used: {} {}",
                    folder.display(),
                    problem.message,
                    problem.hint
                );
                problems.push(LoadProblem { folder: name, problem });
            }
        }
    }
    (defs, problems)
}

/// Loads the one wasm connector in `folder`, which must be named after its id: what the
/// package installer checks a package with before it installs it.
pub fn load_folder(folder: &Path, limits: Limits) -> Result<Definition, Problem> {
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    load_one(folder, &name, limits)?.ok_or_else(|| {
        Problem::new(
            format!("The connector in \"{name}\" isn't a wasm connector."),
            "Use kind = \"wasm\" in its faber.toml.",
        )
    })
}

fn load_one(folder: &Path, name: &str, limits: Limits) -> Result<Option<Definition>, Problem> {
    // A folder without a readable faber.toml is the declarative engine's to report.
    let Ok(text) = std::fs::read_to_string(folder.join(manifest_file::FILE_NAME)) else { return Ok(None) };
    if manifest_file::package_kind(&text).as_deref() != Some("wasm") {
        return Ok(None);
    }
    let package = manifest_file::parse(&text)
        .map_err(|p| Problem::new(format!("connectors/{name}/faber.toml: {}", p.message), p.hint))?;
    let id = &package.manifest.id;
    if id != name {
        return Err(Problem::new(
            format!("The connector folder \"{name}\" holds the connector \"{id}\"."),
            format!("Rename the folder to \"{id}\"."),
        ));
    }
    let module = package.module.as_deref().unwrap_or("connector.wasm");
    let wasm = std::fs::read(folder.join(module)).map_err(|e| {
        Problem::new(
            format!("The connector \"{id}\" has no readable {module} ({e})."),
            "Install the connector again; its package should contain it.",
        )
    })?;
    definition(package.manifest, &wasm, limits).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
        faber-manifest = 1

        [package]
        id = "my-sensor"
        type = "connector"
        kind = "wasm"
        version = "0.1.0"
        name = "My sensor"

        [ports]
        serial = { baud = 9600 }

        [[provides]]
        capability = "distance@1"
    "#;

    fn folder(root: &Path, name: &str, toml: Option<&str>, wasm: Option<&[u8]>) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(t) = toml {
            std::fs::write(dir.join("faber.toml"), t).unwrap();
        }
        if let Some(w) = wasm {
            std::fs::write(dir.join("connector.wasm"), w).unwrap();
        }
    }

    #[test]
    fn no_folder_means_nothing_installed() {
        let (defs, problems) = load_installed(Path::new("/no/such/folder"), Limits::default());
        assert!(defs.is_empty() && problems.is_empty());
    }

    #[test]
    fn bad_folders_are_explained_and_skipped() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "a-no-toml", None, None);
        folder(root.path(), "b-unreadable", Some("[package"), None);
        folder(root.path(), "c-wrong-name", Some(TOML), Some(&[0, 0x61, 0x73, 0x6d]));
        folder(root.path(), "d-old-format", Some(&TOML.replace("faber-manifest = 1", "")), None);
        folder(root.path(), "my-sensor", Some(TOML), Some(b"not wasm at all"));
        let other = TOML.replace("my-sensor", "no-module");
        folder(root.path(), "no-module", Some(&other), None);
        let declarative =
            TOML.replace("kind = \"wasm\"", "kind = \"declarative\"").replace("my-sensor", "lines");
        folder(root.path(), "lines", Some(&declarative), None);
        std::fs::write(root.path().join("stray-file.txt"), "x").unwrap();

        let (defs, problems) = load_installed(root.path(), Limits::default());
        assert!(defs.is_empty());
        let got: Vec<(&str, &str)> =
            problems.iter().map(|p| (p.folder.as_str(), p.problem.message.as_str())).collect();
        let expect = [
            ("c-wrong-name", "holds the connector \"my-sensor\""),
            (
                "d-old-format",
                "connectors/d-old-format/faber.toml: This faber.toml doesn't say which manifest format",
            ),
            ("my-sensor", "isn't a WebAssembly component"),
            ("no-module", "no readable connector.wasm"),
        ];
        assert_eq!(
            got.len(),
            expect.len(),
            "{got:?}; folders without a wasm kind are the declarative engine's to report"
        );
        for ((folder, message), (want_folder, words)) in got.iter().zip(expect) {
            assert_eq!(*folder, want_folder);
            assert!(message.contains(words), "{folder}: {message}");
        }
        assert!(problems.iter().all(|p| !p.problem.hint.is_empty()));
    }
}
