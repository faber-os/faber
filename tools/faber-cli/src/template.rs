//! `faber new connector <name>`: a ready project from `templates/connector-rust` (a wasm
//! connector in Rust) or `templates/connector-declarative` (only a `faber.toml`). Both build,
//! check and pass their tests against a recorded capture before you change a line.
//! `faber new app <name>`: a ready app from `templates/app-rust` (logic in Rust, and a UI).

use crate::CliError;
use faber_connector::capture::{self, Chunk};
use faber_connector::is_kebab_name;
use std::path::{Path, PathBuf};

/// Which kind of connector to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Rust, built as a sandboxed WebAssembly component.
    Wasm,
    /// Only a `faber.toml`, run by the core's engine.
    Declarative,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Kind> {
        match text {
            "wasm" => Some(Kind::Wasm),
            "declarative" => Some(Kind::Declarative),
            _ => None,
        }
    }
}

/// The template's own names, replaced by the new connector's.
const ID: &str = "my-sensor";
const CRATE: &str = "my_sensor";
const NAME: &str = "My sensor";
/// The template's path to the SDK, replaced by the SDK in the Faber checkout this tool was
/// built from. Once the SDK is on crates.io, the template uses version numbers instead.
const SDK_PATH: &str = "../../sdk/rust/";

macro_rules! template {
    ($dir:literal: $($file:literal),* $(,)?) => {
        &[$(($file, include_str!(concat!("../../../templates/", $dir, "/", $file)))),*]
    };
}

const RUST: &[(&str, &str)] = template!("connector-rust":
    "Cargo.toml", "faber.toml", "README.md", "LICENSE", ".gitignore", "src/lib.rs", "tests/capture.rs",
);
const DECLARATIVE: &[(&str, &str)] =
    template!("connector-declarative": "faber.toml", "README.md", "LICENSE", ".gitignore");

const APP: &[(&str, &str)] = template!("app-rust":
    "Cargo.toml", "faber.toml", "README.md", "LICENSE", ".gitignore", "src/lib.rs",
    "ui/index.html", "ui/app.js", "ui/style.css",
);
/// The app template's own names.
const APP_ID: &str = "my-app";
const APP_CRATE: &str = "my_app";
const APP_NAME: &str = "My app";

/// Where the recording both templates are tested against goes.
pub const SAMPLE: &str = "captures/sample.fbcap";

/// What the sample device printed: five distances, the way an Arduino sends them, with lines
/// split across reads like a real serial port does.
pub fn sample_capture() -> Vec<u8> {
    let chunks = ["D:12", "3\r\nD:118\r\n", "D:9", "5\r\nD:42\r\nD:4", "0\r\n"];
    let chunks: Vec<Chunk> = chunks
        .iter()
        .enumerate()
        .map(|(i, text)| Chunk { t_us: 1_000_000 + i as u64 * 100_000, bytes: text.as_bytes().to_vec() })
        .collect();
    capture::encode(&chunks)
}

/// "acme-ranger" → "Acme ranger".
fn display_name(id: &str) -> String {
    let words = id.replace('-', " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The `sdk/rust/` folder of the Faber checkout this tool was built from, with `/` separators so
/// it works in Cargo.toml on every OS.
pub fn sdk_dir() -> String {
    let tools = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = tools.parent().and_then(Path::parent).expect("tools/faber-cli is two folders down");
    format!("{}/", root.join("sdk").join("rust").display()).replace('\\', "/")
}

/// Writes a new connector project called `name` into `parent/name`, and returns its folder.
pub fn new_connector(name: &str, kind: Kind, parent: &Path) -> Result<PathBuf, CliError> {
    let files = match kind {
        Kind::Wasm => RUST,
        Kind::Declarative => DECLARATIVE,
    };
    let dir = write_project(name, "connector", "acme-ranger", parent, files, (ID, CRATE, NAME))?;
    write_file(&dir, SAMPLE, &sample_capture())?;
    Ok(dir)
}

/// Writes a new app project called `name` into `parent/name`, and returns its folder.
/// Its window is built on `@faber-os/shell`: the SDK this `faber` was built with is copied in as
/// `ui/faber-shell.js`, as it is (an app's page can only load its own files).
pub fn new_app(name: &str, parent: &Path) -> Result<PathBuf, CliError> {
    let dir = write_project(name, "app", "wall-follower", parent, APP, (APP_ID, APP_CRATE, APP_NAME))?;
    write_file(&dir, &format!("ui/{}", crate::ui::SDK_FILE), crate::ui::SDK.as_bytes())?;
    Ok(dir)
}

fn write_file(dir: &Path, rel: &str, bytes: &[u8]) -> Result<(), CliError> {
    let path = dir.join(rel);
    let made = path
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| std::fs::write(&path, bytes));
    made.map_err(|e| {
        CliError::new(
            format!("{} can't be written: {e}", path.display()),
            "Check that you may write to this folder.",
        )
    })
}

/// Copies a template into `parent/name`, with the template's own id, crate name and display
/// name (`names`) replaced by the new project's, and the SDK paths by this checkout's.
fn write_project(
    name: &str,
    what: &str,
    example: &str,
    parent: &Path,
    files: &[(&str, &str)],
    names: (&str, &str, &str),
) -> Result<PathBuf, CliError> {
    if !is_kebab_name(name) {
        return Err(CliError::new(
            format!("\"{name}\" can't be a{} {what}'s name.", if what.starts_with('a') { "n" } else { "" }),
            format!(
                "Use lowercase words joined by -, starting with a letter, like {example}. It becomes the {what}'s id."
            ),
        ));
    }
    let dir = parent.join(name);
    if dir.exists() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(CliError::new(
            format!("The folder {} already exists and isn't empty.", dir.display()),
            "Pick another name, or remove that folder first.",
        ));
    }
    let (id, crate_id, display_id) = names;
    let crate_name = name.replace('-', "_");
    let display = display_name(name);
    let sdk = sdk_dir();
    for (rel, text) in files {
        let text = text
            .replace(id, name)
            .replace(crate_id, &crate_name)
            .replace(display_id, &display)
            .replace(SDK_PATH, &sdk);
        write_file(&dir, rel, text.as_bytes())?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_ids_crates_and_display_names() {
        assert_eq!(display_name("acme-ranger"), "Acme ranger");
        let dir = tempfile::tempdir().unwrap();
        let project = new_connector("acme-ranger", Kind::Wasm, dir.path()).unwrap();
        let toml = std::fs::read_to_string(project.join("faber.toml")).unwrap();
        assert!(toml.contains("id = \"acme-ranger\"") && toml.contains("name = \"Acme ranger\""), "{toml}");
        let cargo = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"acme-ranger\""), "{cargo}");
        assert!(cargo.contains(&format!("path = \"{}connector\"", sdk_dir())), "{cargo}");
        assert!(!cargo.contains("../../sdk"), "{cargo}");
        let test = std::fs::read_to_string(project.join("tests/capture.rs")).unwrap();
        assert!(test.contains("acme_ranger::definition()"), "{test}");
        assert_eq!(std::fs::read(project.join(SAMPLE)).unwrap(), sample_capture());
        assert!(project.join(".gitignore").exists());
        assert!(Path::new(&sdk_dir()).join("connector/Cargo.toml").exists(), "{}", sdk_dir());
    }

    #[test]
    fn an_app_gets_its_id_crate_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let project = new_app("nearest-wall", dir.path()).unwrap();
        let toml = std::fs::read_to_string(project.join("faber.toml")).unwrap();
        assert!(toml.contains("id = \"nearest-wall\"") && toml.contains("name = \"Nearest wall\""), "{toml}");
        assert!(toml.contains("type = \"app\""), "{toml}");
        let cargo = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
        assert!(cargo.contains(&format!("path = \"{}app-wasm\"", sdk_dir())), "{cargo}");
        let ui = std::fs::read_to_string(project.join("ui/index.html")).unwrap();
        assert!(ui.contains("app:nearest-wall/nearest") && ui.contains("src=\"app.js\""), "{ui}");
        let sdk = std::fs::read_to_string(project.join("ui/faber-shell.js")).unwrap();
        assert_eq!(sdk, crate::ui::SDK, "the SDK is copied as it is");
        let js = std::fs::read_to_string(project.join("ui/app.js")).unwrap();
        assert!(js.contains("from \"./faber-shell.js\""), "{js}");
        assert!(!project.join(SAMPLE).exists(), "an app has no recordings");
        let e = new_app("Nearest Wall", dir.path()).unwrap_err();
        assert!(e.message.contains("an app's name") && e.hint.contains("wall-follower"), "{e:?}");
    }

    #[test]
    fn bad_names_and_full_folders_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let e = new_connector("Acme Ranger", Kind::Declarative, dir.path()).unwrap_err();
        assert!(e.hint.contains("acme-ranger"), "{e:?}");
        new_connector("acme", Kind::Declarative, dir.path()).unwrap();
        let e = new_connector("acme", Kind::Declarative, dir.path()).unwrap_err();
        assert!(e.message.contains("already exists"), "{e:?}");
        assert!(!dir.path().join("acme/Cargo.toml").exists(), "a declarative connector has no code");
    }

    /// The templates are real projects: their recording is the sample, written here and
    /// committed (like the generated TypeScript types, commit what the tests write).
    #[test]
    fn the_templates_hold_the_sample_capture() {
        let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates");
        let mut rewritten = Vec::new();
        for t in ["connector-rust", "connector-declarative"] {
            let path = templates.join(t).join(SAMPLE);
            if std::fs::read(&path).ok() != Some(sample_capture()) {
                std::fs::write(&path, sample_capture()).unwrap();
                rewritten.push(path.display().to_string());
            }
        }
        assert!(rewritten.is_empty(), "out of date, now rewritten; commit them: {rewritten:?}");
    }
}
