//! Replay ports: captures (`.fbcap`) played back as if a real part were plugged in.

use super::stream::PortReader;
use faber_connector::capture::{self, Chunk};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The built-in demo: a real LD19 in Mikkel's living room, so Faber can be tried without hardware.
pub const DEMO_LD19: &str = "demo-ld19";
static DEMO_LD19_BYTES: &[u8] = include_bytes!("../../../fixtures/captures/ld19-room-walkthrough.fbcap");

#[derive(Clone)]
pub struct ReplayEntry {
    pub chunks: Arc<Vec<Chunk>>,
    /// The connector the capture was made with, if known (used for suggestions).
    pub recorded_with: Option<String>,
}

#[derive(Clone, Default)]
pub struct ReplayLibrary {
    entries: BTreeMap<String, ReplayEntry>,
}

impl ReplayLibrary {
    /// The library with only the built-in demo.
    pub fn with_demo() -> ReplayLibrary {
        let chunks = capture::decode(DEMO_LD19_BYTES).expect("the built-in demo capture is valid");
        let mut lib = ReplayLibrary::default();
        // Data about the recording, not a dependency on the connector.
        lib.insert(DEMO_LD19, chunks, Some("ldrobot-ld19".to_string()));
        lib
    }

    pub fn insert(&mut self, name: &str, chunks: Vec<Chunk>, recorded_with: Option<String>) {
        self.entries.insert(name.to_string(), ReplayEntry { chunks: Arc::new(chunks), recorded_with });
    }

    /// Adds every `*.fbcap` in `dir` as `replay:<file name>`. A broken file is skipped with a
    /// warning that says which file and why, so one bad capture never stops the core.
    pub fn load_dir(&mut self, dir: &Path) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("fbcap") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
            match std::fs::read(&path).map(|b| capture::decode(&b)) {
                Ok(Ok(chunks)) => self.insert(&name, chunks, None),
                Ok(Err(_)) => {
                    tracing::warn!(target: "core::ports", "{} is not a Faber capture file; skipped.", path.display())
                }
                Err(e) => tracing::warn!(target: "core::ports", "could not read {}: {e}", path.display()),
            }
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&ReplayEntry> {
        self.entries.get(name)
    }
}

/// How fast a replay plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    /// At the speed it was recorded, looping forever. What users get.
    RealTime,
    /// As fast as it can be read, once, then the port closes. For tests.
    Instant,
}

/// A capture as a readable port.
pub struct Replay {
    chunks: Arc<Vec<Chunk>>,
    pacing: Pacing,
    next: usize,
    /// Bytes of the current chunk not yet handed out (when it is bigger than the read buffer).
    pending: Vec<u8>,
    loop_start: Instant,
}

/// The longest a read waits before returning "nothing yet", so the reader can notice a stop.
const MAX_WAIT: Duration = Duration::from_millis(50);

impl Replay {
    pub fn new(chunks: Arc<Vec<Chunk>>, pacing: Pacing) -> Replay {
        Replay { chunks, pacing, next: 0, pending: Vec::new(), loop_start: Instant::now() }
    }

    /// When chunk `i` is due, relative to the start of the current loop.
    fn due(&self, i: usize) -> Duration {
        let first = self.chunks.first().map_or(0, |c| c.t_us);
        Duration::from_micros(self.chunks[i].t_us.saturating_sub(first))
    }
}

impl PortReader for Replay {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            if self.next >= self.chunks.len() {
                match self.pacing {
                    Pacing::Instant => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the recording ended"));
                    }
                    Pacing::RealTime if self.chunks.is_empty() => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the recording is empty"));
                    }
                    Pacing::RealTime => {
                        self.next = 0;
                        self.loop_start = Instant::now();
                    }
                }
            }
            if self.pacing == Pacing::RealTime {
                let wait = self.due(self.next).saturating_sub(self.loop_start.elapsed());
                if wait > Duration::ZERO {
                    std::thread::sleep(wait.min(MAX_WAIT));
                    if wait > MAX_WAIT {
                        return Ok(0);
                    }
                }
            }
            self.pending = self.chunks[self.next].bytes.clone();
            self.next += 1;
        }
        let n = self.pending.len().min(buf.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks() -> Arc<Vec<Chunk>> {
        Arc::new(vec![
            Chunk { t_us: 1_000, bytes: vec![1, 2, 3, 4, 5] },
            Chunk { t_us: 2_000, bytes: vec![6] },
        ])
    }

    fn drain(r: &mut Replay) -> (Vec<u8>, io::Error) {
        let mut out = Vec::new();
        let mut buf = [0u8; 2];
        loop {
            match r.read(&mut buf) {
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) => return (out, e),
            }
        }
    }

    #[test]
    fn instant_replay_hands_out_every_byte_then_ends() {
        let mut r = Replay::new(chunks(), Pacing::Instant);
        let (bytes, end) = drain(&mut r);
        assert_eq!(bytes, [1, 2, 3, 4, 5, 6], "chunks bigger than the buffer are split, never lost");
        assert_eq!(end.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn real_time_replay_loops() {
        let mut r = Replay::new(chunks(), Pacing::RealTime);
        let mut buf = [0u8; 16];
        let mut got = Vec::new();
        let start = Instant::now();
        while got.len() < 12 && start.elapsed() < Duration::from_secs(2) {
            let n = r.read(&mut buf).unwrap();
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(got, [1, 2, 3, 4, 5, 6, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn the_demo_is_a_valid_ld19_capture() {
        let lib = ReplayLibrary::with_demo();
        let demo = lib.get(DEMO_LD19).unwrap();
        assert!(demo.chunks.len() > 100);
        assert_eq!(demo.recorded_with.as_deref(), Some("ldrobot-ld19"));
        assert_eq!(lib.names(), [DEMO_LD19]);
    }

    #[test]
    fn load_dir_adds_captures_and_skips_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bench.fbcap"), capture::encode(&chunks())).unwrap();
        std::fs::write(dir.path().join("broken.fbcap"), b"not a capture").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"ignored").unwrap();
        let mut lib = ReplayLibrary::default();
        lib.load_dir(dir.path());
        assert_eq!(lib.names(), ["bench"]);
    }
}
