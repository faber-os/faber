//! Installing and removing connector packages while the machine runs (docs/packages.md §5).
//!
//! A package is checked in full before anything changes: its file ([`packages::open`]), then
//! the connector itself, loaded by the engine for its kind from a staging folder. Only then is
//! it moved into `<data>/connectors/<id>/`, the connector list reloaded, and the parts on that
//! connector restarted with the new version. The core never restarts.
//!
//! Backups hold the installed connectors too (D41, docs/api.md "Storage and backups"). Restoring
//! one never copies connector code in blind: each goes through the same checks and staging load
//! as installing its package, before anything changes.

use super::{ErrorKind, Machine, MachineError, State};
use crate::backup::Archive;
use crate::connectors::{LoadProblem, ShownAs};
use crate::leases::EndReason;
use crate::names::PartName;
use crate::packages::{self, Installed, PackageError, Prepared, Store, Swapped};
use crate::{declarative, wasm};
use faber_connector::manifest_file::PackageHeader;
use faber_connector::{Definition, Manifest, Problem};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A connector from a backup, checked and loaded in staging, waiting to be put in place.
pub(super) struct StagedConnector {
    prepared: Prepared<Definition>,
    /// The package file it was first installed from, as the backup's packages.toml says.
    source: String,
}

fn backup_invalid(id: &str, problem: Problem) -> MachineError {
    MachineError::new(
        ErrorKind::Unprocessable,
        "backup-invalid",
        format!("The connector \"{id}\" in this backup can't be restored: {}", problem.message),
        problem.hint,
    )
}

fn now_rfc3339() -> String {
    let now = jiff::Timestamp::now();
    now.round(jiff::Unit::Second).unwrap_or(now).to_string()
}

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
/// `shown` is how its files are named in a problem: the folder is only a staging copy.
fn load(kind: &str, folder: &Path, shown: &ShownAs) -> Result<Definition, Problem> {
    match kind {
        "wasm" => wasm::load_folder(folder, shown, wasm::Limits::default()),
        _ => declarative::installed::load_folder(folder, shown),
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

    /// Fires after every package install, uninstall or reload of `<data>/connectors/`, once
    /// `packages.toml` says what is installed now.
    pub fn packages_changed(&self) -> tokio::sync::watch::Receiver<u64> {
        self.packages_changed.subscribe()
    }

    fn announce_packages(&self) {
        self.packages_changed.send_modify(|n| *n += 1);
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
        let shown = ShownAs::package(source);
        let prepared = store.prepare(opened, |folder| load(&kind, folder, &shown))?;
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
            self.announce_packages();
            rolled_back?;
            return Err(MachineError::new(
                ErrorKind::Unprocessable,
                "package-not-loadable",
                problem.message,
                problem.hint,
            ));
        }
        let installed = swapped.keep(&now_rfc3339(), source)?;
        tracing::info!(target: "core::machine", "installed the package {id} {} from {source}", installed.version);
        self.restart_parts_on(&mut st, &id);
        self.announce_packages();
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
        self.announce_packages();
        Ok(())
    }

    /// What a backup holds of the packages: `packages.toml`, the connectors folder, and the
    /// installed packages whose folder is there. Folders put there by hand aren't packages, so
    /// they aren't backed up (a restore checks every connector as a package).
    pub(super) fn packages_to_back_up(&self) -> Result<(PathBuf, PathBuf, Vec<String>), MachineError> {
        let store = self.package_store();
        let ids = store
            .list()?
            .into_iter()
            .map(|p| p.id)
            .filter(|id| store.connectors_dir().join(id).is_dir())
            .collect();
        let list = self.data_dir.join("config").join(packages::LIST_FILE);
        Ok((list, store.connectors_dir().to_path_buf(), ids))
    }

    /// Step 1 of a restore, for the connectors: each one in the backup is checked and loaded in
    /// staging exactly like installing its package. The first that fails fails the restore
    /// (`backup-invalid`, naming it and why), and nothing is left staged.
    pub(super) fn stage_backup_connectors(
        &self,
        archive: &mut Archive,
    ) -> Result<Vec<StagedConnector>, MachineError> {
        let ids = archive.connector_ids();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let listed = archive.packages_toml().map(packages::parse_list).transpose();
        let store = self.package_store();
        let mut staged = Vec::new();
        for id in ids {
            match self.stage_backup_connector(&store, archive, &id, &listed) {
                Ok(s) => staged.push(s),
                Err(problem) => {
                    for s in staged {
                        s.prepared.discard();
                    }
                    return Err(backup_invalid(&id, problem));
                }
            }
        }
        Ok(staged)
    }

    fn stage_backup_connector(
        &self,
        store: &Store,
        archive: &mut Archive,
        id: &str,
        listed: &Result<Option<BTreeMap<String, Installed>>, PackageError>,
    ) -> Result<StagedConnector, Problem> {
        let problem = |e: PackageError| Problem::new(e.message, e.hint);
        let listed = listed.as_ref().map_err(|e| problem(e.clone()))?;
        let Some(entry) = listed.as_ref().and_then(|l| l.get(id)) else {
            return Err(Problem::new(
                "The backup's config/packages.toml doesn't list it as an installed package.",
                "Pick a backup Faber made (in home:/Backups, named …-backup-….zip).",
            ));
        };
        let files =
            archive.connector_files(id, packages::MAX_BYTES).map_err(|e| Problem::new(e.message, e.hint))?;
        let opened = packages::from_files(files).map_err(problem)?;
        if opened.header.id != id {
            return Err(Problem::new(
                format!("Its folder holds the connector \"{}\".", opened.header.id),
                "Pick a backup Faber made (in home:/Backups, named …-backup-….zip).",
            ));
        }
        if self.base_connectors.iter().any(|d| d.manifest.id == id) {
            return Err(Problem::new(
                "It has the id of a connector built into Faber.",
                "Restore the backup on a Faber that doesn't have it built in, or remove it from the backup.",
            ));
        }
        let kind = opened.header.kind.clone();
        let shown = ShownAs::installed(id);
        let prepared = store.prepare(opened, |folder| load(&kind, folder, &shown)).map_err(problem)?;
        Ok(StagedConnector { prepared, source: entry.source.clone() })
    }

    /// Step 2 of a restore, for the connectors: each staged one is put in place like an install.
    /// One already installed at the same version is left as it is; another version is replaced,
    /// and the parts on it restart. If one doesn't load once in place, all are put back.
    pub(super) fn restore_connectors(&self, staged: Vec<StagedConnector>) -> Result<(), MachineError> {
        if staged.is_empty() {
            return Ok(());
        }
        let store = self.package_store();
        let mut st = self.state.lock().expect("machine lock");
        let undo = |swapped: Vec<(String, Swapped, String)>| {
            for (id, s, _) in swapped.into_iter().rev() {
                if let Err(e) = s.roll_back() {
                    tracing::error!(target: "core::backup", "couldn't put connectors/{id} back: {} {}", e.message, e.hint);
                }
            }
        };
        let mut swapped = Vec::new();
        let mut rest = staged.into_iter();
        while let Some(s) = rest.next() {
            let (id, version) = (s.prepared.header().id.clone(), s.prepared.header().version.clone());
            if store.get(&id).ok().flatten().is_some_and(|i| i.version == version) {
                tracing::info!(target: "core::backup", "the backup's connector {id} {version} is installed already");
                s.prepared.discard();
                continue;
            }
            match s.prepared.swap_in() {
                Ok(done) => swapped.push((id, done, s.source)),
                Err(e) => {
                    rest.for_each(|r| r.prepared.discard());
                    undo(swapped);
                    self.reload_connectors();
                    return Err(e.into());
                }
            }
        }
        if swapped.is_empty() {
            return Ok(());
        }
        let problems = self.reload_connectors();
        let failed = swapped.iter().find_map(|(id, _, _)| {
            let problem = problems.iter().find(|p| &p.folder == id).map(|p| p.problem.clone());
            problem
                .or_else(|| {
                    self.definition(id).is_none().then(|| {
                        Problem::new("It didn't load once in place.", "Try restoring the backup again.")
                    })
                })
                .map(|p| (id.clone(), p))
        });
        if let Some((id, problem)) = failed {
            undo(swapped);
            self.reload_connectors();
            return Err(backup_invalid(&id, problem));
        }
        let at = now_rfc3339();
        for (id, done, source) in swapped {
            let installed = done.keep(&at, &source)?;
            tracing::info!(target: "core::backup", "restored the package {id} {}", installed.version);
            self.restart_parts_on(&mut st, &id);
        }
        self.announce_packages();
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
