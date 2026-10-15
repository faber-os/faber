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
use crate::apps::AppDefinition;
use crate::apps::manifest::AppManifest;
use crate::backup::{Archive, Folder};
use crate::connectors::{LoadProblem, ShownAs};
use crate::leases::EndReason;
use crate::names::PartName;
use crate::packages::{self, Installed, PackageError, Prepared, Store, Swapped};
use crate::{declarative, wasm};
use faber_connector::manifest_file::{APP_TYPE, PackageHeader};
use faber_connector::{Definition, Manifest, Problem};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A connector from a backup, checked and loaded in staging, waiting to be put in place.
pub(super) struct StagedConnector {
    prepared: Prepared<Definition>,
    /// The package file it was first installed from, as the backup's packages.toml says.
    source: String,
}

/// An app from a backup, checked and loaded in staging, waiting to be put in place with the state
/// it had saved.
pub(super) struct StagedApp {
    prepared: Prepared<AppDefinition>,
    /// The package file it was first installed from, as the backup's packages.toml says.
    source: String,
    /// Its `state.toml` in the backup, checked like the app reads it.
    state: Option<Vec<u8>>,
}

impl StagedConnector {
    pub(super) fn discard(self) {
        self.prepared.discard();
    }
}

impl StagedApp {
    pub(super) fn discard(self) {
        self.prepared.discard();
    }
}

fn backup_invalid(id: &str, problem: Problem) -> MachineError {
    backup_invalid_package("connector", id, problem)
}

fn backup_invalid_package(what: &str, id: &str, problem: Problem) -> MachineError {
    MachineError::new(
        ErrorKind::Unprocessable,
        "backup-invalid",
        format!("The {what} \"{id}\" in this backup can't be restored: {}", problem.message),
        problem.hint,
    )
}

fn pick_a_faber_backup() -> &'static str {
    "Pick a backup Faber made (in home:/Backups, named …-backup-….zip)."
}

/// An app's saved state from a backup, checked exactly as the app reads it: TOML text of string
/// keys and values, at most 64 KB of them.
fn check_app_state(bytes: &[u8]) -> Result<(), Problem> {
    let bad =
        |why: String| Problem::new(format!("its state.toml can't be used: {why}."), pick_a_faber_backup());
    let text = std::str::from_utf8(bytes).map_err(|_| bad("it isn't text".into()))?;
    let values: BTreeMap<String, String> =
        toml::from_str(text).map_err(|e| bad(e.message().trim().to_string()))?;
    let size: usize = values.iter().map(|(k, v)| k.len() + v.len()).sum();
    if size > crate::apps::STATE_MAX_BYTES {
        return Err(bad(format!("it holds more than {} KB", crate::apps::STATE_MAX_BYTES / 1024)));
    }
    Ok(())
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

/// What a package holds, as the core loaded it from the package.
#[derive(Clone, Debug, PartialEq)]
pub enum PackagePreviewOf {
    Connector(Manifest),
    App(AppManifest),
}

/// What a package would install, read from its file and checked, before it is installed.
#[derive(Clone, Debug, PartialEq)]
pub struct PackagePreview {
    pub header: PackageHeader,
    pub of: PackagePreviewOf,
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
    /// The boards it carries firmware for, for the Arduino flasher.
    pub firmware: Vec<String>,
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

/// Loads a staged connector package's folder with the engine for its kind, exactly as the core
/// will. `shown` is how its files are named in a problem: the folder is only a staging copy.
fn load(kind: &str, folder: &Path, shown: &ShownAs) -> Result<Definition, Problem> {
    match kind {
        "wasm" => wasm::load_folder(folder, shown, wasm::Limits::default()),
        _ => declarative::installed::load_folder(folder, shown),
    }
}

/// A staged package, loaded: a connector or an app.
enum Loaded {
    Connector(Definition),
    App(AppDefinition),
}

fn load_any(kind: &str, folder: &Path, shown: &ShownAs) -> Result<Loaded, Problem> {
    match kind {
        APP_TYPE => {
            crate::apps::load_folder(folder, shown, crate::sandbox::Limits::default()).map(Loaded::App)
        }
        _ => load(kind, folder, shown).map(Loaded::Connector),
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
        if installed.kind == APP_TYPE {
            let problem =
                self.app_problems().into_iter().find(|p| p.folder == installed.id).map(|p| p.problem);
            return PackageView { installed, used_by: Vec::new(), problem, firmware: Vec::new() };
        }
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
        let firmware = self.package_store().firmware_boards(&installed.id);
        PackageView { installed, used_by, problem, firmware }
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
        let kind = opened.header.kind.clone();
        let installed_now = store.get(&id).ok().flatten();
        if let Some(other) = installed_now.as_ref().filter(|p| (p.kind == APP_TYPE) != (kind == APP_TYPE)) {
            let what = |k: &str| if k == APP_TYPE { "an app" } else { "a connector" };
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "package-id-taken",
                format!(
                    "This package is {} called \"{id}\", but {} with that id is installed.",
                    what(&kind),
                    what(&other.kind)
                ),
                "Apps and connectors share ids. Uninstall the other one first, or ask the author to rename this one.",
            ));
        }
        let replaces = installed_now.map(|p| p.version);
        let shown = ShownAs::package(source);
        let prepared = store.prepare(opened, |folder| load_any(&kind, folder, &shown))?;
        let opened = prepared.opened();
        let preview_of = PackagePreview {
            header: opened.header.clone(),
            of: match &prepared.loaded {
                Loaded::Connector(d) => PackagePreviewOf::Connector(d.manifest.clone()),
                Loaded::App(a) => PackagePreviewOf::App(a.manifest.clone()),
            },
            readme: opened.readme.clone(),
            captures: opened.captures.clone(),
            replaces,
        };
        if preview {
            prepared.discard();
            return Ok((preview_of, None));
        }
        if kind == APP_TYPE {
            return self.install_app(prepared, preview_of, source);
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

    /// Puts a checked app package in place and loads it; an app that is on restarts with it.
    fn install_app(
        &self,
        prepared: Prepared<Loaded>,
        preview_of: PackagePreview,
        source: &str,
    ) -> Result<(PackagePreview, Option<PackageView>), MachineError> {
        let id = preview_of.header.id.clone();
        let swapped = prepared.swap_in()?;
        let problems = self.reload_apps();
        if let Some(p) = problems.into_iter().find(|p| p.folder == id) {
            tracing::error!(target: "core::machine", "installing {id} failed: {} {}", p.problem.message, p.problem.hint);
            let rolled_back = swapped.roll_back();
            self.reload_apps();
            rolled_back?;
            return Err(MachineError::new(
                ErrorKind::Unprocessable,
                "package-not-loadable",
                p.problem.message,
                p.problem.hint,
            ));
        }
        let installed = swapped.keep(&now_rfc3339(), source)?;
        tracing::info!(target: "core::machine", "installed the app {id} {} from {source}", installed.version);
        self.announce_packages();
        self.app_status.touch();
        let st = self.state.lock().expect("machine lock");
        let view = self.package_view(&st, installed);
        Ok((preview_of, Some(view)))
    }

    /// Uninstalls a package. Refused while a part uses its connector (naming the parts), or
    /// while an app is turned on: turn it off first, so nothing it drives stops by surprise.
    pub fn uninstall_package(&self, id: &str) -> Result<(), MachineError> {
        let _one_at_a_time = self.package_lock.lock().expect("package lock");
        if self.package_store().get(id)?.is_some_and(|p| p.kind == APP_TYPE) {
            if self.app_enabled(id) {
                return Err(MachineError::new(
                    ErrorKind::Conflict,
                    "app-enabled",
                    format!("The app \"{id}\" is turned on."),
                    format!("Turn it off first (POST /api/v0/apps/{id}/disable), then uninstall it."),
                ));
            }
            self.drop_app_state(id);
            self.package_store().uninstall(id)?;
            tracing::info!(target: "core::machine", "uninstalled the app {id}");
            self.forget_app(id);
            self.reload_apps();
            self.announce_packages();
            return Ok(());
        }
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

    /// What a backup holds of the apps: `config/apps.toml`, the apps folder, and the installed
    /// apps whose folder is there.
    pub(super) fn apps_to_back_up(&self) -> Result<(PathBuf, PathBuf, Vec<String>), MachineError> {
        let store = self.package_store();
        let ids = store
            .list()?
            .into_iter()
            .filter(|p| p.kind == APP_TYPE)
            .map(|p| p.id)
            .filter(|id| store.apps_dir().join(id).is_dir())
            .collect();
        Ok((self.apps_toml_path(), store.apps_dir().to_path_buf(), ids))
    }

    /// Step 1 of a restore, for the apps: each one in the backup is checked and loaded in staging
    /// exactly like installing its package (its logic compiled in the sandbox, its UI checked),
    /// and its saved state is checked as the app reads it. The first that fails fails the restore
    /// (`backup-invalid`, naming it and why), and nothing is left staged.
    pub(super) fn stage_backup_apps(&self, archive: &mut Archive) -> Result<Vec<StagedApp>, MachineError> {
        let ids = archive.package_ids(Folder::Apps);
        let connectors = archive.connector_ids();
        if let Some(both) = ids.iter().find(|id| connectors.contains(id)) {
            return Err(backup_invalid_package(
                "app",
                both,
                Problem::new("The backup holds a connector with the same id.", pick_a_faber_backup()),
            ));
        }
        let listed = archive.packages_toml().map(packages::parse_list).transpose();
        let store = self.package_store();
        let mut staged: Vec<StagedApp> = Vec::new();
        for id in ids {
            match self.stage_backup_app(&store, archive, &id, &listed) {
                Ok(s) => staged.push(s),
                Err(problem) => {
                    staged.into_iter().for_each(StagedApp::discard);
                    return Err(backup_invalid_package("app", &id, problem));
                }
            }
        }
        Ok(staged)
    }

    fn stage_backup_app(
        &self,
        store: &Store,
        archive: &mut Archive,
        id: &str,
        listed: &Result<Option<BTreeMap<String, Installed>>, PackageError>,
    ) -> Result<StagedApp, Problem> {
        let problem = |e: PackageError| Problem::new(e.message, e.hint);
        let listed = listed.as_ref().map_err(|e| problem(e.clone()))?;
        let Some(entry) = listed.as_ref().and_then(|l| l.get(id)).filter(|e| e.kind == APP_TYPE) else {
            return Err(Problem::new(
                "The backup's config/packages.toml doesn't list it as an installed app.",
                pick_a_faber_backup(),
            ));
        };
        let mut files = archive
            .package_files(Folder::Apps, id, packages::MAX_BYTES)
            .map_err(|e| Problem::new(e.message, e.hint))?;
        // The state is the app's data, not part of its package: it is checked on its own.
        let state = files.remove(crate::apps::STATE_FILE);
        if let Some(bytes) = &state {
            check_app_state(bytes)?;
        }
        let opened = packages::from_files(files).map_err(problem)?;
        if opened.header.id != id || opened.header.kind != APP_TYPE {
            return Err(Problem::new(
                format!("Its folder holds the {} \"{}\".", opened.header.kind, opened.header.id),
                pick_a_faber_backup(),
            ));
        }
        let shown = ShownAs::installed_app(id);
        let prepared = store
            .prepare(opened, |folder| {
                crate::apps::load_folder(folder, &shown, crate::sandbox::Limits::default())
            })
            .map_err(problem)?;
        Ok(StagedApp { prepared, source: entry.source.clone(), state })
    }

    /// Step 2 of a restore, for the apps: each staged one is put in place like an install, with
    /// its runs stopped meanwhile. One installed already at the same version is left as it is;
    /// another version replaces the installed one. Then each gets the state it had saved (an app
    /// whose backup has no state keeps the one it has). If one doesn't load once in place, all
    /// are put back and nothing else changes.
    pub(super) fn restore_apps(&self, staged: Vec<StagedApp>) -> Result<(), MachineError> {
        if staged.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = staged.iter().map(|s| s.prepared.header().id.clone()).collect();
        let result = self.with_apps_stopped(&ids, || self.put_apps_in_place(staged));
        self.announce_packages();
        result
    }

    fn put_apps_in_place(&self, staged: Vec<StagedApp>) -> Result<(), MachineError> {
        let store = self.package_store();
        let undo = |swapped: Vec<(String, Swapped, String)>| {
            for (id, s, _) in swapped.into_iter().rev() {
                if let Err(e) = s.roll_back() {
                    tracing::error!(target: "core::backup", "couldn't put apps/{id} back: {} {}", e.message, e.hint);
                }
            }
        };
        let mut swapped = Vec::new();
        let mut states = Vec::new();
        let mut rest = staged.into_iter();
        while let Some(s) = rest.next() {
            let (id, version) = (s.prepared.header().id.clone(), s.prepared.header().version.clone());
            if let Some(state) = s.state {
                states.push((id.clone(), state));
            }
            let same =
                store.get(&id).ok().flatten().is_some_and(|i| i.version == version && i.kind == APP_TYPE);
            if same {
                tracing::info!(target: "core::backup", "the backup's app {id} {version} is installed already");
                s.prepared.discard();
                continue;
            }
            match s.prepared.swap_in() {
                Ok(done) => swapped.push((id, done, s.source)),
                Err(e) => {
                    rest.for_each(StagedApp::discard);
                    undo(swapped);
                    return Err(e.into());
                }
            }
        }
        // Each must load once in place, exactly as the core will load it.
        let (_, problems) = crate::apps::load_installed(store.apps_dir(), crate::sandbox::Limits::default());
        let failed = swapped.iter().find_map(|(id, _, _)| {
            problems.iter().find(|p| &p.folder == id).map(|p| (id.clone(), p.problem.clone()))
        });
        if let Some((id, problem)) = failed {
            undo(swapped);
            return Err(backup_invalid_package("app", &id, problem));
        }
        let at = now_rfc3339();
        for (id, done, source) in swapped {
            let installed = done.keep(&at, &source)?;
            tracing::info!(target: "core::backup", "restored the app {id} {}", installed.version);
        }
        for (id, state) in states {
            let path = store.apps_dir().join(&id).join(crate::apps::STATE_FILE);
            let text = String::from_utf8(state).unwrap_or_default(); // checked as text when staged
            super::write_atomically(&path, &text).map_err(|e| {
                MachineError::new(
                    ErrorKind::Internal,
                    "io-error",
                    format!("Couldn't restore the saved state of the app \"{id}\": {e}"),
                    "Check that the disk has space and isn't failing.",
                )
            })?;
        }
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
            self.end_part(st, &name, EndReason::PartRestarted);
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
