//! What the app tests share (`tests/apps.rs`, `tests/bridge.rs`): the example apps built as
//! components from the workspace's own crates, and packed as `faber pack` packs them.
//!
//! Without the `wasm32-unknown-unknown` target the tests say so and pass; CI sets
//! `FABER_REQUIRE_WASM=1`, which makes a missing target a failure.

#![allow(dead_code)] // each test binary uses only some of this

use faber_core::packages;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Mutex;

const TARGET: &str = "wasm32-unknown-unknown";

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn have_target() -> bool {
    let rustc = Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    let rustc = if rustc.exists() { rustc } else { PathBuf::from("rustc") };
    let Ok(out) = Process::new(rustc).args(["--print", "target-libdir", "--target", TARGET]).output() else {
        return false;
    };
    let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    out.status.success() && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

/// The component built from a workspace crate, or `None` (with a message) without the target.
pub fn component(package: &str) -> Option<Vec<u8>> {
    static BUILT: Mutex<Option<HashMap<String, Vec<u8>>>> = Mutex::new(None);
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    let built = built.get_or_insert_with(HashMap::new);
    if let Some(c) = built.get(package) {
        return Some(c.clone());
    }
    if !have_target() {
        assert!(
            std::env::var_os("FABER_REQUIRE_WASM").is_none(),
            "FABER_REQUIRE_WASM is set, but the {TARGET} target is missing"
        );
        eprintln!(
            "skipping the app tests: the {TARGET} target is not installed (rustup target add {TARGET})."
        );
        return None;
    }
    let target_dir = workspace()
        .join(std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| "target".into()))
        .join("wasm-guests");
    let status = Process::new(env!("CARGO"))
        .current_dir(workspace())
        .env("CARGO_TARGET_DIR", &target_dir)
        .args(["rustc", "-p", package, "--lib", "--release", "--target", TARGET, "--crate-type", "cdylib"])
        .status()
        .expect("run cargo");
    assert!(status.success(), "building {package} for {TARGET} failed");
    let file = target_dir.join(TARGET).join("release").join(format!("{}.wasm", package.replace('-', "_")));
    let module = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let component = wit_component::ComponentEncoder::default()
        .module(&module)
        .expect("a module with component type information")
        .validate(true)
        .encode()
        .expect("wrap the module as a component");
    built.insert(package.into(), component.clone());
    Some(component)
}

/// An example app (`examples/apps/<name>`), packed as `faber pack` packs it: its manifest,
/// README, LICENSE, every file of its UI and its component.
pub fn example_package(name: &str, wasm: &[u8]) -> Vec<u8> {
    let dir = workspace().join("examples/apps").join(name);
    let mut files = BTreeMap::new();
    for f in ["faber.toml", "README.md", "LICENSE"] {
        files.insert(f.to_string(), std::fs::read(dir.join(f)).unwrap());
    }
    for entry in std::fs::read_dir(dir.join("ui")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        files.insert(format!("ui/{name}"), std::fs::read(entry.path()).unwrap());
    }
    files.insert("app.wasm".to_string(), wasm.to_vec());
    packages::pack(&files).unwrap()
}

/// A package's files by name, all but its component.
fn files_but_the_component(package: &[u8]) -> BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).unwrap();
        let name = e.name().unwrap().into_owned();
        if !name.ends_with(".wasm") {
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            out.insert(name, data);
        }
    }
    out
}

/// The package a shell end-to-end test installs (`shell/e2e/fixtures/<file>`) must be `fresh`.
/// Written here and committed, like the generated TypeScript types. Every file but the component
/// must match; the component's bytes depend on the Rust toolchain (CI's stable moves on), so it is
/// only rewritten with FABER_REFRESH_FIXTURES set, or with the rest.
pub fn check_e2e_fixture(file: &str, fresh: Vec<u8>) {
    let committed = workspace().join("shell/e2e/fixtures").join(file);
    let old = std::fs::read(&committed).ok();
    let refresh = std::env::var_os("FABER_REFRESH_FIXTURES").is_some();
    let same = old.as_deref().is_some_and(|o| files_but_the_component(o) == files_but_the_component(&fresh));
    if !same || (refresh && old.as_deref() != Some(&fresh)) {
        std::fs::create_dir_all(committed.parent().unwrap()).unwrap();
        std::fs::write(&committed, fresh).unwrap();
        panic!("{} was out of date and has been rewritten; commit it", committed.display());
    }
    packages::open(&old.unwrap()).expect("the committed package opens");
}
