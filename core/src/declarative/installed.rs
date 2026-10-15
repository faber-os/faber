//! Declarative connectors installed on this machine: one folder per connector in
//! `<data>/connectors/`, named after its id, holding its `faber.toml` (docs/packages.md §5). The
//! package installer writes these folders; until it exists they can be copied in by hand.

use super::{FILE_NAME, definition, manifest};
use faber_connector::{Definition, Problem};
use std::path::Path;

/// A connector folder that couldn't be used, and why. The core runs without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadProblem {
    /// The folder's name, e.g. `my-sensor`.
    pub folder: String,
    pub problem: Problem,
}

/// Loads every declarative connector in `dir`. A missing `dir` means none are installed.
/// Folders of other kinds (wasm) are left to their own engine.
pub fn load_installed(dir: &Path) -> (Vec<Definition>, Vec<LoadProblem>) {
    let mut defs = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return (defs, problems) };
    let mut folders: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    folders.sort();
    for folder in folders {
        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match load_one(&folder, &name) {
            Ok(Some(d)) => {
                tracing::info!(
                    target: "core::declarative",
                    "loaded the declarative connector {} {}", d.manifest.id, d.manifest.version
                );
                defs.push(d);
            }
            Ok(None) => {}
            Err(problem) => {
                tracing::error!(
                    target: "core::declarative",
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

/// Reads one folder and hands it to the reader for its `kind`. `Ok(None)`: another engine's.
fn load_one(folder: &Path, name: &str) -> Result<Option<Definition>, Problem> {
    let text = std::fs::read_to_string(folder.join(FILE_NAME)).map_err(|e| {
        Problem::new(
            format!("The connector folder \"{name}\" has no readable faber.toml ({e})."),
            "Install the connector again, or remove the folder.",
        )
    })?;
    match manifest::package_kind(&text).as_deref() {
        Some("declarative") => {}
        Some(_) => return Ok(None),
        None => {
            // Not readable as any kind: let the declarative reader explain exactly what is wrong.
            if let Err(e) = manifest::parse(&text) {
                return Err(problem(name, &e));
            }
            return Ok(None);
        }
    }
    let d = definition(&text).map_err(|e| problem(name, &e))?;
    let id = &d.manifest.id;
    if id != name {
        return Err(Problem::new(
            format!("The connector folder \"{name}\" holds the connector \"{id}\"."),
            format!("Rename the folder to \"{id}\"."),
        ));
    }
    Ok(Some(d))
}

fn problem(name: &str, e: &manifest::ManifestError) -> Problem {
    let place = e.line.map(|l| format!(" line {l}")).unwrap_or_default();
    Problem::new(format!("connectors/{name}/faber.toml{place}: {}", e.message), e.hint.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"faber-manifest = 1
[package]
id = "my-sensor"
type = "connector"
kind = "declarative"
version = "0.1.0"
name = "My sensor"

[ports]
serial = { baud = 9600 }

[[provides]]
capability = "distance@1"
line = { pattern = "D:{meters:f32}" }
"#;

    fn folder(root: &Path, name: &str, toml: Option<&str>) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(t) = toml {
            std::fs::write(dir.join("faber.toml"), t).unwrap();
        }
    }

    #[test]
    fn no_folder_means_nothing_installed() {
        let (defs, problems) = load_installed(Path::new("/no/such/folder"));
        assert!(defs.is_empty() && problems.is_empty());
    }

    #[test]
    fn declarative_folders_load_and_other_kinds_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "my-sensor", Some(TOML));
        folder(root.path(), "a-wasm-one", Some(&TOML.replace("declarative", "wasm")));
        let (defs, problems) = load_installed(root.path());
        assert_eq!(problems, []);
        assert_eq!(defs.len(), 1);
        assert_eq!((defs[0].manifest.id.as_str(), defs[0].kind.id()), ("my-sensor", "declarative"));
    }

    #[test]
    fn bad_folders_are_explained_and_skipped() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "a-no-toml", None);
        folder(root.path(), "b-bad-toml", Some("[package"));
        folder(root.path(), "c-wrong-name", Some(TOML));
        folder(root.path(), "d-bad-line", Some(&TOML.replace("{meters:f32}", "{meters}")));
        folder(root.path(), "my-sensor", Some(TOML));
        let (defs, problems) = load_installed(root.path());
        assert_eq!(defs.len(), 1, "the good one still loads");
        let by = |f: &str| &problems.iter().find(|p| p.folder == f).unwrap().problem;
        assert!(by("a-no-toml").message.contains("no readable faber.toml"));
        assert!(
            by("b-bad-toml").message.contains("connectors/b-bad-toml/faber.toml line 1"),
            "{:?}",
            by("b-bad-toml")
        );
        assert_eq!(by("c-wrong-name").hint, "Rename the folder to \"my-sensor\".");
        let line = by("d-bad-line");
        assert!(
            line.message.contains("faber.toml line 14") && line.hint.contains("{meters:f32}"),
            "{line:?}"
        );
    }
}
