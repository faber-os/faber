//! Replays the recording in captures/ through the connector, the way Faber does, and checks the
//! readings. Record your own device in Faber's serial monitor (Record), put the .fbcap file in
//! captures/, and test against it here.

use faber_capabilities::{Reading, distance};
use faber_connector::testing::RecordingContext;
use faber_connector::{Health, Settings, capture};

#[test]
fn the_sample_capture_gives_distances() {
    let chunks = capture::decode(include_bytes!("../captures/sample.fbcap")).expect("a .fbcap file");
    let mut connector = (my_sensor::definition().create)(&Settings::new()).expect("default settings");
    let mut ctx = RecordingContext::new();
    for chunk in &chunks {
        connector.on_bytes(&mut ctx, chunk.t_us, &chunk.bytes);
    }

    let metres: Vec<f32> = ctx
        .published
        .iter()
        .map(|(signal, reading)| {
            assert_eq!(signal, "distance");
            let Reading::Distance(d) = reading else { panic!("{reading:?}") };
            assert_eq!(d.status, distance::Status::Ok);
            d.meters
        })
        .collect();
    assert_eq!(metres, [1.23, 1.18, 0.95, 0.42, 0.4]);
    assert_eq!(ctx.last_health(), Some(&Health::Running));
}

#[test]
fn faber_toml_is_a_valid_wasm_connector() {
    let package = faber_connector::manifest_file::parse(include_str!("../faber.toml")).unwrap();
    assert_eq!(package.kind, faber_connector::ConnectorKind::Wasm);
    assert_eq!(package.manifest, my_sensor::manifest());
}
