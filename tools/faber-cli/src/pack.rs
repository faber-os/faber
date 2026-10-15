//! `faber pack`: checks the connector or app, then writes `<id>-<version>.faberpkg` (docs/packages.md
//! §5) with the core's own package writer, so what packs also installs.

use crate::CliError;
use crate::check::{self, Options, Report};
use faber_core::packages;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Checks the project in `dir` and packs it into `out_dir` (default: `dir`). Refuses when a
/// recording fails, so a package always passed its own captures.
pub fn pack(
    dir: &Path,
    out_dir: Option<&Path>,
    opts: &Options,
    out: &mut dyn Write,
) -> Result<PathBuf, CliError> {
    let report = check::check(dir, opts, out)?;
    for line in report.lines() {
        let _ = writeln!(out, "{line}");
    }
    if !report.passed() {
        return Err(CliError::new(
            "Not packed: a recording in captures/ failed (above).",
            "Fix the connector, or remove a recording that isn't from this device, then run faber pack again.",
        ));
    }
    let file = package_from(dir, &report, out_dir.unwrap_or(dir))?;
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let _ = writeln!(out, "Packed {} ({} KB).", file.display(), size.div_ceil(1024));
    Ok(file)
}

/// The package's files from the project, written as a package next to it.
fn package_from(dir: &Path, report: &Report, out_dir: &Path) -> Result<PathBuf, CliError> {
    let read = |rel: &str| {
        std::fs::read(dir.join(rel)).map_err(|e| {
            CliError::new(format!("{rel} can't be read: {e}"), "Check the file, then run faber pack again.")
        })
    };
    let mut files = BTreeMap::new();
    for rel in check::REQUIRED {
        files.insert(rel.to_string(), read(rel)?);
    }
    if dir.join("icon.svg").is_file() {
        files.insert("icon.svg".into(), read("icon.svg")?);
    }
    for rel in &report.ui {
        files.insert(rel.clone(), read(rel)?);
    }
    for c in &report.captures {
        let rel = format!("captures/{}", c.name);
        files.insert(rel.clone(), read(&rel)?);
    }
    if let Some(component) = &report.component {
        let module = report.module.clone().unwrap_or_else(|| "connector.wasm".into());
        files.insert(module, component.clone());
    }
    let bytes = packages::pack(&files).map_err(|e| CliError::new(e.message, e.hint))?;
    let h = &report.header;
    let file = out_dir.join(format!("{}-{}.{}", h.id, h.version, packages::EXTENSION));
    std::fs::create_dir_all(out_dir).and_then(|_| std::fs::write(&file, bytes)).map_err(|e| {
        CliError::new(format!("{} can't be written: {e}", file.display()), "Check that you may write there.")
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{self, Kind};

    #[test]
    fn a_declarative_connector_checks_and_packs_from_the_template() {
        let dir = tempfile::tempdir().unwrap();
        let project = template::new_connector("acme-ranger", Kind::Declarative, dir.path()).unwrap();
        let mut log = Vec::new();
        let report = check::check(&project, &Options::default(), &mut log).unwrap();
        assert!(report.passed(), "{:?}", report.lines());
        let c = &report.captures[0];
        assert_eq!((c.name.as_str(), c.readings.get("distance")), ("sample.fbcap", Some(&5)));
        assert_eq!(c.last.as_deref(), Some("0.400 m, ok"));
        let file = pack(&project, None, &Options::default(), &mut log).unwrap();
        assert!(file.ends_with("acme-ranger-0.1.0.faberpkg"));
        let opened = packages::open(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(opened.header.name, "Acme ranger");
        assert_eq!(opened.captures, ["sample.fbcap"]);
        let text = String::from_utf8(log).unwrap();
        assert!(text.starts_with("Checking acme-ranger 0.1.0 (declarative connector)"), "{text}");
    }

    #[test]
    fn a_recording_the_parser_doesnt_understand_stops_the_pack() {
        let dir = tempfile::tempdir().unwrap();
        let project = template::new_connector("acme-ranger", Kind::Declarative, dir.path()).unwrap();
        let toml =
            std::fs::read_to_string(project.join("faber.toml")).unwrap().replace("D:{meters", "R={meters");
        std::fs::write(project.join("faber.toml"), toml).unwrap();
        let e = pack(&project, None, &Options::default(), &mut Vec::new()).unwrap_err();
        assert!(e.message.contains("recording in captures/ failed"), "{e:?}");
        let report = check::check(&project, &Options::default(), &mut Vec::new()).unwrap();
        assert!(report.lines().iter().any(|l| l.contains("recognised nothing")), "{:?}", report.lines());
        assert!(!project.join("acme-ranger-0.1.0.faberpkg").exists());
    }

    /// The package the shell's end-to-end test installs through the Install window
    /// (shell/e2e/packages.spec.ts): the declarative template, packed. Written here and
    /// committed, like the generated TypeScript types: commit what the tests write.
    #[test]
    fn the_e2e_package_is_the_packed_template() {
        let dir = tempfile::tempdir().unwrap();
        let project = template::new_connector("e2e-line-sensor", Kind::Declarative, dir.path()).unwrap();
        let file = pack(&project, None, &Options::default(), &mut Vec::new()).unwrap();
        let fresh = std::fs::read(file).unwrap();
        let committed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../shell/e2e/fixtures/e2e-line-sensor-0.1.0.faberpkg");
        if std::fs::read(&committed).ok() != Some(fresh.clone()) {
            std::fs::create_dir_all(committed.parent().unwrap()).unwrap();
            std::fs::write(&committed, fresh).unwrap();
            panic!("{} was out of date and has been rewritten; commit it", committed.display());
        }
    }

    #[test]
    fn manifest_mistakes_name_their_line() {
        let dir = tempfile::tempdir().unwrap();
        let project = template::new_connector("acme-ranger", Kind::Declarative, dir.path()).unwrap();
        let toml =
            std::fs::read_to_string(project.join("faber.toml")).unwrap().replace("{meters:u32}", "{meters}");
        std::fs::write(project.join("faber.toml"), toml).unwrap();
        let e = check::check(&project, &Options::default(), &mut Vec::new()).unwrap_err();
        assert!(e.message.starts_with("faber.toml line 27:"), "{e:?}");
        std::fs::remove_file(project.join("LICENSE")).unwrap();
        let e = check::check(&project, &Options::default(), &mut Vec::new()).unwrap_err();
        assert!(e.message.contains("no LICENSE"), "{e:?}");
    }
}
