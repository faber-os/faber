//! The core's side of one FaberLink connection, as a pure state machine: bytes and the time go
//! in, bytes and events come out. It does no I/O and reads no clock, so it is tested with a fake
//! clock and an in-memory pipe.
//!
//! ```text
//! Connecting ──HELLO_ACK──► Describing ──all PARTs──► Ready
//!     ▲  (HELLO every 500 ms)    (DESCRIBE every 500 ms)  │
//!     └──────────── the board is silent for 500 ms ◄──────┘
//! A version mismatch ends in Failed until restart().
//! ```

use crate::frame::{self, BadFrame};
use crate::message::*;
use crate::{EncodeError, MAX_VERSION, MIN_VERSION};
use faber_capabilities::{Capability, Command, Reading, estop};
use std::fmt;

/// Timings, all in microseconds of the clock passed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    /// How often a `HEARTBEAT` goes out once the board has answered.
    pub heartbeat_interval_us: u64,
    /// How long to wait for `HELLO_ACK` or all `PART`s before asking again.
    pub retry_us: u64,
    /// No frame from the board for this long and it counts as lost (it sends `STATUS` every
    /// 100 ms).
    pub board_silence_us: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig { heartbeat_interval_us: 50_000, retry_us: 500_000, board_silence_us: 500_000 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Sending `HELLO` until the board answers.
    Connecting,
    /// Collecting the board's `PART`s.
    Describing,
    /// Parts are known: readings flow and commands may be sent.
    Ready,
    /// The two sides can't talk (see [`SessionError`]); nothing is sent until [`Session::restart`].
    Failed,
}

/// What the board said about itself in `HELLO_ACK`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardInfo {
    pub version: u8,
    pub board: String,
    pub firmware: String,
    pub firmware_version: String,
    /// The board stops every actuator when no heartbeat arrives for this long.
    pub heartbeat_timeout_ms: u16,
    pub max_payload: u8,
    pub part_count: u8,
    /// The build of the sketch, when Faber's Arduino editor compiled it (docs/arduino-editor.md).
    pub build_id: Option<u64>,
}

/// Why a session can't continue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// The board and the core have no protocol version in common.
    VersionMismatch { core: (u8, u8), board: (u8, u8), board_name: String, firmware: String },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let range =
            |(a, b): (u8, u8)| if a == b { format!("version {a}") } else { format!("versions {a}–{b}") };
        match self {
            SessionError::VersionMismatch { core, board, board_name, firmware } => {
                let fix = if board.0 > core.1 {
                    "Update Faber to talk to this board"
                } else {
                    "Update the board's firmware (its FaberLink library) to talk to this Faber"
                };
                write!(
                    f,
                    "The board \"{board_name}\" ({firmware}) speaks FaberLink {}, but this Faber speaks {}. {fix}.",
                    range(*board),
                    range(*core)
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

/// Why a command was not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The parts aren't known yet, or the session failed.
    NotReady,
    /// Commands go out only while the core holds a lease on the part.
    NoLease,
    UnknownPart(u8),
    /// The command is for another capability than the part's.
    WrongCapability {
        part: u8,
        part_capability: String,
        command: Capability,
    },
    /// The board's buffers are too small for this message.
    TooLargeForBoard {
        size: usize,
        max: u8,
    },
    Encode(EncodeError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NotReady => write!(f, "the board isn't ready yet"),
            CommandError::NoLease => write!(f, "a command needs a lease on the part"),
            CommandError::UnknownPart(p) => write!(f, "the board has no part {p}"),
            CommandError::WrongCapability { part, part_capability, command } => {
                write!(f, "part {part} is a {part_capability}, not a {}", command.id())
            }
            CommandError::TooLargeForBoard { size, max } => {
                write!(f, "the message is {size} bytes, but the board accepts at most {max}")
            }
            CommandError::Encode(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for CommandError {}

/// Something the board said or did, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Connected(BoardInfo),
    /// Every part the board hosts, by local id, and the pins of those whose board said
    /// (`PINS`, sent by libraries from 2026-10-16 on; none from an older one).
    Described {
        parts: Vec<PartDescription>,
        pins: Vec<PartPins>,
    },
    /// A reading, stamped with the core's clock when it arrived.
    Reading {
        part: u8,
        reading: Reading,
    },
    Applied(Applied),
    Status(Status),
    /// The board engaged (or released) the e-stop, e.g. from its own button.
    Estop(Estop),
    Log(Log),
    /// Nothing heard for [`SessionConfig::board_silence_us`]; the session starts over.
    BoardLost,
    Failed(SessionError),
}

/// Counters for the bridge's health view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub rx_frames: u64,
    /// Dropped for a bad CRC, bad COBS, or being too short or too long.
    pub rx_bad_frames: u64,
    /// A good frame whose payload didn't decode, or a reading that doesn't fit its part.
    pub rx_undecodable: u64,
    /// Frames the board sent that never arrived, counted from gaps in `seq`.
    pub rx_missed: u64,
    pub tx_frames: u64,
}

pub struct Session {
    config: SessionConfig,
    state: State,
    decoder: frame::Decoder,
    out: Vec<u8>,
    events: Vec<Event>,
    tx_seq: u8,
    rx_seq: Option<u8>,
    board: Option<BoardInfo>,
    collected: Vec<Option<PartDescription>>,
    /// `PINS` arrive just before their `PART`, so they are collected the same way.
    collected_pins: Vec<Option<PartPins>>,
    parts: Vec<PartDescription>,
    pins: Vec<PartPins>,
    estop: bool,
    last_rx_us: u64,
    last_request_us: u64,
    last_heartbeat_us: u64,
    stats: Stats,
}

impl Session {
    /// Starts a session: the first `HELLO` is ready in [`Session::take_output`].
    pub fn new(config: SessionConfig, now_us: u64) -> Session {
        let mut s = Session {
            config,
            state: State::Connecting,
            decoder: frame::Decoder::new(),
            out: Vec::new(),
            events: Vec::new(),
            tx_seq: 0,
            rx_seq: None,
            board: None,
            collected: Vec::new(),
            collected_pins: Vec::new(),
            parts: Vec::new(),
            pins: Vec::new(),
            estop: false,
            last_rx_us: now_us,
            last_request_us: now_us,
            last_heartbeat_us: now_us,
            stats: Stats::default(),
        };
        s.connect(now_us);
        s
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn board(&self) -> Option<&BoardInfo> {
        self.board.as_ref()
    }

    /// The board's parts, once [`State::Ready`].
    pub fn parts(&self) -> &[PartDescription] {
        &self.parts
    }

    /// The pins of the board's parts, for those it said (an older library says none).
    pub fn pins(&self) -> &[PartPins] {
        &self.pins
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Bytes to write to the port.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Starts over after [`State::Failed`] (e.g. once the firmware has been updated).
    pub fn restart(&mut self, now_us: u64) {
        self.connect(now_us);
    }

    fn connect(&mut self, now_us: u64) {
        self.state = State::Connecting;
        self.board = None;
        self.parts.clear();
        self.pins.clear();
        self.collected.clear();
        self.collected_pins.clear();
        self.rx_seq = None;
        self.last_rx_us = now_us;
        // A lone 0x00 first ends whatever half frame the board may be holding.
        self.out.push(0);
        self.send_hello(now_us);
    }

    fn send_hello(&mut self, now_us: u64) {
        self.last_request_us = now_us;
        let hello = Hello {
            min_version: MIN_VERSION,
            max_version: MAX_VERSION,
            max_payload: frame::MAX_PAYLOAD as u8,
        };
        self.send(&Message::Hello(hello)).expect("HELLO always encodes");
    }

    fn version(&self) -> u8 {
        self.board.as_ref().map_or(MIN_VERSION, |b| b.version)
    }

    fn send(&mut self, m: &Message) -> Result<(), EncodeError> {
        let wire = m.to_wire(self.version(), self.tx_seq)?;
        self.out.extend_from_slice(&wire);
        self.tx_seq = self.tx_seq.wrapping_add(1);
        self.stats.tx_frames += 1;
        Ok(())
    }

    fn send_heartbeat(&mut self, now_us: u64) {
        self.last_heartbeat_us = now_us;
        self.send(&Message::Heartbeat(Heartbeat { estop: self.estop })).expect("HEARTBEAT always encodes");
    }

    /// Runs the timers: retries, heartbeats, and noticing a silent board. Call it often (every
    /// few ms); it only sends what is due.
    pub fn tick(&mut self, now_us: u64) {
        let since = |t: u64| now_us.saturating_sub(t);
        match self.state {
            State::Failed => {}
            State::Connecting => {
                if since(self.last_request_us) >= self.config.retry_us {
                    self.send_hello(now_us);
                }
            }
            State::Describing | State::Ready => {
                if since(self.last_rx_us) >= self.config.board_silence_us {
                    self.events.push(Event::BoardLost);
                    self.connect(now_us);
                    return;
                }
                if since(self.last_heartbeat_us) >= self.config.heartbeat_interval_us {
                    self.send_heartbeat(now_us);
                }
                if self.state == State::Describing && since(self.last_request_us) >= self.config.retry_us {
                    self.describe(now_us);
                }
            }
        }
    }

    fn describe(&mut self, now_us: u64) {
        self.last_request_us = now_us;
        let n = self.board.as_ref().map_or(0, |b| b.part_count) as usize;
        self.collected = vec![None; n];
        self.collected_pins = vec![None; n];
        if n == 0 {
            self.ready();
            return;
        }
        self.send(&Message::Describe).expect("DESCRIBE always encodes");
    }

    fn ready(&mut self) {
        self.parts = self.collected.drain(..).flatten().collect();
        self.pins = self.collected_pins.drain(..).flatten().collect();
        self.state = State::Ready;
        self.events.push(Event::Described { parts: self.parts.clone(), pins: self.pins.clone() });
    }

    /// Feeds bytes read from the port.
    pub fn receive(&mut self, bytes: &[u8], now_us: u64) {
        for &b in bytes {
            let frame = match self.decoder.push(b) {
                None => continue,
                Some(Err(BadFrame::Cobs | BadFrame::TooShort | BadFrame::Crc | BadFrame::TooLong)) => {
                    self.stats.rx_bad_frames += 1;
                    continue;
                }
                Some(Ok(f)) => (f.kind, f.seq, f.payload.to_vec()),
            };
            self.on_frame(frame.0, frame.1, &frame.2, now_us);
        }
    }

    fn on_frame(&mut self, kind: u8, seq: u8, payload: &[u8], now_us: u64) {
        if self.state == State::Failed {
            return;
        }
        self.stats.rx_frames += 1;
        self.last_rx_us = now_us;
        if let Some(expected) = self.rx_seq {
            self.stats.rx_missed += seq.wrapping_sub(expected) as u64;
        }
        self.rx_seq = Some(seq.wrapping_add(1));
        let message = match Message::decode(self.version(), kind, payload) {
            Ok(m) => m,
            Err(_) => {
                self.stats.rx_undecodable += 1;
                return;
            }
        };
        match message {
            Message::HelloAck(ack) => self.on_hello_ack(ack, now_us),
            // A part's pins come just before it: a PINS after the last PART would be too late.
            Message::Pins(p) if self.state == State::Describing => {
                if let Some(slot) = self.collected_pins.get_mut(p.part as usize) {
                    *slot = Some(p);
                }
            }
            Message::Part(p) if self.state == State::Describing => {
                if let Some(slot) = self.collected.get_mut(p.id as usize) {
                    *slot = Some(p);
                }
                if self.collected.iter().all(Option::is_some) {
                    self.ready();
                }
            }
            Message::Reading(r) if self.state == State::Ready => self.on_reading(r, now_us),
            Message::Applied(a) if self.state == State::Ready => self.events.push(Event::Applied(a)),
            Message::Status(s) => self.events.push(Event::Status(s)),
            Message::Estop(e) => self.events.push(Event::Estop(e)),
            Message::Log(l) => self.events.push(Event::Log(l)),
            // Core → board messages echoed back, or parts and readings out of turn: ignored.
            _ => {}
        }
    }

    fn on_hello_ack(&mut self, ack: HelloAck, now_us: u64) {
        if self.state != State::Connecting {
            return;
        }
        if !(MIN_VERSION..=MAX_VERSION).contains(&ack.version) {
            let e = SessionError::VersionMismatch {
                core: (MIN_VERSION, MAX_VERSION),
                board: (ack.min_version, ack.max_version),
                board_name: ack.board,
                firmware: format!("{} {}", ack.firmware, ack.firmware_version),
            };
            self.state = State::Failed;
            self.events.push(Event::Failed(e));
            return;
        }
        let info = BoardInfo {
            version: ack.version,
            board: ack.board,
            firmware: ack.firmware,
            firmware_version: ack.firmware_version,
            heartbeat_timeout_ms: ack.heartbeat_timeout_ms,
            max_payload: ack.max_payload,
            part_count: ack.part_count,
            build_id: ack.build_id,
        };
        self.board = Some(info.clone());
        self.state = State::Describing;
        self.events.push(Event::Connected(info));
        self.send_heartbeat(now_us);
        self.describe(now_us);
    }

    fn on_reading(&mut self, mut r: PartReading, now_us: u64) {
        let Some(part) = self.parts.iter().find(|p| p.id == r.part) else {
            self.stats.rx_undecodable += 1;
            return;
        };
        if part.capability != r.reading.capability().id() {
            self.stats.rx_undecodable += 1;
            return;
        }
        // The board stamps in its own clock; the core's arrival time is what the router uses.
        set_t_us(&mut r.reading, now_us);
        self.events.push(Event::Reading { part: r.part, reading: r.reading });
    }

    fn part(&self, id: u8) -> Result<&PartDescription, CommandError> {
        if self.state != State::Ready {
            return Err(CommandError::NotReady);
        }
        self.parts.iter().find(|p| p.id == id).ok_or(CommandError::UnknownPart(id))
    }

    /// Sends a command. `lease_held` says that the core holds a lease on the part; without one,
    /// nothing is sent.
    pub fn command(&mut self, part: u8, command: Command, lease_held: bool) -> Result<(), CommandError> {
        let p = self.part(part)?;
        if !lease_held {
            return Err(CommandError::NoLease);
        }
        if p.capability != command.capability().id() {
            return Err(CommandError::WrongCapability {
                part,
                part_capability: p.capability.clone(),
                command: command.capability(),
            });
        }
        let m = Message::Command(PartCommand { part, command });
        let size = m.encode(self.version()).map_err(CommandError::Encode)?.len();
        let max = self.board.as_ref().map_or(0, |b| b.max_payload);
        if size > max as usize {
            return Err(CommandError::TooLargeForBoard { size, max });
        }
        self.send(&m).map_err(CommandError::Encode)
    }

    /// Puts one part (or every part, with `None`) in its safe state now.
    pub fn safe(&mut self, part: Option<u8>) -> Result<(), CommandError> {
        if let Some(id) = part {
            self.part(id)?;
        } else if self.state != State::Ready {
            return Err(CommandError::NotReady);
        }
        self.send(&Message::Safe(Safe { part })).map_err(CommandError::Encode)
    }

    /// Engages or releases the e-stop on the board. It is sent at once and repeated in every
    /// heartbeat, so a lost frame is corrected within one heartbeat.
    pub fn set_estop(&mut self, engaged: bool, source: estop::Source, reason: &str) {
        self.estop = engaged;
        if matches!(self.state, State::Describing | State::Ready) {
            let reason = fit(reason, self.max_reason_len());
            let m = Message::Estop(Estop { engaged, source, reason });
            self.send(&m).expect("a short reason always encodes");
        }
    }
}

impl Session {
    /// The longest e-stop reason that fits the board's buffers (and never more than 60 bytes).
    fn max_reason_len(&self) -> usize {
        let max_payload = self.board.as_ref().map_or(frame::MAX_PAYLOAD as u8, |b| b.max_payload);
        (max_payload as usize).saturating_sub(3).min(60)
    }
}

/// `s` cut to at most `max` bytes, at a character boundary.
fn fit(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn set_t_us(r: &mut Reading, t: u64) {
    match r {
        Reading::LaserScan(x) => x.t_us = t,
        Reading::Distance(x) => x.t_us = t,
        Reading::Motor(x) => x.t_us = t,
        Reading::PlanarBase(x) => x.t_us = t,
        Reading::Estop(x) => x.t_us = t,
        Reading::Pose(x) => x.t_us = t,
        Reading::Position(x) => x.t_us = t,
        Reading::Battery(x) => x.t_us = t,
        Reading::Temperature(x) => x.t_us = t,
        Reading::DigitalIn(x) => x.t_us = t,
        Reading::DigitalOut(x) => x.t_us = t,
        Reading::AnalogIn(x) => x.t_us = t,
        Reading::PwmOut(x) => x.t_us = t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{Info, distance, motor};

    /// A board written with the Rust messages, to test the session on its own.
    struct Board {
        seq: u8,
    }

    impl Board {
        fn wire(&mut self, m: Message) -> Vec<u8> {
            let w = m.to_wire(1, self.seq).unwrap();
            self.seq = self.seq.wrapping_add(1);
            w
        }

        fn ack(&mut self, version: u8, parts: u8) -> Vec<u8> {
            self.wire(Message::HelloAck(HelloAck {
                version,
                min_version: version.max(1),
                max_version: version.max(1),
                max_payload: 250,
                heartbeat_timeout_ms: 250,
                part_count: parts,
                board: "mega".into(),
                firmware: "test".into(),
                firmware_version: "0.1.0".into(),
                build_id: None,
            }))
        }

        fn parts(&mut self) -> Vec<u8> {
            let mut out = self.wire(Message::Part(PartDescription {
                id: 0,
                count: 2,
                lease_ms: 250,
                capability: "motor@1".into(),
                name: "left".into(),
                info: PartInfo::Known(Info::Motor(motor::Info {
                    kind: motor::Kind::Dc,
                    modes: motor::Modes { velocity: false, position: false, duty: true },
                    max_velocity_rad_s: None,
                })),
            }));
            out.extend(self.wire(Message::Part(PartDescription {
                id: 1,
                count: 2,
                lease_ms: 0,
                capability: "distance@1".into(),
                name: "front".into(),
                info: PartInfo::Known(Info::Distance(distance::Info {
                    min_m: 0.1,
                    max_m: 0.8,
                    field_of_view_rad: 0.1,
                    kind: distance::Kind::Infrared,
                })),
            })));
            out
        }

        fn distance(&mut self, part: u8, meters: f32) -> Vec<u8> {
            self.wire(Message::Reading(PartReading {
                part,
                reading: Reading::Distance(distance::Reading {
                    t_us: 5000,
                    meters,
                    status: distance::Status::Ok,
                }),
            }))
        }
    }

    fn sent(bytes: &[u8]) -> Vec<Message> {
        let mut d = frame::Decoder::new();
        let mut out = vec![];
        for &b in bytes {
            if let Some(f) = d.push(b) {
                let f = f.unwrap();
                out.push(Message::decode(1, f.kind, f.payload).unwrap());
            }
        }
        out
    }

    fn ready() -> (Session, Board) {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 1_000);
        s.receive(&b.parts(), 2_000);
        assert_eq!(s.state(), State::Ready);
        s.take_output();
        s.take_events();
        (s, b)
    }

    #[test]
    fn it_says_hello_until_the_board_answers() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let hello = Message::Hello(Hello { min_version: 1, max_version: 1, max_payload: 250 });
        let out = s.take_output();
        assert_eq!(out[0], 0, "a flush byte first");
        assert_eq!(sent(&out), std::slice::from_ref(&hello));
        s.tick(499_000);
        assert!(s.take_output().is_empty());
        s.tick(500_000);
        assert_eq!(sent(&s.take_output()), [hello]);
        assert_eq!(s.state(), State::Connecting);
    }

    #[test]
    fn handshake_and_describe_give_the_parts() {
        let mut s = Session::new(SessionConfig::default(), 0);
        s.take_output();
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 1_000);
        assert_eq!(
            sent(&s.take_output()),
            [Message::Heartbeat(Heartbeat { estop: false }), Message::Describe]
        );
        assert_eq!(s.state(), State::Describing);
        assert_eq!(s.board().unwrap().heartbeat_timeout_ms, 250);
        s.receive(&b.parts(), 2_000);
        assert_eq!(s.state(), State::Ready);
        let names: Vec<_> =
            s.parts().iter().map(|p| (p.id, p.name.as_str(), p.capability.as_str())).collect();
        assert_eq!(names, [(0, "left", "motor@1"), (1, "front", "distance@1")]);
        let events = s.take_events();
        assert!(matches!(&events[0], Event::Connected(b) if b.board == "mega"));
        assert!(
            matches!(&events[1], Event::Described { parts, pins } if parts.len() == 2 && pins.is_empty())
        );
    }

    fn pins(part: u8, list: &[(PinRole, u8)]) -> PartPins {
        PartPins {
            part,
            analog_base: Some(14),
            pins: list.iter().map(|&(role, pin)| PinUse { role, pin }).collect(),
        }
    }

    #[test]
    fn each_parts_pins_arrive_before_it_and_come_with_the_parts() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 1_000);
        let motor = pins(0, &[(PinRole::Pwm, 9), (PinRole::Dir, 8)]);
        let sensor = pins(1, &[(PinRole::Signal, 14)]);
        let parts = sent(&b.parts());
        let mut wire = b.wire(Message::Pins(motor.clone()));
        wire.extend(b.wire(parts[0].clone()));
        wire.extend(b.wire(Message::Pins(sensor.clone())));
        wire.extend(b.wire(parts[1].clone()));
        s.take_events();
        s.receive(&wire, 2_000);
        assert_eq!(s.state(), State::Ready);
        assert_eq!(s.pins(), [motor.clone(), sensor.clone()]);
        let events = s.take_events();
        assert!(
            matches!(&events[..], [Event::Described { parts, pins }] if parts.len() == 2 && *pins == [motor, sensor]),
            "{events:?}"
        );
        assert_eq!(s.stats().rx_undecodable, 0);
    }

    /// What every core since the first one (Phase 4a) does with a message type it doesn't know,
    /// and so what an older core does with `PINS`: count it and carry on. A new library is safe
    /// with an old core (docs/faberlink.md §3).
    #[test]
    fn a_message_type_this_core_doesnt_know_is_skipped_even_while_describing() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 1_000);
        let parts = sent(&Board { seq: 0 }.parts());
        let mut wire = Vec::new();
        for p in parts {
            let mut unknown = vec![0; frame::MAX_WIRE];
            let n = frame::encode(0x7E, b.seq, &[0, 14, 1, 9], &mut unknown).unwrap();
            b.seq = b.seq.wrapping_add(1);
            wire.extend_from_slice(&unknown[..n]);
            wire.extend(b.wire(p));
        }
        s.receive(&wire, 2_000);
        assert_eq!(s.state(), State::Ready);
        assert_eq!(s.parts().len(), 2);
        assert_eq!((s.stats().rx_undecodable, s.stats().rx_missed), (2, 0));
    }

    #[test]
    fn pins_for_a_part_the_board_doesnt_have_or_after_describing_are_ignored() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 1_000);
        s.receive(&b.wire(Message::Pins(pins(5, &[(PinRole::Trig, 7)]))), 1_500);
        s.receive(&b.parts(), 2_000);
        s.receive(&b.wire(Message::Pins(pins(0, &[(PinRole::Pwm, 9)]))), 3_000);
        assert_eq!(s.state(), State::Ready);
        assert!(s.pins().is_empty());
    }

    #[test]
    fn asking_again_forgets_the_pins_heard_so_far() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 0);
        s.receive(&b.wire(Message::Pins(pins(0, &[(PinRole::Pwm, 9)]))), 10_000);
        s.tick(400_000);
        s.receive(&b.distance(1, 0.5), 450_000); // keeps the board alive
        s.tick(500_000); // DESCRIBE again: the new firmware may use other pins
        s.receive(&b.parts(), 510_000);
        assert_eq!(s.state(), State::Ready);
        assert!(s.pins().is_empty());
    }

    #[test]
    fn a_board_without_parts_is_ready_at_once() {
        let mut s = Session::new(SessionConfig::default(), 0);
        s.receive(&Board { seq: 0 }.ack(1, 0), 1_000);
        assert_eq!(s.state(), State::Ready);
        assert!(!sent(&s.take_output()).contains(&Message::Describe));
    }

    #[test]
    fn describe_is_asked_again_if_parts_go_missing() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        s.receive(&b.ack(1, 2), 0);
        s.take_output();
        let parts = b.parts();
        s.receive(&parts[..parts.len() / 2], 10_000);
        s.tick(400_000);
        s.receive(&b.distance(1, 0.5), 450_000); // keeps the board alive; ignored before Ready
        s.tick(500_000);
        assert!(sent(&s.take_output()).contains(&Message::Describe));
        s.receive(&b.parts(), 510_000);
        assert_eq!(s.state(), State::Ready);
    }

    #[test]
    fn heartbeats_go_out_every_50_ms_and_carry_the_estop() {
        let (mut s, _) = ready();
        s.tick(40_000);
        assert!(s.take_output().is_empty());
        s.tick(51_000);
        assert_eq!(sent(&s.take_output()), [Message::Heartbeat(Heartbeat { estop: false })]);
        s.set_estop(true, estop::Source::Shell, "Stop button");
        let out = sent(&s.take_output());
        assert_eq!(
            out,
            [Message::Estop(Estop {
                engaged: true,
                source: estop::Source::Shell,
                reason: "Stop button".into()
            })]
        );
        s.tick(101_000);
        assert_eq!(sent(&s.take_output()), [Message::Heartbeat(Heartbeat { estop: true })]);
    }

    #[test]
    fn an_estop_reason_is_cut_to_fit_the_board() {
        let mut s = Session::new(SessionConfig::default(), 0);
        let mut b = Board { seq: 0 };
        let ack = Message::HelloAck(HelloAck {
            version: 1,
            min_version: 1,
            max_version: 1,
            max_payload: 32,
            heartbeat_timeout_ms: 250,
            part_count: 0,
            board: "uno".into(),
            firmware: "f".into(),
            firmware_version: "1".into(),
            build_id: None,
        });
        s.receive(&b.wire(ack), 0);
        s.take_output();
        s.set_estop(true, estop::Source::App, &"ø".repeat(40));
        let out = sent(&s.take_output());
        let [Message::Estop(e)] = &out[..] else { panic!("{out:?}") };
        assert_eq!(e.reason, "ø".repeat(14), "29 bytes fit; a 2-byte character is never split");
        assert_eq!(fit("abc", 10), "abc");
    }

    #[test]
    fn readings_are_routed_with_the_core_clock() {
        let (mut s, mut b) = ready();
        s.receive(&b.distance(1, 0.5), 77_000);
        assert_eq!(
            s.take_events(),
            [Event::Reading {
                part: 1,
                reading: Reading::Distance(distance::Reading {
                    t_us: 77_000,
                    meters: 0.5,
                    status: distance::Status::Ok
                })
            }]
        );
        // A distance reading for the motor, and one for a part that doesn't exist: dropped.
        s.receive(&b.distance(0, 0.5), 78_000);
        s.receive(&b.distance(9, 0.5), 79_000);
        assert!(s.take_events().is_empty());
        assert_eq!(s.stats().rx_undecodable, 2);
    }

    #[test]
    fn commands_need_a_lease_the_right_part_and_a_ready_board() {
        let duty = Command::Motor(motor::Command::SetDuty { duty: 0.5 });
        let mut fresh = Session::new(SessionConfig::default(), 0);
        assert_eq!(fresh.command(0, duty, true), Err(CommandError::NotReady));
        let (mut s, _) = ready();
        assert_eq!(s.command(0, duty, false), Err(CommandError::NoLease));
        assert!(s.take_output().is_empty(), "nothing is sent without a lease");
        assert_eq!(s.command(7, duty, true), Err(CommandError::UnknownPart(7)));
        assert!(matches!(s.command(1, duty, true), Err(CommandError::WrongCapability { part: 1, .. })));
        s.command(0, duty, true).unwrap();
        assert_eq!(sent(&s.take_output()), [Message::Command(PartCommand { part: 0, command: duty })]);
        s.safe(None).unwrap();
        s.safe(Some(0)).unwrap();
        assert_eq!(
            sent(&s.take_output()),
            [Message::Safe(Safe { part: None }), Message::Safe(Safe { part: Some(0) })]
        );
    }

    #[test]
    fn a_version_mismatch_fails_with_a_clear_error() {
        let mut s = Session::new(SessionConfig::default(), 0);
        s.take_output();
        let mut b = Board { seq: 0 };
        let ack = Message::HelloAck(HelloAck {
            version: 0,
            min_version: 2,
            max_version: 3,
            max_payload: 64,
            heartbeat_timeout_ms: 250,
            part_count: 1,
            board: "mega".into(),
            firmware: "sennep".into(),
            firmware_version: "2.0.0".into(),
            build_id: None,
        });
        s.receive(&b.wire(ack), 1_000);
        assert_eq!(s.state(), State::Failed);
        let events = s.take_events();
        let Event::Failed(e) = &events[0] else { panic!("{events:?}") };
        assert_eq!(
            e.to_string(),
            "The board \"mega\" (sennep 2.0.0) speaks FaberLink versions 2–3, but this Faber speaks version 1. \
             Update Faber to talk to this board."
        );
        s.tick(10_000_000);
        assert!(s.take_output().is_empty(), "a failed session stays quiet");
        s.restart(10_000_000);
        assert_eq!(s.state(), State::Connecting);
        assert!(!s.take_output().is_empty());
    }

    #[test]
    fn the_build_id_reaches_the_board_info() {
        let mut s = Session::new(SessionConfig::default(), 0);
        s.take_output();
        let mut b = Board { seq: 0 };
        let ack = Message::HelloAck(HelloAck {
            version: 1,
            min_version: 1,
            max_version: 1,
            max_payload: 64,
            heartbeat_timeout_ms: 250,
            part_count: 0,
            board: "uno".into(),
            firmware: "faberlink-arduino".into(),
            firmware_version: "0.1.0".into(),
            build_id: Some(0xFEED_0000_0000_0042),
        });
        s.receive(&b.wire(ack), 1_000);
        let events = s.take_events();
        assert!(
            matches!(&events[0], Event::Connected(i) if i.build_id == Some(0xFEED_0000_0000_0042)),
            "{events:?}"
        );
        assert_eq!(s.board().and_then(|i| i.build_id), Some(0xFEED_0000_0000_0042));
    }

    #[test]
    fn garbage_is_counted_and_the_next_frame_still_arrives() {
        let (mut s, mut b) = ready();
        let mut bytes = vec![0x13, 0x37, 0x42, 0x00, 0xFF, 0x00];
        bytes.extend(b.distance(1, 0.25));
        s.receive(&bytes, 10_000);
        assert_eq!(s.stats().rx_bad_frames, 2);
        assert_eq!(s.take_events().len(), 1);
    }

    #[test]
    fn missed_frames_are_counted_from_seq_gaps() {
        let (mut s, mut b) = ready();
        b.distance(1, 0.1); // sent, but lost on the wire
        b.distance(1, 0.1);
        s.receive(&b.distance(1, 0.1), 10_000);
        assert_eq!(s.stats().rx_missed, 2);
    }

    #[test]
    fn a_silent_board_is_lost_and_the_session_starts_over() {
        let (mut s, _) = ready();
        s.tick(400_000);
        s.take_output();
        s.tick(502_000);
        assert_eq!(s.take_events(), [Event::BoardLost]);
        assert_eq!(s.state(), State::Connecting);
        assert!(s.parts().is_empty());
        let out = sent(&s.take_output());
        assert!(matches!(out[..], [Message::Hello(_)]), "{out:?}");
    }

    proptest::proptest! {
        /// Random bytes never panic the session, and a reading after the next 0x00 still arrives.
        #[test]
        fn garbage_never_panics_the_session(junk in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..600)) {
            let (mut s, mut b) = ready();
            s.receive(&junk, 10_000);
            s.take_events();
            let mut bytes = vec![0];
            bytes.extend(b.distance(1, 0.3));
            s.receive(&bytes, 20_000);
            let events = s.take_events();
            proptest::prop_assert!(
                matches!(events.last(), Some(Event::Reading { part: 1, .. })),
                "{:?}", events
            );
        }
    }
}
