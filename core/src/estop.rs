//! Layer 2: the e-stop's state and its history (docs/simulation.md, section 6).
//!
//! Pure bookkeeping; the caller passes the time. What engaging *does* (revoking every lease,
//! sending every actuator its safe state, refusing new leases) is wired in `machine`.

use faber_capabilities::estop::{Reading, Source};
use std::collections::VecDeque;

/// How many changes are kept for `GET /api/v0/estop`.
const HISTORY: usize = 50;

#[derive(Default)]
pub struct Estop {
    /// The change that engaged it, while it is engaged.
    engaged: Option<Reading>,
    /// Every engage and release, newest last.
    history: VecDeque<Reading>,
}

impl Estop {
    pub fn is_engaged(&self) -> bool {
        self.engaged.is_some()
    }

    /// The change that engaged it, while it is engaged.
    pub fn engaged(&self) -> Option<&Reading> {
        self.engaged.as_ref()
    }

    pub fn history(&self) -> impl Iterator<Item = &Reading> {
        self.history.iter()
    }

    /// Engages it. Always allowed, for anyone. Returns the change, or `None` if it was already
    /// engaged (the first reason stays).
    pub fn engage(&mut self, source: Source, reason: &str, now_us: u64) -> Option<Reading> {
        if self.engaged.is_some() {
            return None;
        }
        let r = Reading { t_us: now_us, engaged: true, source, reason: reason.into() };
        self.engaged = Some(r.clone());
        self.record(r.clone());
        Some(r)
    }

    /// Releases it. Returns the change, or `None` if it wasn't engaged.
    pub fn release(&mut self, source: Source, reason: &str, now_us: u64) -> Option<Reading> {
        self.engaged.take()?;
        let r = Reading { t_us: now_us, engaged: false, source, reason: reason.into() };
        self.record(r.clone());
        Some(r)
    }

    fn record(&mut self, r: Reading) {
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_released() {
        let e = Estop::default();
        assert!(!e.is_engaged());
        assert_eq!(e.history().count(), 0);
    }

    #[test]
    fn engage_and_release_are_recorded_once_each() {
        let mut e = Estop::default();
        let on = e.engage(Source::Shell, "Stop button", 10).unwrap();
        assert!(on.engaged && e.is_engaged());
        assert_eq!(e.engage(Source::App, "again", 20), None, "already engaged");
        assert_eq!(e.engaged().unwrap().reason, "Stop button", "the first reason stays");
        let off = e.release(Source::Shell, "Released", 30).unwrap();
        assert!(!off.engaged && !e.is_engaged());
        assert_eq!(e.release(Source::Shell, "again", 40), None);
        let times: Vec<_> = e.history().map(|r| r.t_us).collect();
        assert_eq!(times, [10, 30]);
    }

    #[test]
    fn the_history_is_bounded() {
        let mut e = Estop::default();
        for t in 0..(HISTORY as u64) {
            e.engage(Source::App, "x", 2 * t);
            e.release(Source::App, "y", 2 * t + 1);
        }
        assert_eq!(e.history().count(), HISTORY);
        assert_eq!(e.history().last().unwrap().t_us, 2 * HISTORY as u64 - 1);
    }
}
