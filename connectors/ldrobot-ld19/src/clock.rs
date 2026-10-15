//! Maps the LiDAR's millisecond counter (wraps at 30000) onto host monotonic time.
//!
//! The sensor stamps each packet when it is measured; the host only sees it after UART + USB
//! latency, which is always positive and varies. So `host - lidar` is smallest when transport
//! was fastest, and the running minimum of that offset is the best estimate of the true offset.
//! The minimum is allowed to creep upward slowly so crystal drift between the two clocks is tracked.

use crate::protocol::TIMESTAMP_WRAP_MS;

/// How fast the offset estimate may grow (s per s). 200 ppm covers cheap crystals.
const DRIFT_ALLOWANCE: f64 = 200e-6;
/// A jump bigger than this between consecutive packets means the sensor restarted or we lost sync.
const MAX_GAP_S: f64 = 1.0;

#[derive(Clone, Debug, Default)]
pub struct LidarClock {
    last_raw_ms: Option<u16>,
    unwrapped_ms: u64,
    offset_s: f64,
    last_host_s: f64,
    pub resets: u32,
}

impl LidarClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the host-time (seconds) at which the packet stamped `raw_ms` was measured.
    pub fn host_time_of(&mut self, raw_ms: u16, host_s: f64) -> f64 {
        match self.last_raw_ms {
            None => {
                self.unwrapped_ms = raw_ms as u64;
                self.offset_s = host_s - self.unwrapped_ms as f64 / 1000.0;
            }
            Some(last) => {
                let delta = (raw_ms as u32 + TIMESTAMP_WRAP_MS - last as u32) % TIMESTAMP_WRAP_MS;
                self.unwrapped_ms += delta as u64;
                let lidar_s = self.unwrapped_ms as f64 / 1000.0;
                let sample = host_s - lidar_s;
                let dt = (host_s - self.last_host_s).max(0.0);
                let predicted = self.offset_s + DRIFT_ALLOWANCE * dt;
                if (sample - predicted).abs() > MAX_GAP_S {
                    self.resets += 1;
                    self.offset_s = sample;
                } else {
                    self.offset_s = predicted.min(sample);
                }
            }
        }
        self.last_raw_ms = Some(raw_ms);
        self.last_host_s = host_s;
        self.unwrapped_ms as f64 / 1000.0 + self.offset_s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_and_rejects_transport_jitter() {
        let mut c = LidarClock::new();
        // True offset 100 s. Packets every 2.4 ms; transport delay 1..6 ms (pseudo random).
        let mut worst = 0.0f64;
        for i in 0..20_000u64 {
            let lidar_ms = (i * 24 / 10) as f64; // 2.4 ms steps
            let raw = (lidar_ms as u64 % 30_000) as u16;
            let delay = 0.001 + ((i * 7919) % 50) as f64 * 1e-4;
            let host = 100.0 + lidar_ms / 1000.0 + delay;
            let t = c.host_time_of(raw, host);
            if i > 200 {
                worst = worst.max((t - (100.0 + lidar_ms / 1000.0)).abs());
            }
        }
        assert!(worst < 0.0015, "worst error {worst}");
        assert_eq!(c.resets, 0);
    }
}
