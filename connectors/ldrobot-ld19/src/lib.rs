//! Faber connector for the LDROBOT LD19 / STL-19P LiDAR.
//!
//! Port: serial at 230400 baud. Provides `laser-scan@1` as raw points, one reading per
//! revolution, in the part's own frame. Bytes in, readings out: no I/O here.

mod clock;
mod protocol;
mod revolution;

use faber_capabilities::{Capability, Info, Reading, Timestamp, laser_scan};
use faber_connector::{
    Connector, Context, Definition, Health, Manifest, PortRequirement, Problem, Provide, Settings,
    SettingsError, UsbHint,
};
use protocol::Parser;
use revolution::RevolutionAssembler;

pub const ID: &str = "ldrobot-ld19";
pub const BAUD: u32 = 230_400;

/// No bytes, or no valid packets, for this long means the part has failed.
const SILENCE_US: Timestamp = 1_000_000;
/// Corrupted packets are judged over this many packets.
const ERROR_WINDOW: u64 = 100;
/// More than this share of corrupted packets in a window means the part is degraded.
const MAX_ERROR_RATE: f64 = 0.10;

pub fn manifest() -> Manifest {
    Manifest {
        id: ID.into(),
        name: "LDROBOT LD19 / STL-19P LiDAR".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PortRequirement::Serial { baud: BAUD },
        // Range from the LD19 datasheet: 0.02–12 m (docs/simulation.md, section 7).
        provides: vec![Provide::with_info(Info::LaserScan(laser_scan::Info {
            range_min_m: 0.02,
            range_max_m: 12.0,
        }))],
        // The LD19 has nothing to set: it always spins at 10 Hz and sends 230400 baud.
        settings: Vec::new(),
        usb_hints: vec![UsbHint {
            vid: 0x10c4,
            pid: 0xea60,
            reason:
                "The LD19 kit comes with this CP2102 adapter. It is a common USB serial chip, so it could \
                     also be something else."
                    .into(),
        }],
    }
}

pub fn definition() -> Definition {
    Definition::new(manifest(), create)
}

fn create(settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    faber_connector::settings::Reader::new(settings, &manifest().settings, "the LD19 connector")?;
    Ok(Box::new(Ld19::default()))
}

#[derive(Default)]
struct Ld19 {
    parser: Parser,
    assembler: RevolutionAssembler,
    reported: Option<Health>,
    started_us: Option<Timestamp>,
    last_bytes_us: Option<Timestamp>,
    last_packet_us: Option<Timestamp>,
    revolutions: u64,
    /// Parser counters at the start of the current error window.
    window_start: (u64, u64),
    degraded_by_errors: Option<Problem>,
}

impl Ld19 {
    fn report(&mut self, ctx: &mut dyn Context, health: Health) {
        if self.reported.as_ref() != Some(&health) {
            ctx.set_health(health.clone());
            self.reported = Some(health);
        }
    }

    /// Re-judges the corrupted-packet rate once per window of packets.
    fn update_error_window(&mut self) {
        let stats = self.parser.stats;
        let good = stats.packets - self.window_start.0;
        let bad = stats.crc_errors + stats.rejected - self.window_start.1;
        if good + bad < ERROR_WINDOW {
            return;
        }
        let rate = bad as f64 / (good + bad) as f64;
        self.degraded_by_errors = (rate > MAX_ERROR_RATE).then(|| {
            Problem::new(
                format!("{:.0} % of the LiDAR's packets arrive corrupted.", rate * 100.0),
                "Check the cable and the USB adapter. Long, loose or unshielded wires often cause this.",
            )
        });
        self.window_start = (stats.packets, stats.crc_errors + stats.rejected);
    }

    fn healthy_state(&self) -> Health {
        match (&self.degraded_by_errors, self.revolutions) {
            (Some(p), _) => Health::Degraded(p.clone()),
            (None, 0) => Health::Starting,
            (None, _) => Health::Running,
        }
    }
}

impl Connector for Ld19 {
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]) {
        self.started_us.get_or_insert(t_us);
        self.last_bytes_us = Some(t_us);
        let mut packets = Vec::new();
        self.parser.push(bytes, t_us, &mut |p| packets.push(p));
        for p in &packets {
            self.last_packet_us = Some(t_us);
            if let Some(scan) = self.assembler.push(p) {
                self.revolutions += 1;
                ctx.publish(Capability::LaserScan.name(), Reading::LaserScan(scan));
            }
        }
        self.update_error_window();
        let health = self.healthy_state();
        self.report(ctx, health);
    }

    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        let started = *self.started_us.get_or_insert(now_us);
        let since = |t: Option<Timestamp>| now_us.saturating_sub(t.unwrap_or(started));
        let health = if since(self.last_bytes_us) >= SILENCE_US {
            Health::Failed(Problem::new(
                "No data from the LiDAR for over a second.",
                "Check that the LiDAR has 5 V power, that its cable is plugged in, and that this is the right port.",
            ))
        } else if since(self.last_packet_us) >= SILENCE_US {
            Health::Failed(Problem::new(
                "Data arrives, but it isn't LD19 data.",
                "Check that this port is the LD19 and runs at 230400 baud. The serial monitor shows what arrives.",
            ))
        } else {
            self.healthy_state()
        };
        self.report(ctx, health);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_connector::testing::RecordingContext;
    use protocol::{POINTS_PER_PACKET, Packet, RawPoint};

    fn packet_bytes(deg: f32, sensor_ms: u16) -> [u8; protocol::PACKET_LEN] {
        let start = (deg * 100.0) as u32 % 36_000;
        Packet {
            speed_dps: 3600,
            start_cdeg: start as u16,
            end_cdeg: ((start + 880) % 36_000) as u16,
            timestamp_ms: sensor_ms,
            points: [RawPoint { dist_mm: 1000, intensity: 200 }; POINTS_PER_PACKET],
            host_us: 0,
        }
        .encode()
    }

    /// Streams `n` good packets, 2.5 ms apart, starting at host time `t0`.
    fn stream(c: &mut dyn Connector, ctx: &mut RecordingContext, t0: Timestamp, n: u64) {
        for k in 0..n {
            let t_ms = k * 25 / 10;
            let bytes = packet_bytes((k as f32 * 9.0) % 360.0, (t_ms % 30_000) as u16);
            c.on_bytes(ctx, t0 + t_ms * 1000, &bytes);
        }
    }

    #[test]
    fn manifest_is_valid() {
        assert_eq!(manifest().validate(), Ok(()));
        assert_eq!(manifest().port, PortRequirement::Serial { baud: 230_400 });
    }

    #[test]
    fn rejects_unknown_settings_with_a_hint() {
        let mut s = Settings::new();
        s.insert("speed".into(), 10.into());
        let err = create(&s).err().expect("an error");
        assert_eq!(err.key, "speed");
        assert!(err.hint.contains("Remove"));
    }

    #[test]
    fn starts_then_runs_and_publishes_revolutions() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        stream(c.as_mut(), &mut ctx, 1_000_000, 130); // > 3 revolutions
        assert_eq!(ctx.health.first(), Some(&Health::Starting));
        assert_eq!(ctx.last_health(), Some(&Health::Running));
        assert!(ctx.published.len() >= 2);
        for (name, reading) in &ctx.published {
            assert_eq!(name, "laser-scan");
            let Reading::LaserScan(r) = reading else {
                panic!("an LD19 publishes only laser scans: {reading:?}")
            };
            assert!(r.validate().is_ok());
        }
    }

    #[test]
    fn health_is_only_reported_when_it_changes() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        stream(c.as_mut(), &mut ctx, 0, 200);
        assert_eq!(ctx.health, vec![Health::Starting, Health::Running]);
    }

    #[test]
    fn fails_when_the_port_goes_silent() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        stream(c.as_mut(), &mut ctx, 0, 130);
        c.on_tick(&mut ctx, 400_000);
        assert_eq!(ctx.last_health(), Some(&Health::Running));
        c.on_tick(&mut ctx, 1_400_000);
        let Some(Health::Failed(p)) = ctx.last_health() else { panic!("{:?}", ctx.health) };
        assert!(p.message.contains("No data"));
        assert!(p.hint.contains("5 V"));
    }

    #[test]
    fn fails_when_nothing_arrives_at_all() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        c.on_tick(&mut ctx, 0);
        c.on_tick(&mut ctx, 1_000_000);
        assert!(matches!(ctx.last_health(), Some(Health::Failed(_))));
    }

    #[test]
    fn fails_on_bytes_that_are_not_ld19_data() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        for k in 0..20u64 {
            c.on_bytes(&mut ctx, k * 100_000, b"$GPGGA,123519,4807.038,N*47\r\n");
        }
        c.on_tick(&mut ctx, 2_000_000);
        let Some(Health::Failed(p)) = ctx.last_health() else { panic!("{:?}", ctx.health) };
        assert!(p.message.contains("isn't LD19 data"));
    }

    #[test]
    fn degrades_on_many_corrupted_packets_and_recovers() {
        let mut c = create(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        stream(c.as_mut(), &mut ctx, 0, 130);
        for k in 0..100u64 {
            let mut b = packet_bytes((k as f32 * 9.0) % 360.0, (k % 30_000) as u16);
            if k % 3 == 0 {
                b[20] ^= 0xFF; // corrupt a third of them
            }
            c.on_bytes(&mut ctx, 400_000 + k * 2500, &b);
        }
        let Some(Health::Degraded(p)) = ctx.last_health() else { panic!("{:?}", ctx.health) };
        assert!(p.message.contains("corrupted"));
        stream(c.as_mut(), &mut ctx, 1_000_000, 250);
        assert_eq!(ctx.last_health(), Some(&Health::Running));
    }
}
