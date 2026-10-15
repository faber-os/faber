//! Shared by `build.rs` and its test (`tests/api_fingerprint.rs`).

use std::path::Path;

/// FNV-1a 64 over every file, sorted by name: name, a zero byte, the contents with `\r` removed
/// (git may check files out with CRLF), a zero byte. Keep in step with `shell/apiFingerprint.ts`.
pub fn fingerprint(dir: &Path) -> Option<String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "ts"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for name in files {
        let text = std::fs::read(dir.join(&name)).ok()?;
        let text: Vec<u8> = text.into_iter().filter(|&b| b != b'\r').collect();
        feed(name.as_bytes());
        feed(&[0]);
        feed(&text);
        feed(&[0]);
    }
    Some(format!("{h:016x}"))
}
