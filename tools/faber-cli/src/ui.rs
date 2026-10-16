//! `faber check` for an app's window (`ui/`): what its pages load must be in the package.
//!
//! An app's page runs in a sandboxed frame that can reach nothing but its own files and the shell
//! (docs/apps.md §4): a script from a CDN, a font from the web or a file the package lacks simply
//! doesn't load, and the window stays blank with no message. This finds those before packing,
//! with the file that asks for it. It also says when `ui/faber-shell.js` isn't the
//! `@faber-os/shell` this `faber` was built with.

use crate::CliError;
use std::path::Path;

/// `@faber-os/shell` as this `faber` copies it into a new app: the SDK's build output
/// (`npm run build` in `sdk/js/shell`), kept identical by the SDK's own tests.
pub const SDK: &str = include_str!("../../../templates/app-rust/ui/faber-shell.js");

/// The SDK's file name in an app's `ui/`.
pub const SDK_FILE: &str = "faber-shell.js";

/// What one UI file asks to load, by the text it uses for it.
fn references(name: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    if name.ends_with(".html") || name.ends_with(".htm") {
        for attr in ["src=", "href="] {
            found.extend(quoted_after(text, attr));
        }
    } else if name.ends_with(".js") || name.ends_with(".mjs") {
        found.extend(quoted_after(text, "from "));
        found.extend(quoted_after(text, "import "));
        found.extend(quoted_after(text, "import("));
    } else if name.ends_with(".css") {
        for u in text.split("url(").skip(1) {
            let inner = u.split(')').next().unwrap_or("").trim().trim_matches(['"', '\'']);
            found.push(inner.to_string());
        }
        found.extend(quoted_after(text, "@import "));
    }
    found
}

/// Every quoted string that directly follows `marker` (`src="a.js"` → `a.js`).
fn quoted_after(text: &str, marker: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        rest = &rest[at + marker.len()..];
        let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else { continue };
        if let Some(end) = rest[1..].find(q) {
            out.push(rest[1..1 + end].to_string());
        }
    }
    out
}

/// Where a reference from `from` (a path in `ui/`) points, within `ui/`; `Err` says why it can't
/// load in an app's window.
fn resolve(from: &str, reference: &str) -> Result<Option<String>, &'static str> {
    let r = reference.trim();
    if r.is_empty() || r.starts_with('#') || r.starts_with("data:") || r.starts_with("blob:") {
        return Ok(None);
    }
    let lower = r.to_ascii_lowercase();
    if lower.starts_with("http:") || lower.starts_with("https:") || r.starts_with("//") {
        return Err("network");
    }
    if lower.contains(':') && !lower.starts_with('.') && !lower.contains('/') {
        return Err("scheme"); // mailto:, javascript:, …
    }
    if r.starts_with('/') {
        return Err("absolute");
    }
    let path = r.split(['?', '#']).next().unwrap_or(r);
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop(); // the file itself
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("outside");
                }
            }
            s => parts.push(s),
        }
    }
    Ok(Some(parts.join("/")))
}

/// Checks that everything the UI files (`files`, paths in `ui/`) load is in `ui/`. Returns notes
/// worth printing that are no reason to stop (an SDK other than this `faber`'s).
pub fn check_references(dir: &Path, files: &[String]) -> Result<Vec<String>, CliError> {
    let ui = dir.join("ui");
    let mut notes = Vec::new();
    for file in files {
        let path = file.split('/').fold(ui.clone(), |p, s| p.join(s));
        let Ok(text) = std::fs::read_to_string(&path) else { continue }; // images and fonts
        if file == SDK_FILE && text != SDK {
            notes.push(format!(
                "ui/{SDK_FILE} isn't the @faber-os/shell this faber has. If the window misbehaves, \
                 copy in a fresh one: faber new app makes it, or build it in sdk/js/shell."
            ));
        }
        for reference in references(file, &text) {
            let why = match resolve(file, &reference) {
                Ok(None) => continue,
                Ok(Some(target)) if files.contains(&target) => continue,
                Ok(Some(target)) => format!("ui/{target} isn't in ui/"),
                Err("network") => "an app's window can't reach the network".to_string(),
                Err("absolute") => "a path starting with / doesn't lead to the app's own files".to_string(),
                Err("outside") => "it is outside ui/".to_string(),
                Err(_) => "an app's window can only load its own files".to_string(),
            };
            return Err(CliError::new(
                format!("ui/{file} loads \"{reference}\", but {why}."),
                "Put every file the window needs in ui/ and load it by a relative path, like \
                 src=\"app.js\". Copy scripts and fonts from the web into ui/ too.",
            ));
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let mut names = Vec::new();
        for (name, text) in files {
            let path = dir.path().join("ui").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            names.push(name.to_string());
        }
        names.sort();
        (dir, names)
    }

    #[test]
    fn what_a_page_loads_is_found_in_html_scripts_and_styles() {
        let html = r##"<link rel="stylesheet" href="style.css"><script type="module" src='app.js'></script><a href="#top">"##;
        assert_eq!(references("index.html", html), ["app.js", "style.css", "#top"]);
        let js = "import { faber } from './faber-shell.js';\nimport \"./side.js\";\nconst m = await import('./lazy.js');";
        assert_eq!(references("app.js", js), ["./faber-shell.js", "./side.js", "./lazy.js"]);
        let css = "@import 'more.css';\nbody { background: url(img/bg.png) }";
        assert_eq!(references("style.css", css), ["img/bg.png", "more.css"]);
    }

    #[test]
    fn references_resolve_inside_ui_only() {
        assert_eq!(resolve("index.html", "./app.js"), Ok(Some("app.js".into())));
        assert_eq!(resolve("js/app.js", "../img/a.png?v=2"), Ok(Some("img/a.png".into())));
        assert_eq!(resolve("index.html", "data:image/png;base64,AA"), Ok(None));
        assert_eq!(resolve("index.html", "https://cdn.example/x.js"), Err("network"));
        assert_eq!(resolve("index.html", "//cdn.example/x.js"), Err("network"));
        assert_eq!(resolve("index.html", "/api/v0/parts"), Err("absolute"));
        assert_eq!(resolve("index.html", "../faber.toml"), Err("outside"));
        assert_eq!(resolve("index.html", "javascript:alert(1)"), Err("scheme"));
    }

    #[test]
    fn a_ui_that_loads_only_its_own_files_passes() {
        let (dir, files) = project(&[
            ("index.html", r#"<script type="module" src="app.js"></script>"#),
            ("app.js", "import { faber } from './faber-shell.js';"),
            (SDK_FILE, SDK),
        ]);
        assert_eq!(check_references(dir.path(), &files).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_missing_file_or_the_network_is_refused_naming_the_page() {
        let (dir, files) = project(&[("index.html", r#"<script src="app.js"></script>"#)]);
        let e = check_references(dir.path(), &files).unwrap_err();
        assert_eq!(e.message, "ui/index.html loads \"app.js\", but ui/app.js isn't in ui/.");
        assert!(e.hint.contains("relative path"), "{e:?}");
        let (dir, files) = project(&[
            ("index.html", r#"<script src="app.js"></script>"#),
            ("app.js", "import x from 'https://cdn.example/lib.js';"),
        ]);
        let e = check_references(dir.path(), &files).unwrap_err();
        assert!(e.message.contains("can't reach the network"), "{e:?}");
    }

    #[test]
    fn an_sdk_other_than_this_fabers_is_noted() {
        let (dir, files) = project(&[("index.html", "<p>hi</p>"), (SDK_FILE, "// old")]);
        let notes = check_references(dir.path(), &files).unwrap();
        assert!(notes[0].contains("isn't the @faber-os/shell this faber has"), "{notes:?}");
    }

    /// The example app and the template are what people copy: their windows must pass.
    #[test]
    fn the_example_and_the_template_load_only_their_own_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for dir in ["examples/apps/wall-follower", "templates/app-rust"] {
            let dir = root.join(dir);
            let mut files: Vec<String> = std::fs::read_dir(dir.join("ui"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            files.sort();
            assert_eq!(check_references(&dir, &files).unwrap(), Vec::<String>::new(), "{}", dir.display());
        }
    }

    #[test]
    fn the_sdk_this_faber_has_is_the_built_module() {
        assert!(SDK.starts_with("// @faber-os/shell "), "{}", &SDK[..60.min(SDK.len())]);
        assert!(SDK.contains("export function connect("));
    }
}
