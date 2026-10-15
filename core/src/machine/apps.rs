//! Apps on the machine (docs/apps.md): which are installed, which are on, which parts fill
//! their roles (`config/apps.toml`), and running their logic.
//!
//! The machine reconciles the apps like the parts: an app runs when it is on and every role is
//! filled by a part that provides the role's capability. A role without a part, or a part that
//! was removed or can't fill the role any more, stops it with a status that says so. An app
//! drives its command roles under leases held as `app:<id>`, like any other lease holder, so the
//! e-stop, the dead man's switch and operators taking over work for it exactly as for the Drive
//! window. The D46 confirm before driving is the shell's; for an app, turning it on is the
//! confirmation.
//!
//! Locks: the apps lock is taken before the machine's state lock, never after it. App threads
//! only ever take the state lock (through the [`Driver`]), so stopping an app (which waits for
//! its thread) is safe while holding the apps lock, but never while holding the state lock.

use super::safety::provides_of;
use super::{ErrorKind, Machine, MachineError, State, write_atomically};
use crate::apps::manifest::{AppManifest, Count};
use crate::apps::{
    self, AppDefinition, AppHandle, AppStatus, BoundPart, BoundRole, DriveError, Driver, RunDeps,
};
use crate::connectors::LoadProblem;
use crate::names::{PartName, SignalAddress};
use crate::packages;
use crate::roles::{self, AppsConfig, RolesError};
use faber_capabilities::Command;
use faber_connector::Problem;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

/// The apps the machine knows, and the ones running.
#[derive(Default)]
pub(super) struct Apps {
    definitions: Vec<AppDefinition>,
    /// Installed apps that couldn't be loaded, and why.
    problems: Vec<LoadProblem>,
    /// Bumped whenever `<data>/apps/` is read again, so an updated app restarts.
    generation: u64,
    running: BTreeMap<String, Running>,
}

/// An app that is on, with what it runs with. A UI-only app has no thread.
struct Running {
    handle: Option<AppHandle>,
    key: (u64, Vec<BoundRole>),
}

impl Running {
    /// Stops the app's logic and waits for it: its leases have ended when this returns.
    fn stop(self) {
        if let Some(h) = self.handle {
            h.stop();
        }
    }
}

/// An app as the API shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct AppView {
    pub manifest: AppManifest,
    pub status: AppStatus,
    pub enabled: bool,
    /// Role → the parts picked for it.
    pub bindings: BTreeMap<String, Vec<PartName>>,
    /// Its package has its own icon (`icon.svg` at its root).
    pub has_icon: bool,
}

/// What the machine wants for one app now.
enum Desired {
    Run(Vec<BoundRole>),
    Not(AppStatus),
}

fn off() -> AppStatus {
    AppStatus::Stopped(Problem::new("The app is turned off.", "Turn it on to start it."))
}

fn roles_problem(e: &RolesError) -> Problem {
    let place = e.line.map(|l| format!(" on line {l}")).unwrap_or_default();
    Problem::new(format!("config/apps.toml has a problem{place}: {}", e.message), e.hint.clone())
}

fn unknown_app(id: &str) -> MachineError {
    MachineError::new(
        ErrorKind::NotFound,
        "unknown-app",
        format!("There is no app called \"{id}\"."),
        "GET /api/v0/apps lists the installed apps; install one from its .faberpkg first.",
    )
}

/// The machine, as a running app's sessions reach it.
struct MachineDriver(Weak<Machine>);

impl MachineDriver {
    fn machine(&self) -> Result<Arc<Machine>, DriveError> {
        self.0.upgrade().ok_or_else(|| DriveError::PartNotReady("Faber is stopping.".into()))
    }
}

fn drive_error(m: &Machine, part: &PartName, e: MachineError) -> DriveError {
    match e.code {
        "lease-held" => {
            let who = m.leases().into_iter().find(|l| l.parts().any(|p| p == part)).map(|l| l.holder);
            DriveError::Held(who.unwrap_or(e.message))
        }
        "estop-engaged" => DriveError::EstopEngaged,
        "lease-ended" => DriveError::LeaseEnded(e.lease_ended.map_or("ended", |r| r.id()).to_string()),
        "lease-required" => DriveError::LeaseRequired,
        "bad-command" | "not-leasable" => DriveError::BadCommand(e.message),
        _ => DriveError::PartNotReady(e.message),
    }
}

impl Driver for MachineDriver {
    fn acquire(&self, part: &PartName, holder: &str) -> Result<String, DriveError> {
        let m = self.machine()?;
        m.request_lease(part.as_str(), holder, false).map(|l| l.id).map_err(|e| drive_error(&m, part, e))
    }

    fn renew(&self, part: &PartName, lease: &str) -> Result<(), DriveError> {
        let m = self.machine()?;
        m.renew_lease(part.as_str(), lease).map(|_| ()).map_err(|e| drive_error(&m, part, e))
    }

    fn release(&self, part: &PartName, lease: &str) {
        if let Ok(m) = self.machine() {
            // Already ended (taken over, expired, …) is fine: it is released either way.
            let _ = m.release_lease(part.as_str(), lease);
        }
    }

    fn command(&self, part: &PartName, lease: &str, command: Command) -> Result<Command, DriveError> {
        let m = self.machine()?;
        m.command(part.as_str(), lease, command).map(|r| r.applied).map_err(|e| drive_error(&m, part, e))
    }
}

impl Machine {
    fn apps_toml(&self) -> PathBuf {
        self.data_dir.join("config").join(roles::FILE_NAME)
    }

    fn apps_dir(&self) -> PathBuf {
        packages::Store::new(&self.data_dir).apps_dir().to_path_buf()
    }

    /// `config/apps.toml` as text (a new file's header when there is none) and as read.
    fn read_apps_config(&self) -> (String, Result<AppsConfig, RolesError>) {
        let text =
            std::fs::read_to_string(self.apps_toml()).unwrap_or_else(|_| roles::NEW_FILE_HEADER.to_string());
        let parsed = roles::parse(&text);
        (text, parsed)
    }

    fn write_apps_config(&self, text: &str) -> Result<(), MachineError> {
        roles::parse(text).map_err(|e| {
            MachineError::new(ErrorKind::Internal, "config-not-saved", roles_problem(&e).message, e.hint)
        })?;
        write_atomically(&self.apps_toml(), text).map_err(|e| {
            MachineError::new(
                ErrorKind::Internal,
                "config-not-saved",
                format!("Could not save config/apps.toml: {e}"),
                "Check that the data folder exists and is writable, and that the disk isn't full.",
            )
        })?;
        self.app_status.touch();
        Ok(())
    }

    /// Fires whenever an app's status, settings or the list of apps changes.
    pub fn apps_changed(&self) -> tokio::sync::watch::Receiver<u64> {
        self.app_status.watch()
    }

    /// Installed apps that couldn't be loaded, and why.
    pub fn app_problems(&self) -> Vec<LoadProblem> {
        self.apps.lock().expect("apps lock").problems.clone()
    }

    /// Every installed app, by id.
    pub fn apps(&self) -> Vec<AppView> {
        let apps = self.apps.lock().expect("apps lock");
        let config = self.read_apps_config().1.unwrap_or_default();
        apps.definitions.iter().map(|d| self.app_view(d, &config)).collect()
    }

    pub fn app(&self, id: &str) -> Result<AppView, MachineError> {
        self.apps().into_iter().find(|a| a.manifest.id == id).ok_or_else(|| unknown_app(id))
    }

    fn app_view(&self, d: &AppDefinition, config: &AppsConfig) -> AppView {
        let entry = config.entry(&d.manifest.id);
        AppView {
            manifest: d.manifest.clone(),
            status: self.app_status.get(&d.manifest.id).unwrap_or(AppStatus::Starting),
            enabled: entry.enabled,
            bindings: entry.roles,
            has_icon: d.folder.join(packages::ICON).is_file(),
        }
    }

    fn known_app(&self, id: &str) -> Result<AppDefinition, MachineError> {
        let apps = self.apps.lock().expect("apps lock");
        apps.definitions.iter().find(|d| d.manifest.id == id).cloned().ok_or_else(|| unknown_app(id))
    }

    /// Turns an app on or off (`POST /api/v0/apps/{id}/enable` and `/disable`). Turning it on is
    /// the user's go-ahead for it to drive its command roles. Off ends its leases at once.
    pub fn set_app_enabled(&self, id: &str, enabled: bool) -> Result<AppView, MachineError> {
        self.known_app(id)?;
        let (text, parsed) = self.read_apps_config();
        parsed.map_err(|e| {
            let p = roles_problem(&e);
            MachineError::new(ErrorKind::Unprocessable, "config-broken", p.message, p.hint)
        })?;
        let text = roles::set_enabled(&text, id, enabled)
            .map_err(|e| MachineError::new(ErrorKind::Internal, "config-not-saved", e.message, e.hint))?;
        self.write_apps_config(&text)?;
        tracing::info!(target: "core::apps", app = id, "turned {}", if enabled { "on" } else { "off" });
        self.reconcile_apps();
        self.app(id)
    }

    /// Picks the parts for an app's roles (`PUT /api/v0/apps/{id}/roles`): exactly these roles;
    /// any left out have no part. Each part must exist and provide the role's capability.
    pub fn set_app_roles(
        &self,
        id: &str,
        wanted: BTreeMap<String, Vec<String>>,
    ) -> Result<AppView, MachineError> {
        let def = self.known_app(id)?;
        let m = &def.manifest;
        let bad = |code: &'static str, message: String, hint: &str| {
            MachineError::new(ErrorKind::BadRequest, code, message, hint)
        };
        let mut roles: BTreeMap<String, Vec<PartName>> = BTreeMap::new();
        {
            let st = self.state.lock().expect("machine lock");
            for (role, parts) in &wanted {
                let Some(spec) = m.role(role) else {
                    let names: Vec<&str> = m.requires.iter().map(|r| r.role.as_str()).collect();
                    return Err(bad(
                        "unknown-role",
                        format!("{} has no role called \"{role}\".", m.name),
                        &format!(
                            "Its roles are: {}.",
                            if names.is_empty() { "none".into() } else { names.join(", ") }
                        ),
                    ));
                };
                if spec.count == Count::One && parts.len() > 1 {
                    return Err(bad(
                        "too-many-parts",
                        format!("The role \"{role}\" takes one part, not {}.", parts.len()),
                        "Pick one part for it.",
                    ));
                }
                let mut names = Vec::new();
                for part in parts {
                    let name = Self::known_part(&st, part).map_err(|_| {
                        MachineError::new(
                            ErrorKind::Unprocessable,
                            "unknown-part",
                            format!("There is no part called \"{part}\" to fill the role \"{role}\"."),
                            "Pick a part from the parts list, or add the part first.",
                        )
                    })?;
                    if names.contains(&name) {
                        return Err(bad(
                            "bad-request",
                            format!("\"{part}\" is picked twice for the role \"{role}\"."),
                            "Pick each part once.",
                        ));
                    }
                    if !provides_of(&st, &name).iter().any(|p| p.capability == spec.capability) {
                        let has: Vec<&str> =
                            provides_of(&st, &name).iter().map(|p| p.capability.id()).collect();
                        return Err(MachineError::new(
                            ErrorKind::Unprocessable,
                            "role-mismatch",
                            format!(
                                "\"{name}\" can't fill the role \"{role}\": it needs a {}, and \"{name}\" provides {}.",
                                spec.capability.id(),
                                if has.is_empty() { "nothing right now".into() } else { has.join(", ") }
                            ),
                            format!("Pick a part that provides {}.", spec.capability.id()),
                        ));
                    }
                    names.push(name);
                }
                if !names.is_empty() {
                    roles.insert(role.clone(), names);
                }
            }
        }
        let lists: Vec<&str> =
            m.requires.iter().filter(|r| r.count == Count::OneOrMore).map(|r| r.role.as_str()).collect();
        let (text, parsed) = self.read_apps_config();
        parsed.map_err(|e| {
            let p = roles_problem(&e);
            MachineError::new(ErrorKind::Unprocessable, "config-broken", p.message, p.hint)
        })?;
        let text = roles::set_roles(&text, id, &roles, &lists)
            .map_err(|e| MachineError::new(ErrorKind::Internal, "config-not-saved", e.message, e.hint))?;
        self.write_apps_config(&text)?;
        self.reconcile_apps();
        self.app(id)
    }

    /// Reads `<data>/apps/` again (after an install or uninstall); apps that changed restart.
    pub(super) fn reload_apps(&self) -> Vec<LoadProblem> {
        let (defs, problems) = apps::load_installed(&self.apps_dir(), crate::sandbox::Limits::default());
        {
            let mut a = self.apps.lock().expect("apps lock");
            a.definitions = defs;
            a.problems = problems.clone();
            a.generation += 1;
        }
        self.reconcile_apps();
        problems
    }

    /// Runs `change` with the apps in `ids` stopped and no app started or stopped meanwhile (it
    /// holds the apps lock), then reads `<data>/apps/` again and reconciles. A restore changes an
    /// app's folder and its saved state this way: an app saves its state when its run ends, so its
    /// run must have ended before the restored state is written.
    pub(super) fn with_apps_stopped<T>(&self, ids: &[String], change: impl FnOnce() -> T) -> T {
        let out = {
            let mut a = self.apps.lock().expect("apps lock");
            for id in ids {
                if let Some(r) = a.running.remove(id) {
                    r.stop();
                }
            }
            let out = change();
            let (defs, problems) = apps::load_installed(&self.apps_dir(), crate::sandbox::Limits::default());
            a.definitions = defs;
            a.problems = problems;
            a.generation += 1;
            out
        };
        self.reconcile_apps();
        out
    }

    /// Replaces `config/apps.toml` with a backup's, which the caller checked, and reconciles.
    pub(super) fn restore_apps_config(&self, text: &str) -> Result<(), MachineError> {
        self.write_apps_config(text)?;
        self.reconcile_apps();
        Ok(())
    }

    /// Where `config/apps.toml` is, for a backup.
    pub(super) fn apps_toml_path(&self) -> PathBuf {
        self.apps_toml()
    }

    /// Whether an installed app is turned on (it can't be uninstalled then).
    pub(super) fn app_enabled(&self, id: &str) -> bool {
        self.read_apps_config().1.is_ok_and(|c| c.entry(id).enabled)
    }

    /// Forgets an uninstalled app's line in apps.toml.
    pub(super) fn forget_app(&self, id: &str) {
        let (text, _) = self.read_apps_config();
        if let Ok(new) = roles::remove_app(&text, id)
            && new != text
            && let Err(e) = self.write_apps_config(&new)
        {
            tracing::warn!(target: "core::apps", "{} {}", e.message, e.hint);
        }
    }

    /// Makes the running apps match apps.toml and the parts: starts the apps that are on and
    /// have every role filled, stops the others with the reason, and restarts an app whose
    /// parts or package changed.
    pub(super) fn reconcile_apps(&self) {
        let mut apps = self.apps.lock().expect("apps lock");
        let parsed = self.read_apps_config().1;
        let desired: Vec<(AppDefinition, Desired)> = {
            let st = self.state.lock().expect("machine lock");
            apps.definitions.iter().map(|d| (d.clone(), desired(&st, d, &parsed))).collect()
        };
        let generation = apps.generation;
        let installed: Vec<String> = desired.iter().map(|(d, _)| d.manifest.id.clone()).collect();
        let gone: Vec<String> = apps.running.keys().filter(|id| !installed.contains(id)).cloned().collect();
        for id in gone {
            if let Some(r) = apps.running.remove(&id) {
                r.stop();
            }
            self.app_status.remove(&id);
        }
        for (def, want) in desired {
            let id = def.manifest.id.clone();
            match want {
                Desired::Run(roles) => {
                    let key = (generation, roles.clone());
                    if apps.running.get(&id).is_some_and(|r| r.key == key) {
                        continue;
                    }
                    if let Some(old) = apps.running.remove(&id) {
                        old.stop(); // the old run ends before the new one starts
                    }
                    let driver: Arc<dyn Driver> =
                        Arc::new(MachineDriver(self.me.get().cloned().unwrap_or_default()));
                    let handle = def.run_spec(roles, Arc::clone(&self.router), driver).map(|spec| {
                        tracing::info!(target: "core::apps", app = id.as_str(), "starting {} {}", def.manifest.name, def.manifest.version);
                        apps::start(spec, self.app_deps())
                    });
                    if handle.is_none() {
                        self.app_status.set(&id, AppStatus::Running); // a UI-only app is just on
                    }
                    apps.running.insert(id, Running { handle, key });
                }
                Desired::Not(status) => {
                    if let Some(r) = apps.running.remove(&id) {
                        r.stop();
                        tracing::info!(target: "core::apps", app = id.as_str(), "stopped");
                    }
                    self.app_status.set(&id, status);
                }
            }
        }
    }

    fn app_deps(&self) -> RunDeps {
        RunDeps {
            clock: Arc::clone(&self.clock),
            router: Arc::clone(&self.router),
            status: Arc::clone(&self.app_status),
            timing: apps::Timing {
                backoff_start: self.timing.backoff_start,
                max_crashes: self.timing.max_crashes,
                healthy_after: self.timing.healthy_after,
            },
        }
    }

    /// Stops every app (the core is stopping). Their leases end.
    pub(super) fn stop_apps(&self) {
        let running = std::mem::take(&mut self.apps.lock().expect("apps lock").running);
        for r in running.into_values() {
            r.stop();
        }
    }

    /// The icon at the root of an app's package (`icon.svg`), served as an image only.
    pub fn app_icon(&self, id: &str) -> Result<Vec<u8>, MachineError> {
        let def = self.known_app(id)?;
        std::fs::read(def.folder.join(packages::ICON)).map_err(|_| {
            MachineError::new(
                ErrorKind::NotFound,
                "not-found",
                format!("{} has no icon.svg.", def.manifest.name),
                "An app's own icon is icon.svg at the root of its package.",
            )
        })
    }

    /// A file of an app's UI, with the type it is served as. Only paths the installer accepts.
    pub fn app_ui_file(&self, id: &str, path: &str) -> Result<(Vec<u8>, &'static str), MachineError> {
        let def = self.known_app(id)?;
        let missing = || {
            MachineError::new(
                ErrorKind::NotFound,
                "not-found",
                format!("{} has no UI file \"{path}\".", def.manifest.name),
                "Its UI starts at ui/index.html.",
            )
        };
        if !def.manifest.ui || !packages::ui_path_ok(path) {
            return Err(missing());
        }
        let file: PathBuf = path.split('/').fold(def.folder.join("ui"), |p: PathBuf, s| p.join(s));
        let kind = packages::ui_content_type(path).ok_or_else(missing)?;
        std::fs::read(Path::new(&file)).map(|b| (b, kind)).map_err(|_| missing())
    }
}

/// What one app should be doing now, from apps.toml and the parts.
fn desired(st: &State, d: &AppDefinition, config: &Result<AppsConfig, RolesError>) -> Desired {
    let config = match config {
        Ok(c) => c,
        Err(e) => return Desired::Not(AppStatus::Invalid(roles_problem(e))),
    };
    let entry = config.entry(&d.manifest.id);
    if !entry.enabled {
        return Desired::Not(off());
    }
    let mut bound = Vec::new();
    for spec in &d.manifest.requires {
        let parts = entry.roles.get(&spec.role).cloned().unwrap_or_default();
        if parts.is_empty() {
            return Desired::Not(AppStatus::Invalid(Problem::new(
                format!("The role \"{}\" has no part, so the app can't run.", spec.role),
                format!("Pick a part that provides {} for it.", spec.capability.id()),
            )));
        }
        if spec.count == Count::One && parts.len() > 1 {
            return Desired::Not(AppStatus::Invalid(Problem::new(
                format!("The role \"{}\" takes one part, but apps.toml names {}.", spec.role, parts.len()),
                "Pick one part for it.",
            )));
        }
        let mut role_parts = Vec::new();
        for part in parts {
            if Machine::known_part(st, part.as_str()).is_err() {
                return Desired::Not(AppStatus::Conflict(Problem::new(
                    format!("The part \"{part}\" filling the role \"{}\" was removed.", spec.role),
                    "Pick another part for the role, or add the part again.",
                )));
            }
            let Some(provide) = provides_of(st, &part).into_iter().find(|p| p.capability == spec.capability)
            else {
                return Desired::Not(AppStatus::Conflict(Problem::new(
                    format!(
                        "The part \"{part}\" filling the role \"{}\" doesn't provide {} right now.",
                        spec.role,
                        spec.capability.id()
                    ),
                    "Check the part in Parts, or pick another part for the role.",
                )));
            };
            role_parts.push(BoundPart { signal: SignalAddress::new(part.clone(), provide.name), part });
        }
        bound.push(BoundRole { spec: spec.clone(), parts: role_parts });
    }
    Desired::Run(bound)
}
