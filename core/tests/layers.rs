//! Enforces the module layers inside faber-core (docs/development.md, "Inside faber-core"):
//! a module may only use modules in lower layers. Fails with the exact file and line.

use std::path::{Path, PathBuf};

/// Module → layer. Must match docs/development.md.
const LAYERS: &[(&str, u8)] = &[
    ("clock", 1),
    ("names", 1),
    ("backup", 2),
    ("config", 2),
    ("connectors", 2),
    ("disks", 2),
    ("files", 2),
    ("host", 2),
    ("leases", 2),
    ("estop", 2),
    ("settings", 2),
    ("sim", 2),
    ("simulation", 2),
    ("logs", 2),
    ("ports", 2),
    ("router", 2),
    ("translations", 2),
    ("declarative", 3),
    ("runtime", 3),
    ("simparts", 3),
    ("suggest", 3),
    ("machine", 4),
    ("api", 5),
];

fn layer(module: &str) -> Option<u8> {
    LAYERS.iter().find(|(m, _)| *m == module).map(|(_, l)| *l)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The top-level module a source file belongs to, e.g. `src/ports/stream.rs` → `ports`.
fn module_of(src: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(src).ok()?;
    let first = rel.components().next()?.as_os_str().to_str()?;
    let name = first.trim_end_matches(".rs");
    (name != "lib" && name != "main").then(|| name.to_string())
}

#[test]
fn modules_only_use_lower_layers() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut violations = Vec::new();
    for file in &files {
        let Some(module) = module_of(&src, file) else { continue };
        let Some(own) = layer(&module) else {
            violations.push(format!(
                "{}: module `{module}` has no layer; add it to docs/development.md and this test",
                file.display()
            ));
            continue;
        };
        for (n, line) in std::fs::read_to_string(file).unwrap().lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for (i, _) in code.match_indices("crate::") {
                let used: String = code[i + "crate::".len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let Some(theirs) = layer(&used) else { continue };
                if used != module && theirs >= own {
                    violations.push(format!(
                        "{}:{}: `{module}` (layer {own}) uses `{used}` (layer {theirs}); modules may only use lower layers",
                        file.display(),
                        n + 1
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "architecture violations:\n{}", violations.join("\n"));
}

#[test]
fn every_module_in_lib_rs_has_a_layer() {
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap();
    for line in lib.lines().filter_map(|l| l.strip_prefix("pub mod ")) {
        let name = line.trim_end_matches(';');
        assert!(layer(name).is_some(), "`{name}` is declared in lib.rs but has no layer");
    }
}
