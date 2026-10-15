//! The core and the shell must compute the same API fingerprint; both test against this value.

#[path = "../build/api_fingerprint.rs"]
mod api_fingerprint;

use std::path::Path;

/// Same constant as `shell/apiFingerprint.test.ts`.
const FIXTURE: &str = "94e7e7bc99bf6f23";

#[test]
fn the_fingerprint_matches_the_shells() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/api-fingerprint");
    assert_eq!(api_fingerprint::fingerprint(&dir).as_deref(), Some(FIXTURE));
}

#[test]
fn the_core_reports_the_fingerprint_of_the_committed_types() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shell/src/lib/api/generated");
    assert_eq!(api_fingerprint::fingerprint(&dir).as_deref(), Some(env!("FABER_API")));
}
