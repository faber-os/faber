//! Apps installed on this machine: one folder per app in `<data>/apps/`, named after its id,
//! holding its `faber.toml`, its logic's component (if it has logic) and its UI in `ui/` (if it
//! has one). The package installer writes these folders.

use super::logic::{self, Compiled};
use super::manifest::{self, AppManifest};
use crate::connectors::{LoadProblem, ShownAs};
use crate::sandbox::Limits;
use faber_connector::Problem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// An app, read and loaded: what it is, and its compiled logic.
#[derive(Clone, Debug)]
pub struct AppDefinition {
    pub manifest: AppManifest,
    pub folder: PathBuf,
    pub logic: Option<Arc<Compiled>>,
}

/// Loads every app in `dir` (`<data>/apps/`). A missing `dir` means none are installed. Never
/// fails: a folder that can't be used is logged, left out and returned as a [`LoadProblem`].
pub fn load_installed(dir: &Path, limits: Limits) -> (Vec<AppDefinition>, Vec<LoadProblem>) {
    let mut defs = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return (defs, problems) };
    let mut folders: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    folders.sort();
    for folder in folders {
        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match load_folder(&folder, &ShownAs::installed_app(&name), limits) {
            Ok(d) => {
                tracing::info!(target: "core::apps", "loaded the app {} {}", d.manifest.id, d.manifest.version);
                defs.push(d);
            }
            Err(problem) => {
                tracing::error!(
                    target: "core::apps",
                    "the app in {} can't be used: {} {}",
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

/// Loads the one app in `folder`, which must be named after its id: what the package installer
/// checks an app's package with before it installs it. Problems name its files as `shown` says.
pub fn load_folder(folder: &Path, shown: &ShownAs, limits: Limits) -> Result<AppDefinition, Problem> {
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let file = shown.file(manifest::FILE_NAME);
    let text = std::fs::read_to_string(folder.join(manifest::FILE_NAME))
        .map_err(|e| Problem::new(format!("{file} can't be read ({e})."), "Install the app again."))?;
    let manifest = manifest::parse(&text).map_err(|e| {
        let place = e.line.map(|l| format!(" line {l}")).unwrap_or_default();
        Problem::new(format!("{file}{place}: {}", e.message), e.hint)
    })?;
    let id = &manifest.id;
    if id != &name {
        return Err(Problem::new(
            format!("The app folder \"{name}\" holds the app \"{id}\"."),
            format!("Rename the folder to \"{id}\"."),
        ));
    }
    if manifest.ui && !folder.join("ui").join("index.html").is_file() {
        return Err(Problem::new(
            format!("{} has no {}.", manifest.name, shown.file("ui/index.html")),
            "An app's UI starts at ui/index.html. Add it, or remove ui = \"ui/\" from faber.toml.",
        ));
    }
    if let Some(icon) = manifest.window.as_ref().and_then(|w| w.icon.as_deref())
        && !folder.join(icon).is_file()
    {
        return Err(Problem::new(
            format!("{} has no {}, which [window] icon names.", manifest.name, shown.file(icon)),
            "Add the icon to the package, or remove the icon line.",
        ));
    }
    let logic = match &manifest.module {
        None => None,
        Some(module) => {
            let wasm = std::fs::read(folder.join(module)).map_err(|e| {
                Problem::new(
                    format!("The app \"{id}\" has no readable {} ({e}).", shown.file(module)),
                    "Make the package again with faber pack; it builds the module into it.",
                )
            })?;
            Some(Arc::new(logic::compile(&manifest.name, &wasm, limits)?))
        }
    };
    Ok(AppDefinition { manifest, folder: folder.to_path_buf(), logic })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UI_ONLY: &str = "faber-manifest = 1\n[package]\nid = \"gauges\"\ntype = \"app\"\n\
        version = \"0.1.0\"\nname = \"Gauges\"\nui = \"ui/\"\n";

    fn folder(root: &Path, name: &str, toml: &str, files: &[(&str, &[u8])]) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("faber.toml"), toml).unwrap();
        for (f, d) in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, d).unwrap();
        }
    }

    #[test]
    fn a_ui_only_app_loads_and_bad_folders_are_explained() {
        let root = tempfile::tempdir().unwrap();
        folder(root.path(), "gauges", UI_ONLY, &[("ui/index.html", b"<p>hi</p>")]);
        folder(root.path(), "a-wrong-name", UI_ONLY, &[("ui/index.html", b"x")]);
        folder(root.path(), "b-bad", &UI_ONLY.replace("gauges", "b-bad").replace("ui = ", "uix = "), &[]);
        let no_ui = UI_ONLY.replace("gauges", "c-no-ui");
        folder(root.path(), "c-no-ui", &no_ui, &[]);
        let logic = UI_ONLY.replace("gauges", "d-logic").replace("ui = \"ui/\"", "module = \"app.wasm\"");
        folder(root.path(), "d-logic", &logic, &[("app.wasm", b"not wasm")]);
        let (defs, problems) = load_installed(root.path(), Limits::default());
        assert_eq!(defs.len(), 1);
        assert!(defs[0].manifest.ui && defs[0].logic.is_none());
        let got: Vec<(&str, &str)> =
            problems.iter().map(|p| (p.folder.as_str(), p.problem.message.as_str())).collect();
        let expect = [
            ("a-wrong-name", "holds the app \"gauges\""),
            ("b-bad", "apps/b-bad/faber.toml line 7: unknown field `uix`"),
            ("c-no-ui", "no apps/c-no-ui/ui/index.html"),
            ("d-logic", "isn't a WebAssembly component"),
        ];
        assert_eq!(got.len(), expect.len(), "{got:?}");
        for ((f, m), (wf, words)) in got.iter().zip(expect) {
            assert_eq!(*f, wf);
            assert!(m.contains(words), "{f}: {m}");
        }
    }
}
