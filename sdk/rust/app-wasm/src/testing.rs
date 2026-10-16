//! A [`Host`] for testing an app's logic on the PC, without wasm or a core: set the readings its
//! roles see and the messages its windows send, then look at what it commanded, published,
//! stored, sent its windows and logged.

use crate::{
    Command, DRIVE_ENDED, DRIVE_STARTED, DriveError, Host, LogLevel, Message, Mount, Payload, Reading, Role,
};
use std::collections::{BTreeMap, BTreeSet};

/// Plays the core for an app under test.
#[derive(Debug, Default)]
pub struct TestHost {
    pub roles: Vec<Role>,
    /// The newest reading of each `(role, index)`.
    pub readings: BTreeMap<(String, u32), Reading>,
    /// Where each `(role, index)` sits on the machine; a part of a role without one is at the
    /// origin, as in parts.toml.
    pub mounts: BTreeMap<(String, u32), Mount>,
    /// What `poll` already handed out, so it hands each reading out once.
    polled: BTreeSet<(String, u32)>,
    /// The roles the app holds a lease on.
    pub leases: BTreeSet<(String, u32)>,
    /// When set, `send` and `acquire` answer this instead (e.g. an operator took over).
    pub refuse: Option<DriveError>,
    /// Every command the app sent and the core accepted.
    pub sent: Vec<(String, u32, Command)>,
    pub published: Vec<(String, Reading)>,
    pub state: BTreeMap<String, String>,
    pub logged: Vec<(LogLevel, String)>,
    /// What the app's windows sent, for the next `poll_messages`.
    pub inbox: Vec<Message>,
    /// Every message the app sent its windows: to which (`None`: all), kind, payload.
    pub outbox: Vec<(Option<u64>, String, Payload)>,
    /// Roles declared `drive = "window"`: driven only while a window drives them, as the core
    /// enforces it (see [`TestHost::window_drives`]).
    pub window_driven: BTreeSet<String>,
    /// The `(role, index)`s a window drives now.
    pub driven: BTreeSet<(String, u32)>,
}

impl TestHost {
    /// A host whose roles are these.
    pub fn with_roles(roles: Vec<Role>) -> Self {
        TestHost { roles, ..TestHost::default() }
    }

    /// A new reading on a role's part; `poll` hands it out once.
    pub fn set_reading(&mut self, role: &str, index: u32, reading: Reading) {
        let key = (role.to_string(), index);
        self.polled.remove(&key);
        self.readings.insert(key, reading);
    }

    /// A window sends the app a text message.
    pub fn window_says(&mut self, window: u64, kind: &str, text: &str) {
        self.inbox.push(Message { window, kind: kind.into(), payload: Payload::Text(text.into()) });
    }

    /// A window starts driving `role` through the logic (`faber.driveVia`), as the core says it.
    pub fn window_drives(&mut self, window: u64, role: &str, index: u32) {
        self.driven.insert((role.to_string(), index));
        self.window_says(window, DRIVE_STARTED, &format!("{role} {index}"));
    }

    /// A window's drive ends (`stopped`, `heartbeat-lost`, …): as the core does, the app's lease
    /// on the role ends too, whatever the app does next.
    pub fn window_stops(&mut self, window: u64, role: &str, index: u32, reason: &str) {
        let key = (role.to_string(), index);
        self.driven.remove(&key);
        self.leases.remove(&key);
        self.window_says(window, DRIVE_ENDED, &format!("{role} {index} {reason}"));
    }

    fn may_drive(&self, role: &str, index: u32) -> bool {
        self.roles.iter().any(|r| r.name == role && r.command && index < r.parts)
    }

    fn window_allows(&self, role: &str, index: u32) -> bool {
        !self.window_driven.contains(role) || self.driven.contains(&(role.to_string(), index))
    }
}

impl Host for TestHost {
    fn roles(&mut self) -> Vec<Role> {
        self.roles.clone()
    }

    fn latest(&mut self, role: &str, index: u32) -> Option<Reading> {
        self.readings.get(&(role.to_string(), index)).cloned()
    }

    fn mount(&mut self, role: &str, index: u32) -> Option<Mount> {
        let filled = self.roles.iter().any(|r| r.name == role && index < r.parts);
        filled.then(|| self.mounts.get(&(role.to_string(), index)).copied().unwrap_or_default())
    }

    fn poll(&mut self, role: &str, index: u32) -> Option<Reading> {
        let key = (role.to_string(), index);
        let reading = self.readings.get(&key).cloned()?;
        self.polled.insert(key).then_some(reading)
    }

    fn acquire(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        if !self.may_drive(role, index) {
            return Err(DriveError::NoSuchRole);
        }
        if let Some(e) = &self.refuse {
            return Err(e.clone());
        }
        if !self.window_allows(role, index) {
            return Err(DriveError::PartNotReady(format!("no window is driving \"{role}\".")));
        }
        self.leases.insert((role.to_string(), index));
        Ok(())
    }

    fn renew(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        if self.leases.contains(&(role.to_string(), index)) { Ok(()) } else { Err(DriveError::LeaseRequired) }
    }

    fn release(&mut self, role: &str, index: u32) {
        self.leases.remove(&(role.to_string(), index));
    }

    fn send(&mut self, role: &str, index: u32, command: Command) -> Result<Command, DriveError> {
        if !self.may_drive(role, index) {
            return Err(DriveError::NoSuchRole);
        }
        if let Some(e) = &self.refuse {
            self.leases.remove(&(role.to_string(), index));
            return Err(e.clone());
        }
        if !self.leases.contains(&(role.to_string(), index)) {
            return Err(DriveError::LeaseRequired);
        }
        self.sent.push((role.to_string(), index, command));
        Ok(command)
    }

    fn publish(&mut self, name: &str, reading: Reading) {
        self.published.push((name.to_string(), reading));
    }

    fn state_get(&mut self, key: &str) -> Option<String> {
        self.state.get(key).cloned()
    }

    fn state_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.state.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn state_remove(&mut self, key: &str) {
        self.state.remove(key);
    }

    fn state_keys(&mut self) -> Vec<String> {
        self.state.keys().cloned().collect()
    }

    fn poll_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.inbox)
    }

    fn send_message(&mut self, window: Option<u64>, kind: &str, payload: Payload) -> Result<(), String> {
        self.outbox.push((window, kind.to_string(), payload));
        Ok(())
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        self.logged.push((level, message.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{Twist2d, distance, planar_base};

    fn base() -> Role {
        Role { name: "base".into(), capability: "planar-base@1".into(), command: true, parts: 1 }
    }

    #[test]
    fn mounts_are_answered_for_filled_roles_only() {
        let mut h = TestHost::with_roles(vec![base()]);
        h.mounts.insert(("base".into(), 0), Mount { x: 0.2, ..Mount::default() });
        assert_eq!(h.mount("base", 0).map(|m| m.x), Some(0.2));
        assert_eq!(h.mount("base", 1), None);
        assert_eq!(h.mount("lidar", 0), None);
    }

    #[test]
    fn poll_hands_each_reading_out_once() {
        let mut h = TestHost::default();
        let r = Reading::Distance(distance::Reading { t_us: 1, meters: 1.0, status: distance::Status::Ok });
        h.set_reading("front", 0, r.clone());
        assert_eq!(h.poll("front", 0), Some(r.clone()));
        assert_eq!(h.poll("front", 0), None);
        assert_eq!(h.latest("front", 0), Some(r.clone()));
        h.set_reading("front", 0, r.clone());
        assert_eq!(h.poll("front", 0), Some(r));
    }

    #[test]
    fn a_window_driven_role_needs_a_window_driving_it() {
        let mut h = TestHost::with_roles(vec![base()]);
        h.window_driven.insert("base".into());
        assert!(matches!(h.acquire("base", 0), Err(DriveError::PartNotReady(_))));
        h.window_drives(7, "base", 0);
        h.acquire("base", 0).unwrap();
        h.window_stops(7, "base", 0, "stopped");
        assert!(h.leases.is_empty(), "the core ends the lease");
        let kinds: Vec<String> = h.poll_messages().into_iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [DRIVE_STARTED, DRIVE_ENDED]);
        assert!(h.poll_messages().is_empty());
        h.send_message(None, "limits", Payload::Text("{}".into())).unwrap();
        assert_eq!(h.outbox, [(None, "limits".to_string(), Payload::Text("{}".into()))]);
    }

    #[test]
    fn commands_need_a_lease_on_a_role_the_app_may_drive() {
        let mut h = TestHost::with_roles(vec![base()]);
        let go = Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.2, 0.0, 0.0)));
        assert_eq!(h.send("base", 0, go), Err(DriveError::LeaseRequired));
        assert_eq!(h.acquire("lidar", 0), Err(DriveError::NoSuchRole));
        h.acquire("base", 0).unwrap();
        assert_eq!(h.send("base", 0, go), Ok(go));
        h.refuse = Some(DriveError::LeaseEnded { reason: "taken-over".into() });
        assert!(matches!(h.send("base", 0, go), Err(DriveError::LeaseEnded { .. })));
        assert_eq!(h.sent.len(), 1);
    }
}
