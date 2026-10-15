//! Layer 1: the core's monotonic clock (µs). Every reading and log entry uses it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub trait Clock: Send + Sync {
    fn now_us(&self) -> u64;
}

pub type SharedClock = Arc<dyn Clock>;

/// Microseconds since the core started.
pub struct MonotonicClock {
    start: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        MonotonicClock { start: Instant::now() }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

/// A clock tests move by hand.
#[derive(Default)]
pub struct ManualClock {
    now: AtomicU64,
}

impl ManualClock {
    pub fn new(start_us: u64) -> Self {
        ManualClock { now: AtomicU64::new(start_us) }
    }

    pub fn set(&self, us: u64) {
        self.now.store(us, Ordering::SeqCst);
    }

    pub fn advance(&self, us: u64) {
        self.now.fetch_add(us, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_us(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_clock_never_goes_back() {
        let c = MonotonicClock::new();
        let a = c.now_us();
        let b = c.now_us();
        assert!(b >= a);
    }

    #[test]
    fn manual_clock_moves_only_when_told() {
        let c = ManualClock::new(10);
        assert_eq!(c.now_us(), 10);
        c.advance(5);
        assert_eq!(c.now_us(), 15);
        c.set(100);
        assert_eq!(c.now_us(), 100);
    }
}
