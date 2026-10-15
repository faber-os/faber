//! An app from the template, end to end (docs/apps.md): `faber new app` makes a project outside
//! Faber's tree whose own tests pass, `faber check` builds it and runs it in the sandbox, `faber
//! pack` packs it with its UI, and the package installs on a running core, where it runs once
//! its role has a part.
//!
//! Needs the wasm32-unknown-unknown target: without it the test says so and passes, unless
//! FABER_REQUIRE_WASM is set (CI).

use faber_cli::check::{self, Options};
use faber_cli::{pack, template};
use faber_core::apps::AppStatus;
use faber_core::clock::MonotonicClock;
use faber_core::machine::{Machine, MachineOptions, NewPart};
use faber_core::packages;
use faber_core::ports::{ReplayLibrary, SystemOpener};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Nothing;
impl faber_core::ports::SerialDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::ports::Discovered> {
        Vec::new()
    }
}
impl faber_core::ports::UsbProblemDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::ports::FailedUsb> {
        Vec::new()
    }
}
impl faber_core::disks::DiskDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::disks::Disk> {
        Vec::new()
    }
}

fn guest_target() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    workspace
        .join(std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| "target".into()))
        .join("wasm-guests")
}

#[test]
fn an_app_from_the_template_checks_packs_installs_and_runs() {
    if !check::have_target() {
        assert!(
            std::env::var_os("FABER_REQUIRE_WASM").is_none(),
            "FABER_REQUIRE_WASM is set, but the wasm32-unknown-unknown target is missing"
        );
        eprintln!("skipping: run `rustup target add wasm32-unknown-unknown` to run this test");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    let project = template::new_app("nearest-wall", work.path()).unwrap();
    let target = guest_target();
    let tested = Command::new(env!("CARGO"))
        .current_dir(&project)
        .args(["test", "--quiet", "--target-dir"])
        .arg(&target)
        .status()
        .unwrap();
    assert!(tested.success(), "the template's own tests pass");

    let opts = Options { target_dir: Some(target) };
    let mut log = Vec::new();
    let report = check::check(&project, &opts, &mut log).unwrap();
    let text = String::from_utf8(log.clone()).unwrap();
    assert!(text.starts_with("Checking nearest-wall 0.1.0 (app)"), "{text}");
    assert!(text.contains("asks only for the app world") && text.contains("runs 20 ticks"), "{text}");
    assert!(report.passed());
    let file = pack::pack(&project, None, &opts, &mut log).unwrap();
    let opened = packages::open(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(opened.header.kind, "app");

    // On a core: it installs off, runs once its role has a part, and publishes.
    let data = tempfile::tempdir().unwrap();
    let replays = ReplayLibrary::default();
    let machine = Machine::start(MachineOptions {
        data_dir: data.path().to_path_buf(),
        clock: Arc::new(MonotonicClock::new()),
        discovery: Arc::new(Nothing),
        usb_problems: Arc::new(Nothing),
        disks: Arc::new(Nothing),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: faber_core::runtime::Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    machine.install_package(&std::fs::read(&file).unwrap(), "nearest-wall-0.1.0.faberpkg", false).unwrap();
    assert!(data.path().join("apps/nearest-wall/ui/index.html").is_file());
    machine
        .add_part(NewPart {
            name: "front".into(),
            connector: "generic-distance".into(),
            port: None,
            simulate: Some(faber_core::config::Simulate::Manual),
            mount: Default::default(),
            settings: Default::default(),
        })
        .unwrap();
    let mut roles = BTreeMap::new();
    roles.insert("sensors".to_string(), vec!["front".to_string()]);
    machine.set_app_roles("nearest-wall", roles).unwrap();
    machine.set_app_enabled("nearest-wall", true).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let nearest = faber_core::names::SignalAddress::parse("app:nearest-wall/nearest").unwrap();
    while !(machine.app("nearest-wall").unwrap().status == AppStatus::Running
        && matches!(machine.router().latest(&nearest), Some(Some(_))))
    {
        assert!(Instant::now() < until, "{:?}", machine.app("nearest-wall").unwrap().status);
        std::thread::sleep(Duration::from_millis(10));
    }
    machine.shutdown();
}
