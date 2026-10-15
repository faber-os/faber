//! Fingerprints the API: a hash of the TypeScript types generated from the API's Rust types
//! (`shell/src/lib/api/generated`). The shell's build computes the same hash
//! (`shell/apiFingerprint.ts`), and `GET /api/v0/system` reports this one, so a shell and a core
//! that don't speak the same API notice it instead of failing with 404s.

#[path = "build/api_fingerprint.rs"]
mod api_fingerprint;

use std::path::Path;

fn main() {
    let dir = Path::new("../shell/src/lib/api/generated");
    println!("cargo:rerun-if-changed={}", dir.display());
    let api = api_fingerprint::fingerprint(dir).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=FABER_API={api}");
}
