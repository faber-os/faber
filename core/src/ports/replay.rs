//! Replay ports: captures (`.fbcap`) played back as if a real part were plugged in.

use super::stream::PortReader;
use faber_connector::capture::{self, Chunk};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// The sample recording (D32): a real LD19 in Mikkel's living room. Not a port until the user
/// adds it to Recordings (`POST /api/v0/files/samples`); tests use it directly.
pub const SAMPLE_LD19_NAME: &str = "ld19-sample";
pub const SAMPLE_LD19_BYTES: &[u8] = include_bytes!("../../../fixtures/captures/ld19-room-walkthrough.fbcap");
/// Which connector made the sample (data about the recording, not a dependency on the connector).
const SAMPLE_LD19_CONNECTOR: &str = "ldrobot-ld19";
/// Kept for tests that predate the Recordings folder.
pub const DEMO_LD19: &str = "demo-ld19";

#[derive(Clone)]
pub struct ReplayEntry {
    pub chunks: Arc<Vec<Chunk>>,
    /// The connector the capture was made with, if known (used for suggestions).
    pub recorded_with: Option<String>,
}

#[derive(Default)]
struct Inner {
    /// Added in code (tests). Never removed by a folder reload.
    pinned: BTreeMap<String, ReplayEntry>,
    /// Mirrors the Recordings folder; the key also remembers the file's size and modified time
    /// so unchanged files aren't decoded again on every port scan.
    from_dir: BTreeMap<String, (ReplayEntry, (u64, Option<std::time::SystemTime>))>,
}

/// The recordings that are replay ports. Cloning shares it, so the port opener always sees what
/// the machine last loaded.
#[derive(Clone, Default)]
pub struct ReplayLibrary {
    inner: Arc<RwLock<Inner>>,
}

impl ReplayLibrary {
    /// A library with the sample pinned as `replay:demo-ld19`. For tests.
    pub fn with_demo() -> ReplayLibrary {
        let lib = ReplayLibrary::default();
        let chunks = capture::decode(SAMPLE_LD19_BYTES).expect("the sample capture is valid");
        lib.insert(DEMO_LD19, chunks, Some(SAMPLE_LD19_CONNECTOR.to_string()));
        lib
    }

    pub fn insert(&self, name: &str, chunks: Vec<Chunk>, recorded_with: Option<String>) {
        let entry = ReplayEntry { chunks: Arc::new(chunks), recorded_with };
        self.inner.write().expect("replay lock").pinned.insert(name.to_string(), entry);
    }

    /// Makes the folder entries match every `*.fbcap` in `dir` (`replay:<file name>`). A broken
    /// file is skipped with a warning naming it, so one bad capture never stops the core.
    /// Returns true if the set of names changed.
    pub fn sync_dir(&self, dir: &Path) -> bool {
        let mut found = BTreeMap::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("fbcap") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
            let Ok(meta) = entry.metadata() else { continue };
            found.insert(name, (path, (meta.len(), meta.modified().ok())));
        }
        let mut inner = self.inner.write().expect("replay lock");
        let before: Vec<String> = inner.from_dir.keys().cloned().collect();
        let mut next = BTreeMap::new();
        for (name, (path, stamp)) in found {
            if let Some((entry, old)) = inner.from_dir.remove(&name)
                && old == stamp
            {
                next.insert(name, (entry, stamp));
                continue;
            }
            match std::fs::read(&path).map(|b| capture::decode(&b)) {
                Ok(Ok(chunks)) => {
                    let recorded_with = (name == SAMPLE_LD19_NAME).then(|| SAMPLE_LD19_CONNECTOR.to_string());
                    next.insert(name, (ReplayEntry { chunks: Arc::new(chunks), recorded_with }, stamp));
                }
                Ok(Err(_)) => {
                    tracing::warn!(target: "core::ports", "{} is not a Faber recording; skipped.", path.display())
                }
                Err(e) => tracing::warn!(target: "core::ports", "could not read {}: {e}", path.display()),
            }
        }
        let changed = next.keys().cloned().collect::<Vec<_>>() != before;
        inner.from_dir = next;
        changed
    }

    pub fn names(&self) -> Vec<String> {
        let inner = self.inner.read().expect("replay lock");
        let mut names: Vec<String> = inner.pinned.keys().chain(inner.from_dir.keys()).cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn get(&self, name: &str) -> Option<ReplayEntry> {
        let inner = self.inner.read().expect("replay lock");
        inner.pinned.get(name).cloned().or_else(|| inner.from_dir.get(name).map(|(e, _)| e.clone()))
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
    fn sync_dir_mirrors_the_folder_and_skips_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bench.fbcap"), capture::encode(&chunks())).unwrap();
        std::fs::write(dir.path().join("broken.fbcap"), b"not a capture").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"ignored").unwrap();
        let lib = ReplayLibrary::default();
        let shared = lib.clone();
        assert!(lib.sync_dir(dir.path()));
        assert_eq!(shared.names(), ["bench"], "clones see the same library");
        assert!(!lib.sync_dir(dir.path()), "nothing changed");
        std::fs::remove_file(dir.path().join("bench.fbcap")).unwrap();
        assert!(lib.sync_dir(dir.path()));
        assert!(lib.names().is_empty());
    }

    #[test]
    fn the_sample_knows_its_connector() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ld19-sample.fbcap"), SAMPLE_LD19_BYTES).unwrap();
        let lib = ReplayLibrary::default();
        lib.sync_dir(dir.path());
        assert_eq!(lib.get(SAMPLE_LD19_NAME).unwrap().recorded_with.as_deref(), Some("ldrobot-ld19"));
    }
}
