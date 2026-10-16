//! Layer 2: leases (docs/capabilities.md, "The lease"; docs/simulation.md, section 5).
//!
//! A lease is the exclusive, time-limited right of one holder to command one actuator part, and
//! the parts it is built from. This module is the lease table and nothing else: it is pure, the
//! caller passes the time (the core's monotonic clock), and it never sends anything. Every way a
//! lease ends comes back as an [`Ended`], so the caller can send the part its safe state.

use crate::names::PartName;
use std::collections::{BTreeMap, VecDeque};

/// How many ended leases are remembered, so a late command hears *why* its lease ended.
const HISTORY: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// The holder's proof. Unguessable enough that one program can't use another's by accident.
    pub id: String,
    pub part: PartName,
    /// Who holds it, in words people recognise ("Drive window", "drive-square.py").
    pub holder: String,
    pub duration_us: u64,
    pub deadline_us: u64,
    /// The parts `part` is built from: locked while the lease lasts.
    pub locks: Vec<PartName>,
}

impl Lease {
    /// Every part the lease covers: its own part, then the ones it locks.
    pub fn parts(&self) -> impl Iterator<Item = &PartName> {
        std::iter::once(&self.part).chain(&self.locks)
    }

    fn covers(&self, part: &PartName) -> bool {
        self.parts().any(|p| p == part)
    }
}

/// Why a lease ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The holder released it.
    Released,
    /// No command or renewal before its deadline.
    Expired,
    /// The e-stop was engaged.
    Estop,
    /// Someone took it over (an operator, or the shell's "take over").
    TakenOver,
    PartRemoved,
    /// The part was restarted (its config changed).
    PartRestarted,
    /// The part stopped working.
    PartFailed,
    /// The part was paused (`paused = true`), e.g. to free its port for another program.
    PartPaused,
    /// The core is stopping.
    Shutdown,
}

impl EndReason {
    pub fn id(self) -> &'static str {
        match self {
            EndReason::Released => "released",
            EndReason::Expired => "expired",
            EndReason::Estop => "estop",
            EndReason::TakenOver => "taken-over",
            EndReason::PartRemoved => "part-removed",
            EndReason::PartRestarted => "part-restarted",
            EndReason::PartFailed => "part-failed",
            EndReason::PartPaused => "part-paused",
            EndReason::Shutdown => "shutdown",
        }
    }
}

/// A lease that ended. Its parts must now get their safe state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub lease: Lease,
    pub reason: EndReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaseError {
    /// Someone else holds `part` (the requested part or one it is built from). `through` is set
    /// when the lease in the way is on another part that locks `part`, e.g. the base a motor
    /// belongs to.
    Held { part: PartName, holder: String, through: Option<PartName> },
    /// No lease with this id is held on this part.
    Required,
    /// This lease existed but has ended.
    Ended(EndReason),
}

/// A request for a lease.
#[derive(Clone, Debug)]
pub struct Request {
    pub part: PartName,
    pub holder: String,
    pub duration_us: u64,
    /// The parts `part` is built from.
    pub locks: Vec<PartName>,
    /// End whatever lease is in the way (an operator taking over).
    pub take_over: bool,
}

pub struct Leases {
    active: BTreeMap<PartName, Lease>,
    /// (id, part, reason) of recently ended leases, newest last.
    ended: VecDeque<(String, PartName, EndReason)>,
    seed: u64,
    counter: u64,
}

impl Leases {
    /// `seed` makes lease ids differ between runs of the core (pass something like the time).
    pub fn new(seed: u64) -> Leases {
        Leases { active: BTreeMap::new(), ended: VecDeque::new(), seed, counter: 0 }
    }

    /// Grants a lease, or says who is in the way. Leases in the way that have already expired
    /// are ended (and returned); with `take_over`, every lease in the way is.
    pub fn request(&mut self, req: Request, now_us: u64) -> Result<(Lease, Vec<Ended>), LeaseError> {
        let wanted: Vec<&PartName> = std::iter::once(&req.part).chain(&req.locks).collect();
        let in_way: Vec<PartName> = self
            .active
            .values()
            .filter(|l| wanted.iter().any(|p| l.covers(p)))
            .map(|l| l.part.clone())
            .collect();
        let mut ended = Vec::new();
        for part in in_way {
            let lease = &self.active[&part];
            let reason = if lease.deadline_us <= now_us {
                EndReason::Expired
            } else if req.take_over {
                EndReason::TakenOver
            } else {
                let blocked = wanted.iter().copied().find(|p| lease.covers(p)).expect("in the way").clone();
                let through = (lease.part != blocked).then(|| lease.part.clone());
                return Err(LeaseError::Held { part: blocked, holder: lease.holder.clone(), through });
            };
            ended.push(self.end(&part, reason));
        }
        self.counter += 1;
        let lease = Lease {
            id: lease_id(self.seed, self.counter),
            part: req.part.clone(),
            holder: req.holder,
            duration_us: req.duration_us,
            deadline_us: now_us + req.duration_us,
            locks: req.locks,
        };
        self.active.insert(req.part, lease.clone());
        Ok((lease, ended))
    }

    /// Checks that `id` is the live lease on `part`. Doesn't renew it.
    pub fn check(&self, part: &PartName, id: &str, now_us: u64) -> Result<&Lease, LeaseError> {
        match self.active.get(part) {
            Some(l) if l.id == id && l.deadline_us > now_us => Ok(l),
            // Past its deadline: the sweeper is about to end it.
            Some(l) if l.id == id => Err(LeaseError::Ended(EndReason::Expired)),
            _ => Err(self.why_not(part, id)),
        }
    }

    /// Pushes the deadline out by the lease's duration.
    pub fn renew(&mut self, part: &PartName, id: &str, now_us: u64) -> Result<Lease, LeaseError> {
        self.check(part, id, now_us)?;
        let lease = self.active.get_mut(part).expect("checked");
        lease.deadline_us = now_us + lease.duration_us;
        Ok(lease.clone())
    }

    /// The holder is done.
    pub fn release(&mut self, part: &PartName, id: &str) -> Result<Ended, LeaseError> {
        match self.active.get(part) {
            Some(l) if l.id == id => Ok(self.end(part, EndReason::Released)),
            _ => Err(self.why_not(part, id)),
        }
    }

    /// Ends every lease whose deadline has passed.
    pub fn sweep(&mut self, now_us: u64) -> Vec<Ended> {
        let expired: Vec<PartName> =
            self.active.values().filter(|l| l.deadline_us <= now_us).map(|l| l.part.clone()).collect();
        expired.iter().map(|p| self.end(p, EndReason::Expired)).collect()
    }

    /// Ends every lease (the e-stop, or the core stopping).
    pub fn revoke_all(&mut self, reason: EndReason) -> Vec<Ended> {
        let parts: Vec<PartName> = self.active.keys().cloned().collect();
        parts.iter().map(|p| self.end(p, reason)).collect()
    }

    /// Ends the leases that cover `part`: its own, and one on a part built from it.
    pub fn revoke_part(&mut self, part: &PartName, reason: EndReason) -> Vec<Ended> {
        let parts: Vec<PartName> =
            self.active.values().filter(|l| l.covers(part)).map(|l| l.part.clone()).collect();
        parts.iter().map(|p| self.end(p, reason)).collect()
    }

    /// Every live lease, by part.
    pub fn active(&self) -> Vec<Lease> {
        self.active.values().cloned().collect()
    }

    /// The live lease that covers `part`, if any.
    pub fn covering(&self, part: &PartName) -> Option<&Lease> {
        self.active.values().find(|l| l.covers(part))
    }

    fn end(&mut self, part: &PartName, reason: EndReason) -> Ended {
        let lease = self.active.remove(part).expect("ending a live lease");
        if self.ended.len() == HISTORY {
            self.ended.pop_front();
        }
        self.ended.push_back((lease.id.clone(), lease.part.clone(), reason));
        Ended { lease, reason }
    }

    fn why_not(&self, part: &PartName, id: &str) -> LeaseError {
        self.ended
            .iter()
            .rev()
            .find(|(i, p, _)| i == id && p == part)
            .map_or(LeaseError::Required, |(_, _, r)| LeaseError::Ended(*r))
    }
}

/// 16 hex digits that don't reveal the order leases were granted in (splitmix64).
fn lease_id(seed: u64, n: u64) -> String {
    let mut z = seed.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!("{:016x}", z ^ (z >> 31))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000;

    fn p(s: &str) -> PartName {
        PartName::parse(s).unwrap()
    }

    fn req(part: &str, holder: &str) -> Request {
        Request {
            part: p(part),
            holder: holder.into(),
            duration_us: 250 * MS,
            locks: vec![],
            take_over: false,
        }
    }

    fn base_req(holder: &str) -> Request {
        Request { locks: vec![p("left"), p("right")], ..req("base", holder) }
    }

    #[test]
    fn a_granted_lease_has_a_deadline_and_blocks_others() {
        let mut l = Leases::new(1);
        let (lease, ended) = l.request(req("base", "Drive window"), 1_000).unwrap();
        assert!(ended.is_empty());
        assert_eq!(lease.deadline_us, 1_000 + 250 * MS);
        assert_eq!(lease.id.len(), 16);
        let e = l.request(req("base", "drive-square.py"), 2_000).unwrap_err();
        assert_eq!(e, LeaseError::Held { part: p("base"), holder: "Drive window".into(), through: None });
        assert_eq!(l.check(&p("base"), &lease.id, 2_000).unwrap().holder, "Drive window");
    }

    #[test]
    fn ids_are_unique_and_differ_between_runs() {
        let mut a = Leases::new(1);
        let (x, _) = a.request(req("a", "h"), 0).unwrap();
        let (y, _) = a.request(req("b", "h"), 0).unwrap();
        assert_ne!(x.id, y.id);
        let (z, _) = Leases::new(2).request(req("a", "h"), 0).unwrap();
        assert_ne!(x.id, z.id);
    }

    #[test]
    fn renewing_pushes_the_deadline_and_expiry_ends_it() {
        let mut l = Leases::new(1);
        let (lease, _) = l.request(req("base", "h"), 0).unwrap();
        let renewed = l.renew(&p("base"), &lease.id, 200 * MS).unwrap();
        assert_eq!(renewed.deadline_us, 450 * MS);
        assert!(l.sweep(449 * MS).is_empty());
        let ended = l.sweep(450 * MS);
        assert_eq!(ended.len(), 1);
        assert_eq!((ended[0].reason, &ended[0].lease.part), (EndReason::Expired, &p("base")));
        assert_eq!(l.renew(&p("base"), &lease.id, 460 * MS), Err(LeaseError::Ended(EndReason::Expired)));
        assert!(l.active().is_empty());
    }

    #[test]
    fn a_lease_past_its_deadline_is_refused_even_before_the_sweep() {
        let mut l = Leases::new(1);
        let (lease, _) = l.request(req("base", "h"), 0).unwrap();
        assert_eq!(
            l.check(&p("base"), &lease.id, 250 * MS).unwrap_err(),
            LeaseError::Ended(EndReason::Expired)
        );
        assert_eq!(
            l.renew(&p("base"), &lease.id, 300 * MS).unwrap_err(),
            LeaseError::Ended(EndReason::Expired)
        );
    }

    #[test]
    fn an_expired_lease_is_not_in_the_way() {
        let mut l = Leases::new(1);
        l.request(req("base", "old"), 0).unwrap();
        let (lease, ended) = l.request(req("base", "new"), 300 * MS).unwrap();
        assert_eq!(lease.holder, "new");
        assert_eq!(ended.len(), 1);
        assert_eq!((ended[0].reason, ended[0].lease.holder.as_str()), (EndReason::Expired, "old"));
    }

    #[test]
    fn release_ends_it_and_a_late_command_hears_why() {
        let mut l = Leases::new(1);
        let (lease, _) = l.request(req("base", "h"), 0).unwrap();
        assert_eq!(l.release(&p("base"), "wrong").unwrap_err(), LeaseError::Required);
        let ended = l.release(&p("base"), &lease.id).unwrap();
        assert_eq!(ended.reason, EndReason::Released);
        assert_eq!(l.check(&p("base"), &lease.id, 1).unwrap_err(), LeaseError::Ended(EndReason::Released));
        assert_eq!(l.release(&p("base"), &lease.id).unwrap_err(), LeaseError::Ended(EndReason::Released));
        assert_eq!(l.check(&p("base"), "never-existed", 1).unwrap_err(), LeaseError::Required);
        assert_eq!(l.check(&p("other"), &lease.id, 1).unwrap_err(), LeaseError::Required, "ids are per part");
    }

    #[test]
    fn an_operator_can_take_over() {
        let mut l = Leases::new(1);
        let (app, _) = l.request(req("base", "drive-square.py"), 0).unwrap();
        let (op, ended) = l.request(Request { take_over: true, ..req("base", "Drive window") }, 10).unwrap();
        assert_eq!(op.holder, "Drive window");
        assert_eq!(ended.len(), 1);
        assert_eq!((ended[0].reason, &ended[0].lease.id), (EndReason::TakenOver, &app.id));
        assert_eq!(l.check(&p("base"), &app.id, 20).unwrap_err(), LeaseError::Ended(EndReason::TakenOver));
    }

    #[test]
    fn a_lease_on_a_base_locks_the_motors_it_is_built_from() {
        let mut l = Leases::new(1);
        let (base, _) = l.request(base_req("Drive window"), 0).unwrap();
        let e = l.request(req("left", "motor test"), 1).unwrap_err();
        assert_eq!(
            e,
            LeaseError::Held { part: p("left"), holder: "Drive window".into(), through: Some(p("base")) }
        );
        assert_eq!(l.covering(&p("right")).unwrap().id, base.id);
        let parts: Vec<_> = base.parts().map(PartName::as_str).collect();
        assert_eq!(parts, ["base", "left", "right"]);
    }

    #[test]
    fn a_base_cannot_be_leased_while_one_of_its_motors_is() {
        let mut l = Leases::new(1);
        l.request(req("right", "motor test"), 0).unwrap();
        let e = l.request(base_req("Drive window"), 1).unwrap_err();
        assert_eq!(e, LeaseError::Held { part: p("right"), holder: "motor test".into(), through: None });
        let (_, ended) = l.request(Request { take_over: true, ..base_req("Drive window") }, 2).unwrap();
        assert_eq!(ended[0].lease.part, p("right"));
    }

    #[test]
    fn revoking_ends_the_right_leases() {
        let mut l = Leases::new(1);
        l.request(base_req("a"), 0).unwrap();
        l.request(req("arm", "b"), 0).unwrap();
        let ended = l.revoke_part(&p("left"), EndReason::PartRemoved);
        assert_eq!(ended.len(), 1, "removing a motor ends the lease on its base");
        assert_eq!((ended[0].reason, &ended[0].lease.part), (EndReason::PartRemoved, &p("base")));
        assert!(l.revoke_part(&p("nothing"), EndReason::PartRemoved).is_empty());
        let all = l.revoke_all(EndReason::Estop);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].reason, EndReason::Estop);
        assert!(l.active().is_empty());
    }

    #[test]
    fn the_history_is_bounded() {
        let mut l = Leases::new(1);
        let (first, _) = l.request(req("a", "h"), 0).unwrap();
        l.release(&p("a"), &first.id).unwrap();
        for _ in 0..HISTORY {
            let (x, _) = l.request(req("a", "h"), 0).unwrap();
            l.release(&p("a"), &x.id).unwrap();
        }
        assert_eq!(
            l.check(&p("a"), &first.id, 0).unwrap_err(),
            LeaseError::Required,
            "forgotten after a while"
        );
    }

    #[test]
    fn reasons_have_kebab_ids() {
        let all = [
            EndReason::Released,
            EndReason::Expired,
            EndReason::Estop,
            EndReason::TakenOver,
            EndReason::PartRemoved,
            EndReason::PartRestarted,
            EndReason::PartFailed,
            EndReason::Shutdown,
        ];
        for r in all {
            assert!(r.id().chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{}", r.id());
        }
    }
}
