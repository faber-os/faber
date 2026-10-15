//! "Is there a FaberLink board on this port?" (detect, then suggest). The probe says HELLO a few
//! times and listens for HELLO_ACK, and nothing else: it sends no command, no heartbeat and no
//! DESCRIBE, so a board it finds keeps every actuator stopped.
//!
//! It is only ever run when someone asks, on a port nobody uses (docs/faberlink.md §9): opening a
//! serial port resets most Arduino-based devices (a 3D printer mid-print, too), and bytes they
//! don't expect may confuse them, so Faber never probes ports on its own.

use crate::clock::SharedClock;
use crate::ports::{PortReader, PortWriter};
use faberlink::session::{BoardInfo, Event, Session, SessionConfig};
use std::io;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeTiming {
    /// How long to listen. An Arduino restarts when its port opens and answers about a second
    /// later, so this is a little longer than that.
    pub window: Duration,
    /// How often to say HELLO meanwhile.
    pub hello_every: Duration,
}

impl Default for ProbeTiming {
    fn default() -> Self {
        ProbeTiming { window: Duration::from_millis(2500), hello_every: Duration::from_millis(250) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProbeOutcome {
    /// A FaberLink board answered.
    Found(BoardInfo),
    /// A FaberLink board answered, but it speaks another protocol version (what to update).
    OtherVersion { message: String },
    /// Nothing answered in time.
    Nothing,
}

/// Says HELLO on an open port and listens for up to `timing.window`.
pub fn probe(
    mut reader: Box<dyn PortReader>,
    writer: &mut dyn PortWriter,
    clock: &SharedClock,
    timing: &ProbeTiming,
) -> io::Result<ProbeOutcome> {
    let start = clock.now_us();
    let config =
        SessionConfig { retry_us: timing.hello_every.as_micros() as u64, ..SessionConfig::default() };
    let mut session = Session::new(config, start);
    writer.write(&session.take_output())?;
    let mut buf = vec![0u8; 1024];
    while clock.now_us().saturating_sub(start) < timing.window.as_micros() as u64 {
        let n = reader.read(&mut buf)?;
        let now = clock.now_us();
        session.receive(&buf[..n], now);
        for e in session.take_events() {
            match e {
                Event::Connected(info) => return Ok(ProbeOutcome::Found(info)),
                Event::Failed(e) => return Ok(ProbeOutcome::OtherVersion { message: e.to_string() }),
                _ => {}
            }
        }
        session.tick(now);
        let out = session.take_output();
        if !out.is_empty() {
            writer.write(&out)?;
        }
        if n == 0 {
            // A reader that never blocks (a test's) must not spin.
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    Ok(ProbeOutcome::Nothing)
}
