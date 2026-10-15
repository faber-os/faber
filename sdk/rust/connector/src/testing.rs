//! Helpers for testing connectors without hardware.

use crate::{Context, Health};
use faber_capabilities::Reading;

/// A [`Context`] that records everything a connector does, for assertions in tests.
#[derive(Debug, Default)]
pub struct RecordingContext {
    pub published: Vec<(String, Reading)>,
    pub health: Vec<Health>,
}

impl RecordingContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recent health the connector reported, if any.
    pub fn last_health(&self) -> Option<&Health> {
        self.health.last()
    }
}

impl Context for RecordingContext {
    fn publish(&mut self, name: &str, reading: Reading) {
        self.published.push((name.to_string(), reading));
    }

    fn set_health(&mut self, health: Health) {
        self.health.push(health);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::laser_scan;

    #[test]
    fn records_publishes_and_health() {
        let mut ctx = RecordingContext::new();
        assert_eq!(ctx.last_health(), None);
        ctx.set_health(Health::Running);
        ctx.publish(
            "laser-scan",
            Reading::LaserScan(laser_scan::Reading::even_bins(1, 0.0, 0.1, 0.1, vec![])),
        );
        assert_eq!(ctx.last_health(), Some(&Health::Running));
        assert_eq!(ctx.published.len(), 1);
        assert_eq!(ctx.published[0].0, "laser-scan");
    }
}
