//! Assembles LD19 packets into whole revolutions as `laser-scan@1` raw-point readings.
//!
//! The split happens per point, at the exact 360° → 0° crossing (like LDROBOT's own SDK), so
//! consecutive revolutions never overlap. Every point gets its own measurement time from the
//! sensor clock. Output is in the part's own frame (x forward, y left, angles counter-clockwise);
//! the sensor's angles grow clockwise, so they are mirrored here.

use crate::clock::LidarClock;
use crate::protocol::{POINTS_PER_PACKET, Packet};
use faber_capabilities::laser_scan;
use std::f32::consts::TAU;

/// Safety valve: a stalled motor would otherwise grow one "revolution" forever.
const MAX_POINTS_PER_REVOLUTION: usize = 2_000;

#[derive(Default)]
struct Building {
    angles: Vec<f32>,
    ranges: Vec<f32>,
    intensities: Vec<f32>,
    t_start_s: f64,
    t_end_s: f64,
}

pub struct RevolutionAssembler {
    clock: LidarClock,
    cur: Building,
    last_angle_deg: f32,
    seen_wrap: bool,
    speed_sum: f64,
    speed_n: u32,
}

impl Default for RevolutionAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Sensor angle (degrees, clockwise) → part frame (radians, counter-clockwise, in [0, 2π)).
pub fn to_part_frame(sensor_deg: f32) -> f32 {
    let a = (-sensor_deg.to_radians()).rem_euclid(TAU);
    if a >= TAU { 0.0 } else { a }
}

impl RevolutionAssembler {
    pub fn new() -> Self {
        RevolutionAssembler {
            clock: LidarClock::new(),
            cur: Building::default(),
            last_angle_deg: -1.0,
            seen_wrap: false,
            speed_sum: 0.0,
            speed_n: 0,
        }
    }

    /// Feeds one packet. Returns a reading when this packet completed a revolution. The first,
    /// partial revolution (the stream started mid-turn) is dropped.
    pub fn push(&mut self, p: &Packet) -> Option<laser_scan::Reading> {
        let t_pkt = self.clock.host_time_of(p.timestamp_ms, p.host_us as f64 * 1e-6);
        let speed_dps = f64::from(p.speed_dps.max(1));
        let step_deg = f64::from(p.span_cdeg()) / 100.0 / (POINTS_PER_PACKET - 1) as f64;
        let mut done = None;
        for (i, rp) in p.points.iter().enumerate() {
            let angle_deg = p.angle_deg(i);
            if angle_deg < self.last_angle_deg - 180.0 || self.cur.ranges.len() >= MAX_POINTS_PER_REVOLUTION {
                done = self.finish();
            }
            self.last_angle_deg = angle_deg;
            let t = t_pkt + i as f64 * step_deg / speed_dps;
            if self.cur.ranges.is_empty() {
                self.cur.t_start_s = t;
            }
            self.cur.t_end_s = t;
            self.cur.angles.push(to_part_frame(angle_deg));
            self.cur.ranges.push(if rp.dist_mm == 0 { f32::INFINITY } else { f32::from(rp.dist_mm) * 1e-3 });
            self.cur.intensities.push(f32::from(rp.intensity) / 255.0);
        }
        self.speed_sum += f64::from(p.speed_dps);
        self.speed_n += 1;
        done
    }

    fn finish(&mut self) -> Option<laser_scan::Reading> {
        let b = std::mem::take(&mut self.cur);
        let mean_speed_dps = if self.speed_n > 0 { self.speed_sum / f64::from(self.speed_n) } else { 0.0 };
        self.speed_sum = 0.0;
        self.speed_n = 0;
        if !self.seen_wrap {
            self.seen_wrap = true;
            return None;
        }
        let n = b.ranges.len();
        let scan_time = if mean_speed_dps > 0.0 { (360.0 / mean_speed_dps) as f32 } else { 0.0 };
        let time_increment = (n > 1).then(|| ((b.t_end_s - b.t_start_s) / (n - 1) as f64) as f32);
        let mut r = laser_scan::Reading::raw_points(
            (b.t_start_s * 1e6).round() as u64,
            scan_time,
            b.angles,
            b.ranges,
        );
        r.time_increment = time_increment;
        r.intensities = Some(b.intensities);
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RawPoint;

    /// A synthetic packet whose 12 points start at `start_deg` and span 8.8°.
    fn packet(start_deg: f32, host_us: u64, sensor_ms: u16, dist_mm: u16) -> Packet {
        let start = (start_deg * 100.0) as u32 % 36_000;
        Packet {
            speed_dps: 3600, // 10 Hz
            start_cdeg: start as u16,
            end_cdeg: ((start + 880) % 36_000) as u16,
            timestamp_ms: sensor_ms,
            points: [RawPoint { dist_mm, intensity: 255 }; POINTS_PER_PACKET],
            host_us,
        }
    }

    /// Feeds `revs` full revolutions of packets starting at `start_deg`.
    fn spin(a: &mut RevolutionAssembler, start_deg: f32, revs: usize) -> Vec<laser_scan::Reading> {
        let mut out = Vec::new();
        let per_rev = 40; // 40 packets * 9° = 360°
        for k in 0..revs * per_rev {
            let deg = (start_deg + k as f32 * 9.0) % 360.0;
            let t_ms = k as u64 * 25 / 10; // 2.5 ms per packet = 10 Hz
            if let Some(r) = a.push(&packet(deg, 1_000_000 + t_ms * 1000, (t_ms % 30_000) as u16, 1500)) {
                out.push(r);
            }
        }
        out
    }

    #[test]
    fn mirrors_clockwise_sensor_angles() {
        assert_eq!(to_part_frame(0.0), 0.0);
        assert!((to_part_frame(90.0) - 3.0 * TAU / 4.0).abs() < 1e-5, "90° clockwise is 270° CCW");
        assert!((to_part_frame(270.0) - TAU / 4.0).abs() < 1e-5);
        assert!(to_part_frame(359.99) < 0.001);
    }

    #[test]
    fn drops_the_first_partial_revolution_then_emits_whole_ones() {
        let mut a = RevolutionAssembler::new();
        let revs = spin(&mut a, 100.0, 4);
        // Starting at 100° gives: partial (dropped), then 3 whole revolutions; the 4th is still open.
        assert_eq!(revs.len(), 3);
        for r in &revs {
            assert!(r.validate().is_ok());
            assert_eq!(r.len(), 40 * POINTS_PER_PACKET);
            assert!((r.scan_time - 0.1).abs() < 1e-4, "10 Hz");
            assert!(r.ranges.iter().all(|&x| (x - 1.5).abs() < 1e-6));
        }
    }

    #[test]
    fn revolutions_start_at_the_zero_crossing() {
        let mut a = RevolutionAssembler::new();
        let revs = spin(&mut a, 0.0, 3);
        // Sensor 0° maps to part angle 0, the first point of every whole revolution.
        assert!(revs[0].angle(0) < 1e-4, "{}", revs[0].angle(0));
    }

    #[test]
    fn zero_distance_is_no_return() {
        let mut a = RevolutionAssembler::new();
        let mut last = None;
        for k in 0..120 {
            let deg = (k as f32 * 9.0) % 360.0;
            let dist = if k % 2 == 0 { 0 } else { 800 };
            if let Some(r) = a.push(&packet(deg, k * 2500, (k * 25 / 10) as u16, dist)) {
                last = Some(r);
            }
        }
        let r = last.expect("a revolution");
        assert!(r.ranges.iter().any(|x| x.is_infinite()));
        assert!(r.ranges.iter().any(|&x| (x - 0.8).abs() < 1e-6));
    }

    #[test]
    fn timestamps_follow_the_host_clock() {
        let mut a = RevolutionAssembler::new();
        let revs = spin(&mut a, 0.0, 4);
        let dt = revs[1].t_us as i64 - revs[0].t_us as i64;
        assert!((dt - 100_000).abs() < 1_000, "revolutions 100 ms apart, got {dt} µs");
    }
}
