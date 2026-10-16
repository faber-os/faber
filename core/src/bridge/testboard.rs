//! Boards behind a fake serial port, for tests: the Arduino library's real C++ core
//! (`faberlink_interop::FakeBoard`), or [`ScriptedBoard`], a small board written with the Rust
//! messages that hosts whatever parts a test gives it (a drive base, new firmware…).
//!
//! The port runs the board on the core's clock: every read ticks it (its `poll()`), every write
//! feeds it. Unplugging closes the port the way a pulled USB cable does.

use crate::clock::SharedClock;
use crate::ports::{PortEntry, PortId, PortKind, PortOpener, PortReader, PortWriter};
use faber_capabilities::{Info, motor};
use faberlink::Message;
use faberlink::frame::Decoder;
use faberlink::message::*;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// The board side of a fake port.
pub trait Board: Send + 'static {
    fn feed(&mut self, bytes: &[u8], now_ms: u32);
    fn tick(&mut self, now_ms: u32);
    fn take_output(&mut self) -> Vec<u8>;
}

impl Board for faberlink_interop::FakeBoard {
    fn feed(&mut self, bytes: &[u8], now_ms: u32) {
        faberlink_interop::FakeBoard::feed(self, bytes, now_ms);
    }
    fn tick(&mut self, now_ms: u32) {
        faberlink_interop::FakeBoard::tick(self, now_ms);
    }
    fn take_output(&mut self) -> Vec<u8> {
        faberlink_interop::FakeBoard::take_output(self)
    }
}

struct WireState<B> {
    board: B,
    plugged: bool,
    opens: u32,
    /// Everything the core wrote, for tests that look at the frames.
    written: Vec<u8>,
}

/// A board on a fake serial port, and the opener that opens it.
pub struct Wire<B: Board> {
    state: Arc<Mutex<WireState<B>>>,
    clock: SharedClock,
}

impl<B: Board> Clone for Wire<B> {
    fn clone(&self) -> Self {
        Wire { state: Arc::clone(&self.state), clock: Arc::clone(&self.clock) }
    }
}

impl<B: Board> Wire<B> {
    pub fn new(board: B, clock: SharedClock) -> Wire<B> {
        let state = WireState { board, plugged: true, opens: 0, written: Vec::new() };
        Wire { state: Arc::new(Mutex::new(state)), clock }
    }

    fn lock(&self) -> MutexGuard<'_, WireState<B>> {
        self.state.lock().unwrap()
    }

    /// Looks at (or changes) the board.
    pub fn board<R>(&self, f: impl FnOnce(&mut B) -> R) -> R {
        f(&mut self.lock().board)
    }

    pub fn unplug(&self) {
        self.lock().plugged = false;
    }

    pub fn plug_in(&self) {
        self.lock().plugged = true;
    }

    pub fn opens(&self) -> u32 {
        self.lock().opens
    }

    /// Every message the core has sent so far.
    pub fn sent(&self) -> Vec<Message> {
        decode(&self.lock().written)
    }

    pub fn entry() -> PortEntry {
        PortEntry {
            id: PortId::Serial("FAKE".into()),
            kind: PortKind::Serial,
            label: "FAKE".into(),
            path: Some("FAKE".into()),
            usb: None,
        }
    }

    fn now_ms(&self) -> u32 {
        (self.clock.now_us() / 1000) as u32
    }
}

fn unplugged() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "unplugged")
}

struct WireReader<B: Board>(Wire<B>);
struct WireWriter<B: Board>(Wire<B>);

impl<B: Board> PortReader for WireReader<B> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(1));
        let now = self.0.now_ms();
        let mut s = self.0.lock();
        if !s.plugged {
            return Err(unplugged());
        }
        s.board.tick(now);
        let out = s.board.take_output();
        assert!(out.len() <= buf.len(), "the fake board wrote more than one read takes");
        buf[..out.len()].copy_from_slice(&out);
        Ok(out.len())
    }
}

impl<B: Board> PortWriter for WireWriter<B> {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let now = self.0.now_ms();
        let mut s = self.0.lock();
        if !s.plugged {
            return Err(unplugged());
        }
        s.written.extend_from_slice(data);
        s.board.feed(data, now);
        Ok(())
    }
}

impl<B: Board> PortOpener for Wire<B> {
    fn open(&self, _: &PortEntry, _: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
        let mut s = self.lock();
        if !s.plugged {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such port"));
        }
        s.opens += 1;
        drop(s);
        Ok((Box::new(WireReader(self.clone())), Box::new(WireWriter(self.clone()))))
    }
}

/// Every message in `bytes`.
pub fn decode(bytes: &[u8]) -> Vec<Message> {
    let mut d = Decoder::new();
    let mut out = Vec::new();
    for &b in bytes {
        if let Some(Ok(f)) = d.push(b)
            && let Ok(m) = Message::decode(1, f.kind, f.payload)
        {
            out.push(m);
        }
    }
    out
}

/// A board written with the Rust messages. It answers HELLO and DESCRIBE with the parts it is
/// given, applies every command as it comes, and sends STATUS every 100 ms. It has none of the
/// library's safety rules: tests that need those use the C++ board.
pub struct ScriptedBoard {
    pub name: String,
    pub parts: Vec<(String, Info)>,
    /// Speaks only this protocol version range.
    pub versions: (u8, u8),
    /// Doesn't answer at all (not a FaberLink board).
    pub mute: bool,
    pub hardware_estop: bool,
    /// Says it has a fault (STATUS health 2) after refusing a command.
    pub fault: bool,
    /// Sends no STATUS (old firmware, or a stuck loop).
    pub no_status: bool,
    pub commands: Vec<PartCommand>,
    decoder: Decoder,
    seq: u8,
    out: Vec<u8>,
    connected: bool,
    last_status_ms: u32,
}

impl ScriptedBoard {
    pub fn new(name: &str, parts: Vec<(&str, Info)>) -> ScriptedBoard {
        ScriptedBoard {
            name: name.into(),
            parts: parts.into_iter().map(|(n, i)| (n.to_string(), i)).collect(),
            versions: (1, 1),
            mute: false,
            hardware_estop: false,
            fault: false,
            no_status: false,
            commands: Vec::new(),
            decoder: Decoder::new(),
            seq: 0,
            out: Vec::new(),
            connected: false,
            last_status_ms: 0,
        }
    }

    /// A board with one motor, `left`.
    pub fn one_motor() -> ScriptedBoard {
        ScriptedBoard::new("scripted", vec![("left", motor_info())])
    }

    fn send(&mut self, m: Message) {
        let wire = m.to_wire(1, self.seq).expect("a test message encodes");
        self.seq = self.seq.wrapping_add(1);
        self.out.extend(wire);
    }

    /// Presses the board's stop button: ESTOP now, and STATUS says so until it is reset.
    pub fn press_estop(&mut self) {
        self.hardware_estop = true;
        self.send(Message::Estop(Estop {
            engaged: true,
            source: faber_capabilities::estop::Source::Part,
            reason: "stop button".into(),
        }));
    }

    fn describe(&mut self) {
        let count = self.parts.len() as u8;
        let parts = self.parts.clone();
        for (id, (name, info)) in parts.into_iter().enumerate() {
            let lease_ms = if info.capability().lease().is_some() { 250 } else { 0 };
            self.send(Message::Part(PartDescription {
                id: id as u8,
                count,
                lease_ms,
                capability: info.capability().id().into(),
                name,
                info: PartInfo::Known(info),
            }));
        }
    }
}

pub fn motor_info() -> Info {
    Info::Motor(motor::Info {
        kind: motor::Kind::Dc,
        modes: motor::Modes { velocity: true, position: false, duty: true },
        max_velocity_rad_s: Some(10.0),
    })
}

impl Board for ScriptedBoard {
    fn feed(&mut self, bytes: &[u8], _now_ms: u32) {
        let mut frames = Vec::new();
        for &b in bytes {
            if let Some(Ok(f)) = self.decoder.push(b) {
                frames.push((f.kind, f.payload.to_vec()));
            }
        }
        for (kind, payload) in frames {
            if self.mute {
                continue;
            }
            match Message::decode(1, kind, &payload) {
                Ok(Message::Hello(h)) => {
                    let (lo, hi) = (h.min_version.max(self.versions.0), h.max_version.min(self.versions.1));
                    let version = if lo <= hi { hi } else { 0 };
                    self.connected = version != 0;
                    let ack = HelloAck {
                        version,
                        min_version: self.versions.0,
                        max_version: self.versions.1,
                        max_payload: 250,
                        heartbeat_timeout_ms: 250,
                        part_count: self.parts.len() as u8,
                        board: self.name.clone(),
                        firmware: "scripted".into(),
                        firmware_version: "1.0.0".into(),
                        build_id: None,
                    };
                    self.send(Message::HelloAck(ack));
                }
                Ok(Message::Describe) if self.connected => self.describe(),
                Ok(Message::Command(c)) if self.connected => {
                    self.commands.push(c);
                    let applied =
                        Applied { part: c.part, cause: Cause::Command, clamped: false, command: c.command };
                    self.send(Message::Applied(applied));
                }
                _ => {}
            }
        }
    }

    fn tick(&mut self, now_ms: u32) {
        if self.connected && !self.no_status && now_ms.wrapping_sub(self.last_status_ms) >= 100 {
            self.last_status_ms = now_ms;
            let status = Status {
                health: if self.fault {
                    Health::Fault
                } else if self.hardware_estop {
                    Health::Degraded
                } else {
                    Health::Ok
                },
                hardware_estop: self.hardware_estop,
                estop: self.hardware_estop,
                failsafe: false,
                uptime_ms: now_ms,
                last_fault: if self.fault { Fault::RefusedCommand } else { Fault::None },
                rx_bad: 0,
                rx_gaps: 0,
            };
            self.send(Message::Status(status));
        }
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }
}
