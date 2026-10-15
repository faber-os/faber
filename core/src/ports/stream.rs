//! Opening ports as byte streams, and the reader thread that turns a stream into events.

use super::replay::{Pacing, Replay, ReplayLibrary};
use super::{PortEntry, PortId};
use crate::clock::SharedClock;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Reads from a port. `Ok(0)` means "nothing yet"; implementations wait at most ~50 ms so the
/// reader can notice when it should stop.
pub trait PortReader: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

pub trait PortWriter: Send {
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
}

/// What a port's reader thread reports.
#[derive(Debug, PartialEq, Eq)]
pub enum PortEvent {
    Data {
        t_us: u64,
        bytes: Vec<u8>,
    },
    /// The port stopped working (unplugged, a replay that ended, an OS error).
    Closed {
        reason: String,
    },
}

/// Opens ports. A trait so tests can use in-memory ports.
pub trait PortOpener: Send + Sync {
    fn open(&self, port: &PortEntry, baud: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)>;
}

/// Opens real serial ports and replay ports.
pub struct SystemOpener {
    replays: ReplayLibrary,
}

impl SystemOpener {
    pub fn new(replays: ReplayLibrary) -> SystemOpener {
        SystemOpener { replays }
    }
}

struct SerialReader(Box<dyn serialport::SerialPort>);
struct SerialWriter(Box<dyn serialport::SerialPort>);
struct NoWriter;

impl PortReader for SerialReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(0),
            // Some drivers report a closed handle as a successful empty read; treat it as closed.
            Ok(0) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "the port closed")),
            other => other,
        }
    }
}

impl PortWriter for SerialWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        io::Write::write_all(&mut self.0, data)
    }
}

impl PortWriter for NoWriter {
    fn write(&mut self, _data: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

impl PortOpener for SystemOpener {
    fn open(&self, port: &PortEntry, baud: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
        if let PortId::Replay(name) = &port.id {
            let entry = self.replays.get(name).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("no recording called {name}"))
            })?;
            return Ok((Box::new(Replay::new(entry.chunks, Pacing::RealTime)), Box::new(NoWriter)));
        }
        let path = port
            .path
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "port has no OS name"))?;
        let serial = serialport::new(path, baud).timeout(Duration::from_millis(50)).open()?;
        let writer = serial.try_clone()?;
        Ok((Box::new(SerialReader(serial)), Box::new(SerialWriter(writer))))
    }
}

/// Reads `reader` on its own thread, stamping each chunk with the core clock, and hands every
/// event to `sink`. Ends when `stop` is set, when `sink` returns false (nobody listens any more),
/// or when the port closes (after a final `Closed` event).
pub fn spawn_reader(
    mut reader: Box<dyn PortReader>,
    clock: SharedClock,
    mut sink: impl FnMut(PortEvent) -> bool + Send + 'static,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        while !stop.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    if !sink(PortEvent::Data { t_us: clock.now_us(), bytes: buf[..n].to_vec() }) {
                        return;
                    }
                }
                Err(e) => {
                    sink(PortEvent::Closed { reason: e.to_string() });
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use faber_connector::capture::Chunk;
    use std::sync::mpsc;

    #[test]
    fn reader_stamps_chunks_and_reports_the_end() {
        let chunks = Arc::new(vec![Chunk { t_us: 0, bytes: vec![7, 8] }]);
        let clock = Arc::new(ManualClock::new(500));
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let h = spawn_reader(
            Box::new(Replay::new(chunks, Pacing::Instant)),
            clock,
            move |e| tx.send(e).is_ok(),
            stop,
        );
        assert_eq!(rx.recv().unwrap(), PortEvent::Data { t_us: 500, bytes: vec![7, 8] });
        assert!(matches!(rx.recv().unwrap(), PortEvent::Closed { .. }));
        h.join().unwrap();
    }

    #[test]
    fn reader_stops_when_asked() {
        let chunks =
            Arc::new(vec![Chunk { t_us: 0, bytes: vec![1] }, Chunk { t_us: 60_000_000, bytes: vec![2] }]);
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let h = spawn_reader(
            Box::new(Replay::new(chunks, Pacing::RealTime)),
            Arc::new(ManualClock::new(0)),
            move |e| tx.send(e).is_ok(),
            Arc::clone(&stop),
        );
        assert!(matches!(rx.recv().unwrap(), PortEvent::Data { .. }));
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap(); // returns within ~50 ms instead of waiting a minute for the next chunk
    }

    #[test]
    fn opening_a_missing_replay_explains_why() {
        let opener = SystemOpener::new(ReplayLibrary::default());
        let port = PortEntry {
            id: PortId::Replay("nope".into()),
            kind: super::super::PortKind::Replay,
            label: "x".into(),
            path: None,
            usb: None,
        };
        let err = opener.open(&port, 9600).err().unwrap();
        assert!(err.to_string().contains("nope"));
    }
}
