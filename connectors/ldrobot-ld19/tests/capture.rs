//! Runs a real LD19 capture (Mikkel's living room, 2026-10-12) through the connector.

use faber_capabilities::Reading;
use faber_connector::capture;
use faber_connector::testing::RecordingContext;
use faber_connector::{Health, Settings};

fn load() -> Vec<capture::Chunk> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/captures/ld19-room-walkthrough.fbcap");
    capture::decode(&std::fs::read(path).expect("fixture exists")).expect("valid capture")
}

#[test]
fn real_capture_gives_whole_revolutions_at_about_10_hz() {
    let chunks = load();
    let duration_s = (chunks.last().unwrap().t_us - chunks[0].t_us) as f64 * 1e-6;
    let def = ldrobot_ld19::definition();
    let mut c = (def.create)(&Settings::new()).unwrap();
    let mut ctx = RecordingContext::new();
    let mut next_tick = chunks[0].t_us;
    for chunk in &chunks {
        while next_tick <= chunk.t_us {
            c.on_tick(&mut ctx, next_tick);
            next_tick += 100_000;
        }
        c.on_bytes(&mut ctx, chunk.t_us, &chunk.bytes);
    }

    assert!(ctx.health.iter().all(|h| !matches!(h, Health::Failed(_))), "{:?}", ctx.health);
    assert_eq!(ctx.last_health(), Some(&Health::Running));

    let scans: Vec<_> = ctx
        .published
        .iter()
        .map(|(_, reading)| match reading {
            Reading::LaserScan(r) => r,
            other => panic!("an LD19 publishes only laser scans: {other:?}"),
        })
        .collect();
    let expected = duration_s * 10.0;
    assert!(
        (scans.len() as f64 - expected).abs() < expected * 0.1,
        "{} revolutions in {duration_s:.1} s",
        scans.len()
    );
    for s in &scans {
        s.validate().expect("valid reading");
        assert!((400..=520).contains(&s.len()), "points per revolution: {}", s.len());
        assert!((0.09..0.11).contains(&s.scan_time), "scan time {}", s.scan_time);
    }
    for pair in scans.windows(2) {
        assert!(pair[1].t_us > pair[0].t_us, "timestamps increase");
    }
}
