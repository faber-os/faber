//! Installing and removing connector packages while the machine runs (docs/packages.md §5).
//!
//! A package is checked in full before anything changes: its file ([`packages::open`]), then
//! the connector itself, loaded by the engine for its kind from a staging folder. Only then is
//! it moved into `<data>/connectors/<id>/`, the connector list reloaded, and the parts on that
//! connector restarted with the new version. The core never restarts.

use super::{ErrorKind, Machine, MachineError, State};
use crate::connectors::LoadProblem;
use crate::leases::EndReason;
use crate::names::PartName;
use crate::packages::{self, Installed, PackageError, Store};
use crate::{declarative, wasm};
use faber_connector::manifest_file::PackageHeader;
use faber_connector::{Definition, Manifest, Problem};
use std::path::Path;

/// Every connector: `base` (the built-in ones), then those installed in `folder`
/// (`<data>/connectors/`), each folder read by the engine for its kind. An installed one can't
/// replace one that is already there. Never fails: a bad folder is logged, left out and
/// returned as a [`LoadProblem`].
pub fn installed_connectors(base: Vec<Definition>, folder: &Path) -> (Vec<Definition>, Vec<LoadProblem>) {
    let mut defs = base;
    // Each engine reads only the folders of its own kind, so no folder is reported twice.
    let (mut installed, mut problems) = wasm::load_installed(folder, wasm::Limits::default());
    let (declarative, declarative_problems) = declarative::installed::load_installed(folder);
    installed.extend(declarative);
    problems.extend(declarative_problems);
    for d in installed {
        if defs.iter().any(|b| b.manifest.id == d.manifest.id) {
            let problem = Problem::new(
                format!(
                    "The installed connector {} has the id of a connector that is already there.",
                    d.manifest.id
                ),
                "It is not loaded. Rename its id and its folder.",
            );
            tracing::error!(target: "core", "{} {}", problem.message, problem.hint);
            problems.push(LoadProblem { folder: d.manifest.id.clone(), problem });
            continue;
        }
        defs.push(d);
    }
    problems.sort_by(|a, b| a.folder.cmp(&b.folder));
    (defs, problems)
}

/// What a package would install, read from its file and checked, before it is installed.
#[derive(Clone, Debug, PartialEq)]
pub struct PackagePreview {
    pub header: PackageHeader,
    /// The connector as the core loaded it from the package.
    pub manifest: Manifest,
    pub readme: String,
    /// The recordings in its `captures/`.
    pub captures: Vec<String>,
    /// The version installed now, which this replaces.
    pub replaces: Option<String>,
}

/// An installed package and how it is doing.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageView {
    pub installed: Installed,
    /// The parts that use its connector. It can't be uninstalled while there are any.
    pub used_by: Vec<PartName>,
    /// Why its connector isn't loaded, when it isn't.
    pub problem: Option<Problem>,
}

impl From<PackageError> for MachineError {
    fn from(e: PackageError) -> Self {
        let kind = match e.code {
            "package-too-big" => ErrorKind::TooLarge,
            "not-installed" => ErrorKind::NotFound,
            "io-error" => ErrorKind::Internal,
            _ => ErrorKind::Unprocessable,
        };
        MachineError::new(kind, e.code, e.message, e.hint)
    }
}

/// Loads a staged package's folder with the engine for its kind, exactly as the core will.
fn load(kind: &str, folder: &Path) -> Result<Definition, Problem> {
    match kind {
        "wasm" => wasm::load_folder(folder, wasm::Limits::default()),
        _ => declarative::installed::load_folder(folder),
    }
}

/// "the part "lidar"" or "the parts "a" and "b"".
fn parts_phrase(parts: &[PartName]) -> String {
    let quoted: Vec<String> = parts.iter().map(|p| format!("\"{p}\"")).collect();
    match quoted.as_slice() {
        [one] => format!("the part {one}"),
        [rest @ .., last] => format!("the parts {} and {last}", rest.join(", ")),
        [] => String::new(),
    }
}

impl Machine {
    fn package_store(&self) -> Store {
        Store::new(&self.data_dir)
    }

    /// Every installed package (`GET /api/v0/packages`).
    pub fn packages(&self) -> Result<Vec<PackageView>, MachineError> {
        let list = self.package_store().list()?;
        let st = self.state.lock().expect("machine lock");
        Ok(list.into_iter().map(|p| self.package_view(&st, p)).collect())
    }

    fn package_view(&self, st: &State, installed: Installed) -> PackageView {
        let used_by = parts_using(st, &installed.id);
        let problem = self
            .connector_problems()
            .into_iter()
            .find(|p| p.folder == installed.id)
            .map(|p| p.problem)
            .or_else(|| {
                self.definition(&installed.id).is_none().then(|| {
                    Problem::new(
                        format!("The connector {} isn't loaded.", installed.id),
                        "Install the package again.",
                    )
                })
            });
        PackageView { installed, used_by, problem }
    }

    /// Checks a package file in full and, unless `preview`, installs it: `source` is the file's
    /// name. Parts that use its connector restart with the new version.
    pub fn install_package(
        &self,
        bytes: &[u8],
        source: &str,
        preview: bool,
    ) -> Result<(PackagePreview, Option<PackageView>), MachineError> {
        let opened = packages::open(bytes)?;
        let id = opened.header.id.clone();
        if self.base_connectors.iter().any(|d| d.manifest.id == id) {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "package-id-taken",
                format!("This package is the connector \"{id}\", which is built into Faber."),
                "It can't replace the built-in one. Ask its author to give it another id.",
            ));
        }
        let _one_at_a_time = self.package_lock.lock().expect("package lock");
        let store = self.package_store();
        let replaces = store.get(&id).ok().flatten().map(|p| p.version);
        let kind = opened.header.kind.clone();
        let prepared = store.prepare(opened, |folder| load(&kind, folder))?;
        let opened = prepared.opened();
        let preview_of = PackagePreview {
            header: opened.header.clone(),
            manifest: prepared.loaded.manifest.clone(),
            readme: opened.readme.clone(),
            captures: opened.captures.clone(),
            replaces,
        };
        if preview {
            prepared.discard();
            return Ok((preview_of, None));
        }

        let mut st = self.state.lock().expect("machine lock");
        let swapped = prepared.swap_in()?;
        let problems = self.reload_connectors();
        let failed = problems.into_iter().find(|p| p.folder == id).map(|p| p.problem).or_else(|| {
            self.definition(&id).is_none().then(|| {
                Problem::new(
                    format!("The connector {id} didn't load once installed."),
                    "Try installing it again.",
                )
            })
        });
        if let Some(problem) = failed {
            tracing::error!(target: "core::machine", "installing {id} failed: {} {}", problem.message, problem.hint);
            let rolled_back = swapped.roll_back();
            self.reload_connectors();
            rolled_back?;
            return Err(MachineError::new(
                ErrorKind::Unprocessable,
                "package-not-loadable",
                problem.message,
                problem.hint,
            ));
        }
        let at = jiff::Timestamp::now().round(jiff::Unit::Second).unwrap_or_else(|_| jiff::Timestamp::now());
        let installed = swapped.keep(&at.to_string(), source)?;
        tracing::info!(target: "core::machine", "installed the package {id} {} from {source}", installed.version);
        self.restart_parts_on(&mut st, &id);
        let view = self.package_view(&st, installed);
        Ok((preview_of, Some(view)))
    }

    /// Uninstalls a package. Refused while a part uses its connector, naming the parts.
    pub fn uninstall_package(&self, id: &str) -> Result<(), MachineError> {
        let _one_at_a_time = self.package_lock.lock().expect("package lock");
        let mut st = self.state.lock().expect("machine lock");
        let users = parts_using(&st, id);
        if !users.is_empty() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "package-in-use",
                format!("The connector \"{id}\" is used by {}.", parts_phrase(&users)),
                "Remove those parts in the Parts window first, then uninstall it.",
            ));
        }
        self.package_store().uninstall(id)?;
        tracing::info!(target: "core::machine", "uninstalled the package {id}");
        self.reload_connectors();
        self.reconcile(&mut st);
        Ok(())
    }

    /// Reads `<data>/connectors/` again. Returns the folders that couldn't be loaded.
    fn reload_connectors(&self) -> Vec<LoadProblem> {
        let (defs, problems) =
            installed_connectors(self.base_connectors.clone(), &self.data_dir.join("connectors"));
        *self.connectors.write().expect("connectors lock") = defs;
        *self.connector_problems.lock().expect("connector problems lock") = problems.clone();
        problems
    }

    /// Stops every part on `connector` and starts it again from the current connector list.
    fn restart_parts_on(&self, st: &mut State, connector: &str) {
        let names: Vec<PartName> = st
            .running
            .iter()
            .filter(|(_, r)| r.config.connector == connector)
            .map(|(n, _)| n.clone())
            .collect();
        for name in names {
            self.revoke_part(st, &name, EndReason::PartRestarted);
            self.stop_part(st, &name);
        }
        self.reconcile(st);
    }
}

fn parts_using(st: &State, connector: &str) -> Vec<PartName> {
    st.config.parts.iter().filter(|(_, c)| c.connector == connector).map(|(n, _)| n.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_named_in_a_sentence() {
        let names = |n: &[&str]| n.iter().map(|s| PartName::parse(s).unwrap()).collect::<Vec<_>>();
        assert_eq!(parts_phrase(&names(&["lidar"])), "the part \"lidar\"");
        assert_eq!(parts_phrase(&names(&["a", "b", "c"])), "the parts \"a\", \"b\" and \"c\"");
    }
}
