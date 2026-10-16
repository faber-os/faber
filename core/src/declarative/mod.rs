//! Layer 3: the declarative connector engine (docs/packages.md §6,
//! docs/connectors-declarative.md). A declarative connector is only a `faber.toml`; this engine
//! runs it as an ordinary [`Connector`](faber_connector::Connector), so the supervisor treats it
//! exactly like a built-in one.
//!
//! Serial devices that send text lines, read with typed patterns or the built-in NMEA reader, and
//! I²C sensors whose registers it reads on a schedule (Phase 6). SPI is not declarative yet.

pub mod engine;
pub mod installed;
pub mod lines;
pub mod manifest;
pub mod nmea;
pub mod pattern;
pub mod registers;
pub mod values;

pub use manifest::{Declarative, FILE_NAME, FORMAT, ManifestError};

use faber_connector::{ConnectorKind, Definition};
use std::sync::Arc;

/// A connector definition from the text of a declarative `faber.toml`.
pub fn definition(text: &str) -> Result<Definition, ManifestError> {
    Ok(from_package(manifest::parse(text)?))
}

pub fn from_package(package: Declarative) -> Definition {
    let package = Arc::new(package);
    let (for_create, for_provides) = (Arc::clone(&package), Arc::clone(&package));
    Definition::new(package.manifest.clone(), move |settings| for_create.create(settings))
        .with_provides(move |_, settings| for_provides.provides(settings))
        .with_kind(ConnectorKind::Declarative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{Reading, battery, distance, position};
    use faber_connector::testing::RecordingContext;
    use faber_connector::{Connector, Health, Settings};

    fn manifest(provides: &str, extra: &str) -> String {
        format!(
            "faber-manifest = 1\n[package]\nid = \"t\"\ntype = \"connector\"\nkind = \"declarative\"\n\
             version = \"0.1.0\"\nname = \"Test sensor\"\n[ports]\nserial = {{ baud = 9600 }}\n{extra}\n{provides}\n"
        )
    }

    const DISTANCE: &str = "[[provides]]\ncapability = \"distance@1\"\n\
        info = { min-m = 0.02, max-m = 4.0, field-of-view-rad = 0.26, kind = \"ultrasonic\" }\n\
        line = { pattern = \"D:{meters:f32}\", scale = 0.01 }";

    fn connector(text: &str) -> Box<dyn Connector> {
        (definition(text).unwrap().create)(&Settings::new()).unwrap()
    }

    fn feed(c: &mut dyn Connector, ctx: &mut RecordingContext, chunks: &[(u64, &str)]) {
        for (t, s) in chunks {
            c.on_bytes(ctx, *t, s.as_bytes());
        }
    }

    fn distances(ctx: &RecordingContext) -> Vec<(u64, f32, distance::Status)> {
        ctx.published
            .iter()
            .map(|(name, r)| {
                assert_eq!(name, "distance");
                let Reading::Distance(d) = r else { panic!("{r:?}") };
                (d.t_us, d.meters, d.status)
            })
            .collect()
    }

    #[test]
    fn lines_become_readings_stamped_when_their_last_byte_arrived() {
        let mut c = connector(&manifest(DISTANCE, ""));
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(10, "D:12"), (20, "3\nD:5"), (30, "00\r\nD:1\n")]);
        use distance::Status::*;
        assert_eq!(
            distances(&ctx),
            [(20, 1.23, Ok), (30, 4.0, TooFar), (30, 0.02, TooClose)],
            "scaled, and judged against the info's range"
        );
        assert_eq!(ctx.health, [Health::Starting, Health::Running]);
        let d = definition(&manifest(DISTANCE, "")).unwrap();
        assert_eq!(d.kind, faber_connector::ConnectorKind::Declarative);
    }

    #[test]
    fn garbage_is_counted_shown_after_a_run_and_never_fatal() {
        let mut c = connector(&manifest(DISTANCE, "[lines]\nwarn-after-bad-lines = 3"));
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(1, "hello\nD:x\n")]);
        assert_eq!(ctx.health, [Health::Starting], "two bad lines are not yet a problem");
        c.on_bytes(&mut ctx, 2, &[0xff, 0xfe, b'\n']);
        let Some(Health::Degraded(p)) = ctx.last_health() else { panic!("{:?}", ctx.health) };
        assert!(p.message.contains("The last 3 lines") && p.message.contains("3 bad lines so far"), "{p:?}");
        assert!(p.message.contains("isn't text"), "{p:?}");
        assert!(p.hint.contains("9600 baud") && p.hint.contains("serial monitor"), "{p:?}");
        feed(c.as_mut(), &mut ctx, &[(3, "D:100\n")]);
        assert_eq!(ctx.last_health(), Some(&Health::Running), "one good line ends the run");
        assert_eq!(ctx.published.len(), 1);
    }

    #[test]
    fn a_bad_value_says_which_field() {
        let mut c = connector(&manifest(DISTANCE, "[lines]\nwarn-after-bad-lines = 1"));
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(1, "D:abc\n")]);
        let Some(Health::Degraded(p)) = ctx.last_health() else { panic!() };
        assert!(p.message.contains("meters: \"abc\" is not a number"), "{p:?}");
    }

    #[test]
    fn lines_over_the_limit_are_bad_lines() {
        let mut c = connector(&manifest(DISTANCE, "[lines]\nmax-length = 8\nwarn-after-bad-lines = 1"));
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(1, "D:1234567890\n")]);
        let Some(Health::Degraded(p)) = ctx.last_health() else { panic!() };
        assert!(p.message.contains("longer than 8 bytes"), "{p:?}");
    }

    #[test]
    fn silence_and_unreadable_data_fail_the_part_after_the_timeout() {
        let mut c = connector(&manifest(DISTANCE, "[lines]\ntimeout-s = 1"));
        let mut ctx = RecordingContext::new();
        c.on_tick(&mut ctx, 0);
        c.on_tick(&mut ctx, 1_000_000);
        let Some(Health::Failed(p)) = ctx.last_health() else { panic!() };
        assert!(p.message.contains("Nothing has arrived") && p.message.contains("1 s"), "{p:?}");
        feed(c.as_mut(), &mut ctx, &[(1_100_000, "junk\n")]);
        c.on_tick(&mut ctx, 1_200_000);
        let Some(Health::Failed(p)) = ctx.last_health() else { panic!() };
        assert!(p.message.contains("no line could be read"), "{p:?}");
        feed(c.as_mut(), &mut ctx, &[(1_300_000, "D:50\n")]);
        c.on_tick(&mut ctx, 1_400_000);
        assert_eq!(ctx.last_health(), Some(&Health::Running));
    }

    #[test]
    fn settings_reach_scale_and_info() {
        let text = manifest(
            "[[provides]]\ncapability = \"distance@1\"\n\
             info = { min-m = 0.0, max-m = { setting = \"max-m\" }, field-of-view-rad = 0.1, kind = \"infrared\" }\n\
             line = { pattern = \"{meters:i32}mm\", scale = { setting = \"scale\" } }",
            "[[settings]]\nkey = \"max-m\"\nlabel = \"Max\"\ndescription = \"Farthest.\"\nkind = \"number\"\nunit = \"m\"\ndefault = 2.0\n\
             [[settings]]\nkey = \"scale\"\nlabel = \"Scale\"\ndescription = \"Units to metres.\"\nkind = \"number\"\ndefault = 0.001\n",
        );
        let d = definition(&text).unwrap();
        let mut s = Settings::new();
        s.insert("max-m".into(), 5.into());
        let provided = (d.provides)(&d.manifest, &s).unwrap();
        assert!(matches!(&provided[0].info, Some(faber_capabilities::Info::Distance(i)) if i.max_m == 5.0));
        let mut c = (d.create)(&s).unwrap();
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(1, "4500mm\n")]);
        assert_eq!(distances(&ctx), [(1, 4.5, distance::Status::Ok)]);
        let mut bad = Settings::new();
        bad.insert("gain".into(), 1.into());
        let e = (d.create)(&bad).err().unwrap();
        assert_eq!(e.message, "The Test sensor connector has no setting called \"gain\".");
    }

    #[test]
    fn several_provides_share_one_stream() {
        let text = manifest(
            "[[provides]]\ncapability = \"distance@1\"\nname = \"front\"\nline = { pattern = \"F={meters:f32}\" }\n\
             [[provides]]\ncapability = \"distance@1\"\nname = \"rear\"\nline = { pattern = \"R={meters:f32}\", set = { status = \"ok\" } }",
            "",
        );
        let mut c = connector(&text);
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(1, "F=1\nR=2\n")]);
        let names: Vec<&str> = ctx.published.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["front", "rear"]);
    }

    #[test]
    fn nested_fields_and_fixed_values_make_other_capabilities() {
        let text = manifest(
            "[[provides]]\ncapability = \"pose@1\"\n\
             line = { pattern = \"P {position.x:f32} {position.y:f32}\", set = { frame = \"odom\", position.z = 0, orientation = { x = 0, y = 0, z = 0, w = 1 } } }",
            "",
        );
        let mut c = connector(&text);
        let mut ctx = RecordingContext::new();
        feed(c.as_mut(), &mut ctx, &[(4, "P 1.5 -2\n")]);
        assert_eq!(
            ctx.published,
            [(
                "pose".to_string(),
                Reading::Pose(faber_capabilities::pose::Reading::planar(4, "odom", 1.5, -2.0, 0.0))
            )]
        );
    }

    /// Any capability's fields can come from a line. A battery's level, which no sensor sends,
    /// is worked out from its voltage and info, with the capability's hysteresis.
    #[test]
    fn a_battery_monitor_gives_volts_and_amps_and_faber_works_out_the_level() {
        let text = manifest(
            "[[provides]]
capability = \"battery@1\"
             info = { chemistry = \"li-ion\", cells-in-series = 3, capacity-ah = 15 }
             line = { pattern = \"V={volts:f32} I={amps:f32}\", fields.amps.scale = -0.001 }",
            "",
        );
        let mut c = connector(&text);
        let mut ctx = RecordingContext::new();
        let lines = "V=12.1 I=800
V=10.4 I=900
V=10.55 I=0
V=10.7 I=0
V=9.8 I=2000
";
        feed(c.as_mut(), &mut ctx, &[(1, lines)]);
        let seen: Vec<(f32, Option<f32>, battery::Level)> = ctx
            .published
            .iter()
            .map(|(_, r)| {
                let Reading::Battery(b) = r else { panic!("{r:?}") };
                (b.volts, b.amps, b.level)
            })
            .collect();
        use battery::Level::*;
        assert_eq!(
            seen,
            [
                (12.1, Some(-0.8), Ok),
                (10.4, Some(-0.9), Low),
                (10.55, Some(0.0), Low),
                (10.7, Some(0.0), Ok),
                (9.8, Some(-2.0), Critical)
            ]
        );
        let given = manifest(
            "[[provides]]
capability = \"battery@1\"
line = { pattern = \"V={volts:f32}\", set = { level = \"low\" } }",
            "",
        );
        let mut c = connector(&given);
        let mut ctx = RecordingContext::new();
        feed(
            c.as_mut(),
            &mut ctx,
            &[(
                1, "V=12
",
            )],
        );
        let Reading::Battery(b) = &ctx.published[0].1 else { panic!() };
        assert_eq!(b.level, Low, "a level the manifest gives is used as it is");
    }

    #[test]
    fn a_battery_without_a_level_needs_its_info() {
        let text = manifest(
            "[[provides]]
capability = \"battery@1\"
line = { pattern = \"V={volts:f32}\" }",
            "",
        );
        let e = definition(&text).unwrap_err();
        assert!(e.message.contains("needs its info"), "{e}");
        assert!(e.hint.contains("cells-in-series"), "{e:?}");
    }

    #[test]
    fn nmea_gives_positions() {
        let text = manifest("[[provides]]\ncapability = \"position@1\"\nline = { format = \"nmea\" }", "");
        let mut c = connector(&text);
        let mut ctx = RecordingContext::new();
        let gga = nmea::sentence("GPGGA,120000,5540.566,N,01234.100,E,1,07,1.0,12.5,M,41.0,M,,");
        let gsv = nmea::sentence("GPGSV,1,1,00");
        let damaged = gga.replace("5540", "5541");
        feed(c.as_mut(), &mut ctx, &[(5, &format!("{gsv}\r\n{gga}\r\n{damaged}\r\n"))]);
        assert_eq!(ctx.published.len(), 1);
        let Reading::Position(p) = &ctx.published[0].1 else { panic!() };
        assert_eq!((p.t_us, p.fix, p.satellites), (5, position::Fix::Fix3d, 7));
        assert_eq!(ctx.published[0].0, "position");
        assert_eq!(ctx.last_health(), Some(&Health::Running), "one damaged sentence is no problem yet");
    }
}
