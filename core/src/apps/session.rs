//! What a running app can do, by role name only (docs/apps.md §4): read its roles' signals,
//! drive the roles it may command under leases the core holds for it, publish its derived
//! signals, keep a small state and log. The sandbox (`logic`) hands every import to this; it is
//! plain Rust, so it is tested without wasm.

use super::manifest::{PublishSpec, RoleSpec};
use super::state::StateStore;
use crate::names::{PartName, SignalAddress};
use crate::router::{PublishError, Router};
use faber_capabilities::{Command, Reading};
use faber_connector::Problem;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Why a lease or a command was refused, as the app sees it (`faber:app/drive`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveError {
    /// No role with this name and index that the app may drive.
    NoSuchRole,
    /// Someone else drives the part.
    Held(String),
    LeaseRequired,
    /// The lease ended; the reason as the `leases` topic says it (`taken-over`, …).
    LeaseEnded(String),
    EstopEngaged,
    BadCommand(String),
    PartNotReady(String),
}

/// Leases and commands on parts: the machine owns the lease table, so it does them. The app's
/// session only ever asks for the parts filling its own command roles.
pub trait Driver: Send + Sync {
    /// Takes the lease on `part` for `holder`; answers the lease's id.
    fn acquire(&self, part: &PartName, holder: &str) -> Result<String, DriveError>;
    fn renew(&self, part: &PartName, lease: &str) -> Result<(), DriveError>;
    fn release(&self, part: &PartName, lease: &str);
    /// Checks, clamps and applies a command under `lease`; answers it as applied.
    fn command(&self, part: &PartName, lease: &str, command: Command) -> Result<Command, DriveError>;
}

/// One part filling a role, and the signal of it the app reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundPart {
    pub part: PartName,
    pub signal: SignalAddress,
}

/// A role from the manifest and the parts the user picked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundRole {
    pub spec: RoleSpec,
    pub parts: Vec<BoundPart>,
}

/// One role as the app sees it: never the parts' names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleView {
    pub name: String,
    pub capability: String,
    pub command: bool,
    pub parts: u32,
}

/// One run of one app.
pub struct Session {
    app: String,
    holder: String,
    roles: Vec<BoundRole>,
    publishes: Vec<PublishSpec>,
    router: Arc<Router>,
    driver: Arc<dyn Driver>,
    /// (role, index) → (part, lease id), for the leases this run holds.
    leases: BTreeMap<(String, u32), (PartName, String)>,
    /// (role, index) → the reading `poll` last handed out.
    polled: BTreeMap<(String, u32), Arc<Reading>>,
    pub state: StateStore,
    /// A rule the app broke during a call; it stops the app.
    bug: Option<Problem>,
}

/// What a lease holder is called for an app: `app:<id>`.
pub fn holder(app: &str) -> String {
    format!("app:{app}")
}

impl Session {
    pub fn new(
        app: &str,
        roles: Vec<BoundRole>,
        publishes: Vec<PublishSpec>,
        router: Arc<Router>,
        driver: Arc<dyn Driver>,
        state: StateStore,
    ) -> Self {
        Session {
            app: app.to_string(),
            holder: holder(app),
            roles,
            publishes,
            router,
            driver,
            leases: BTreeMap::new(),
            polled: BTreeMap::new(),
            state,
            bug: None,
        }
    }

    pub fn app(&self) -> &str {
        &self.app
    }

    /// The rule the app broke during the last call, if any.
    pub fn take_bug(&mut self) -> Option<Problem> {
        self.bug.take()
    }

    pub fn roles(&self) -> Vec<RoleView> {
        self.roles
            .iter()
            .map(|r| RoleView {
                name: r.spec.role.clone(),
                capability: r.spec.capability.id().to_string(),
                command: r.spec.command,
                parts: r.parts.len() as u32,
            })
            .collect()
    }

    fn bound(&self, role: &str, index: u32) -> Option<(&BoundRole, &BoundPart)> {
        let r = self.roles.iter().find(|r| r.spec.role == role)?;
        Some((r, r.parts.get(index as usize)?))
    }

    fn newest(&self, role: &str, index: u32) -> Option<Arc<Reading>> {
        let (_, p) = self.bound(role, index)?;
        self.router.latest(&p.signal).flatten()
    }

    pub fn latest(&self, role: &str, index: u32) -> Option<Reading> {
        self.newest(role, index).map(|r| (*r).clone())
    }

    /// The newest reading, only if `poll` hasn't handed it out before.
    pub fn poll(&mut self, role: &str, index: u32) -> Option<Reading> {
        let newest = self.newest(role, index)?;
        let key = (role.to_string(), index);
        if self.polled.get(&key).is_some_and(|seen| Arc::ptr_eq(seen, &newest)) {
            return None;
        }
        self.polled.insert(key, Arc::clone(&newest));
        Some((*newest).clone())
    }

    /// The part behind a role the app may drive.
    fn drivable(&self, role: &str, index: u32) -> Result<PartName, DriveError> {
        match self.bound(role, index) {
            Some((r, p)) if r.spec.command => Ok(p.part.clone()),
            _ => Err(DriveError::NoSuchRole),
        }
    }

    pub fn acquire(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        let part = self.drivable(role, index)?;
        let key = (role.to_string(), index);
        if let Some((_, lease)) = self.leases.get(&key) {
            match self.driver.renew(&part, lease) {
                Ok(()) => return Ok(()),
                // It ended meanwhile: ask for a new one.
                Err(_) => {
                    self.leases.remove(&key);
                }
            }
        }
        let lease = self.driver.acquire(&part, &self.holder)?;
        self.leases.insert(key, (part, lease));
        Ok(())
    }

    pub fn renew(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        self.drivable(role, index)?;
        let key = (role.to_string(), index);
        let (part, lease) = self.leases.get(&key).cloned().ok_or(DriveError::LeaseRequired)?;
        self.driver.renew(&part, &lease).inspect_err(|_| {
            self.leases.remove(&key);
        })
    }

    pub fn release(&mut self, role: &str, index: u32) {
        if let Some((part, lease)) = self.leases.remove(&(role.to_string(), index)) {
            self.driver.release(&part, &lease);
        }
    }

    pub fn send(&mut self, role: &str, index: u32, command: Command) -> Result<Command, DriveError> {
        self.drivable(role, index)?;
        let key = (role.to_string(), index);
        let (part, lease) = self.leases.get(&key).cloned().ok_or(DriveError::LeaseRequired)?;
        self.driver.command(&part, &lease, command).inspect_err(|e| {
            if matches!(e, DriveError::LeaseEnded(_) | DriveError::LeaseRequired | DriveError::EstopEngaged) {
                self.leases.remove(&key);
            }
        })
    }

    /// Ends every lease this run holds (it stopped, trapped or was turned off).
    pub fn release_all(&mut self) {
        for ((_, _), (part, lease)) in std::mem::take(&mut self.leases) {
            self.driver.release(&part, &lease);
        }
    }

    /// Publishes a derived signal. Only `[[publishes]]` names with their capability; anything
    /// else is a bug that stops the app.
    pub fn publish(&mut self, name: &str, reading: Reading) {
        let Some(spec) = self.publishes.iter().find(|p| p.name == name) else {
            self.bug.get_or_insert_with(|| {
                bug(format!("it published \"{name}\", which its manifest's [[publishes]] doesn't declare"))
            });
            return;
        };
        if reading.capability() != spec.capability {
            let (got, want) = (reading.capability().id(), spec.capability.id());
            self.bug
                .get_or_insert_with(|| bug(format!("it published {got} on \"{name}\", which is a {want}")));
            return;
        }
        match self.router.publish(&SignalAddress::derived(&self.app, name), reading) {
            Ok(()) => {}
            // Declared when the app starts; gone only while it is being stopped.
            Err(PublishError::Undeclared | PublishError::WrongCapability { .. }) => {}
        }
    }
}

fn bug(what: String) -> Problem {
    Problem::new(
        format!("The app stopped because of a bug: {what}."),
        "Faber restarts it automatically. If this keeps happening, report it to the app's author.",
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::apps::manifest::Count;
    use faber_capabilities::{Capability, Twist2d, distance, planar_base};
    use std::sync::Mutex;

    /// Plays the machine: one lease per part, and a log of what happened.
    #[derive(Default)]
    pub struct FakeDriver {
        pub log: Mutex<Vec<String>>,
        /// When set, every command answers this.
        pub refuse: Mutex<Option<DriveError>>,
    }

    impl Driver for FakeDriver {
        fn acquire(&self, part: &PartName, holder: &str) -> Result<String, DriveError> {
            self.log.lock().unwrap().push(format!("acquire {part} as {holder}"));
            Ok(format!("lease-{part}"))
        }
        fn renew(&self, part: &PartName, _: &str) -> Result<(), DriveError> {
            self.log.lock().unwrap().push(format!("renew {part}"));
            Ok(())
        }
        fn release(&self, part: &PartName, lease: &str) {
            self.log.lock().unwrap().push(format!("release {part} {lease}"));
        }
        fn command(&self, part: &PartName, _: &str, c: Command) -> Result<Command, DriveError> {
            if let Some(e) = self.refuse.lock().unwrap().clone() {
                return Err(e);
            }
            self.log.lock().unwrap().push(format!("command {part} {}", c.kind()));
            Ok(c)
        }
    }

    pub fn p(s: &str) -> PartName {
        PartName::parse_any(s).unwrap()
    }

    pub fn role(name: &str, capability: Capability, command: bool, parts: &[&str]) -> BoundRole {
        BoundRole {
            spec: RoleSpec { role: name.into(), capability, command, count: Count::One },
            parts: parts
                .iter()
                .map(|n| BoundPart { part: p(n), signal: SignalAddress::new(p(n), capability.name()) })
                .collect(),
        }
    }

    fn session() -> (Session, Arc<Router>, Arc<FakeDriver>) {
        let router = Arc::new(Router::new());
        let driver = Arc::new(FakeDriver::default());
        let roles = vec![
            role("front", Capability::Distance, false, &["front_ir"]),
            role("base", Capability::PlanarBase, true, &["base"]),
        ];
        let publishes = vec![PublishSpec { name: "wall-distance".into(), capability: Capability::Distance }];
        let s = Session::new(
            "wall-follower",
            roles,
            publishes,
            Arc::clone(&router),
            driver.clone(),
            StateStore::in_memory(),
        );
        (s, router, driver)
    }

    fn dist(t_us: u64, meters: f32) -> Reading {
        Reading::Distance(distance::Reading { t_us, meters, status: distance::Status::Ok })
    }

    #[test]
    fn roles_are_seen_by_name_only() {
        let (s, _, _) = session();
        let views = s.roles();
        assert_eq!(
            views[0],
            RoleView { name: "front".into(), capability: "distance@1".into(), command: false, parts: 1 }
        );
        assert!(views[1].command);
    }

    #[test]
    fn latest_reads_the_part_and_poll_hands_each_reading_out_once() {
        let (mut s, router, _) = session();
        let addr = SignalAddress::new(p("front_ir"), "distance");
        assert_eq!(s.latest("front", 0), None, "no signal yet");
        router.declare(addr.clone(), Capability::Distance);
        assert_eq!(s.poll("front", 0), None, "nothing published yet");
        router.publish(&addr, dist(1, 0.5)).unwrap();
        assert_eq!(s.poll("front", 0), Some(dist(1, 0.5)));
        assert_eq!(s.poll("front", 0), None);
        assert_eq!(s.latest("front", 0), Some(dist(1, 0.5)));
        router.publish(&addr, dist(2, 0.5)).unwrap();
        assert_eq!(s.poll("front", 0), Some(dist(2, 0.5)));
        assert_eq!((s.latest("front", 1), s.latest("nope", 0)), (None, None));
    }

    #[test]
    fn only_command_roles_are_driven_and_only_under_a_lease() {
        let (mut s, _, driver) = session();
        let go = Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.3, 0.0, 0.0)));
        assert_eq!(s.acquire("front", 0), Err(DriveError::NoSuchRole), "a sensor role");
        assert_eq!(s.acquire("base", 1), Err(DriveError::NoSuchRole), "no second part");
        assert_eq!(s.send("base", 0, go), Err(DriveError::LeaseRequired));
        s.acquire("base", 0).unwrap();
        s.acquire("base", 0).unwrap();
        assert_eq!(s.send("base", 0, go), Ok(go));
        *driver.refuse.lock().unwrap() = Some(DriveError::LeaseEnded("taken-over".into()));
        assert_eq!(s.send("base", 0, go), Err(DriveError::LeaseEnded("taken-over".into())));
        *driver.refuse.lock().unwrap() = None;
        assert_eq!(s.send("base", 0, go), Err(DriveError::LeaseRequired), "the ended lease is forgotten");
        s.acquire("base", 0).unwrap();
        s.release_all();
        assert_eq!(
            *driver.log.lock().unwrap(),
            [
                "acquire base as app:wall-follower",
                "renew base",
                "command base set-velocity",
                "acquire base as app:wall-follower",
                "release base lease-base"
            ]
        );
    }

    #[test]
    fn publishing_is_checked_against_the_manifest() {
        let (mut s, router, _) = session();
        let addr = SignalAddress::derived("wall-follower", "wall-distance");
        router.declare(addr.clone(), Capability::Distance);
        s.publish("wall-distance", dist(5, 0.3));
        assert_eq!(router.latest(&addr), Some(Some(Arc::new(dist(5, 0.3)))));
        assert!(s.take_bug().is_none());
        s.publish("elsewhere", dist(6, 0.3));
        let b = s.take_bug().unwrap();
        assert!(b.message.contains("\"elsewhere\""), "{b:?}");
        let pose = Reading::PlanarBase(planar_base::Reading::default());
        s.publish("wall-distance", pose);
        assert!(s.take_bug().unwrap().message.contains("planar-base@1 on \"wall-distance\""));
    }
}
