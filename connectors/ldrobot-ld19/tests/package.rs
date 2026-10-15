//! The LD19's `faber.toml` (for its wasm package) says what its Rust manifest says.

use faber_connector::{ConnectorKind, manifest_file};

#[test]
fn faber_toml_matches_the_built_in_manifest() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/faber.toml")).unwrap();
    let package = manifest_file::parse(&text).expect("a valid faber.toml");
    assert_eq!(package.kind, ConnectorKind::Wasm);
    assert_eq!(package.manifest, ldrobot_ld19::manifest());
}
