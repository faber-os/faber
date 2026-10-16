//! Every FaberLink message (docs/faberlink.md §3), with its payload layout.
//!
//! `HELLO` and `HELLO_ACK` are the same in every protocol version, so two sides can always find
//! out which version to speak. Every other message is encoded for one negotiated version.

use crate::bytes::{Reader, Writer};
use crate::{DecodeError, EncodeError, frame, layout};
use faber_capabilities::{Capability, Command, Info, Reading, estop};

/// The protocol versions this crate speaks.
pub const MIN_VERSION: u8 = 1;
pub const MAX_VERSION: u8 = 1;

/// The type byte of each message.
pub mod kind {
    pub const HELLO: u8 = 0x01;
    pub const HELLO_ACK: u8 = 0x02;
    pub const DESCRIBE: u8 = 0x03;
    pub const PART: u8 = 0x04;
    pub const PINS: u8 = 0x05;
    pub const HEARTBEAT: u8 = 0x10;
    pub const STATUS: u8 = 0x11;
    pub const READING: u8 = 0x20;
    pub const COMMAND: u8 = 0x21;
    pub const APPLIED: u8 = 0x22;
    pub const SAFE: u8 = 0x23;
    pub const ESTOP: u8 = 0x30;
    pub const LOG: u8 = 0x40;
}

/// core → board: "I speak versions `min..=max` and accept payloads up to `max_payload`."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub min_version: u8,
    pub max_version: u8,
    pub max_payload: u8,
}

/// board → core, the answer to `HELLO`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloAck {
    /// The version both sides will speak, or 0 if the two ranges don't overlap.
    pub version: u8,
    /// The versions the board speaks.
    pub min_version: u8,
    pub max_version: u8,
    /// The largest payload the board accepts (its buffers are sized for it).
    pub max_payload: u8,
    /// No `HEARTBEAT` for this long and the board puts every actuator in its safe state.
    pub heartbeat_timeout_ms: u16,
    /// How many `PART`s `DESCRIBE` will give.
    pub part_count: u8,
    pub board: String,
    pub firmware: String,
    pub firmware_version: String,
    /// Which build of the sketch the board runs: Faber's Arduino editor compiles every sketch
    /// with one (`FABER_BUILD_ID`, docs/arduino-editor.md). `None` for a sketch built elsewhere.
    /// On the wire it is optional, after the strings, so a board without one sends exactly the
    /// bytes it always did.
    pub build_id: Option<u64>,
}

/// What a part's info decodes to: known capabilities are typed; a capability this core doesn't
/// know keeps its raw bytes, so the core can say which one it was.
#[derive(Clone, Debug, PartialEq)]
pub enum PartInfo {
    Known(Info),
    /// A capability this core knows that has no info (`digital-in@1`): its info layout is empty.
    Empty,
    Unknown(Vec<u8>),
}

/// board → core: one part the board hosts (one per part, in answer to `DESCRIBE`).
#[derive(Clone, Debug, PartialEq)]
pub struct PartDescription {
    /// The board's local id for the part, used in every later message about it.
    pub id: u8,
    /// How many parts the board has in all.
    pub count: u8,
    /// For actuators: no `COMMAND` for this long and the board stops it by itself. 0 for sensors.
    pub lease_ms: u16,
    /// The versioned capability id, e.g. `motor@1`.
    pub capability: String,
    pub name: String,
    pub info: PartInfo,
}

impl PartDescription {
    pub fn known_capability(&self) -> Option<Capability> {
        Capability::from_id(&self.capability)
    }
}

/// What a pin does for its part, in `PINS`. One table for every part class, so the core can name
/// a pin without knowing the class; a code this side doesn't know is kept as `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinRole {
    /// A motor driver's speed input (PWM), on a driver with a direction pin.
    Pwm,
    /// A motor driver's direction input.
    Dir,
    /// A motor driver's enable input (PWM), on a driver with IN1 and IN2 (L298N ENA, TB6612 PWMA).
    En,
    /// A motor driver's first input: a level beside EN, or one of the two PWM inputs.
    In1,
    /// A motor driver's second input.
    In2,
    /// A motor driver's standby input (TB6612 STBY), held high while the board runs.
    Stby,
    /// A range finder's trigger output (HC-SR04 Trig).
    Trig,
    /// A range finder's echo input (HC-SR04 Echo).
    Echo,
    /// A sensor's output, read by the board (a Sharp IR's Vo).
    Signal,
    /// A battery's voltage, through a divider.
    Sense,
    /// An output the board switches: an LED, a relay module, a MOSFET.
    Out,
    /// A quadrature encoder's channel A.
    A,
    /// A quadrature encoder's channel B.
    B,
    /// The I²C bus's data line, for a part on the bus.
    Sda,
    /// The I²C bus's clock line.
    Scl,
    /// A time-of-flight sensor's shutdown input (VL53L0X XSHUT).
    Xshut,
    /// A role a later library added.
    Other(u8),
}

const PIN_ROLES: &[PinRole] = &[
    PinRole::Pwm,
    PinRole::Dir,
    PinRole::En,
    PinRole::In1,
    PinRole::In2,
    PinRole::Stby,
    PinRole::Trig,
    PinRole::Echo,
    PinRole::Signal,
    PinRole::Sense,
    PinRole::Out,
    PinRole::A,
    PinRole::B,
    PinRole::Sda,
    PinRole::Scl,
    PinRole::Xshut,
];

impl PinRole {
    /// Its code on the wire: 1, 2, 3, … in the order of [`PinRole`].
    pub fn code(self) -> u8 {
        match self {
            PinRole::Other(c) => c,
            known => index(PIN_ROLES, &known) + 1,
        }
    }

    pub fn from_code(c: u8) -> PinRole {
        c.checked_sub(1).and_then(|i| PIN_ROLES.get(i as usize).copied()).unwrap_or(PinRole::Other(c))
    }
}

/// One pin a part uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinUse {
    pub role: PinRole,
    /// The Arduino pin number, as the sketch wrote it (`A0` is 14 on an Uno, 54 on a Mega).
    pub pin: u8,
}

/// board → core: the pins one part uses, sent just before its `PART` (docs/faberlink.md §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartPins {
    pub part: u8,
    /// The pin number of `A0` on this board, so pins from it on can be named `A0`, `A1`, …;
    /// `None` when the library doesn't know (0xFF on the wire): pins are then plain numbers.
    pub analog_base: Option<u8>,
    pub pins: Vec<PinUse>,
}

impl PartPins {
    /// A pin as it is printed on the board: `D7`, `A0`, or just `7` when the board didn't say
    /// where its analog pins start.
    pub fn label(&self, pin: u8) -> String {
        match self.analog_base {
            Some(a) if pin >= a => format!("A{}", pin - a),
            Some(_) => format!("D{pin}"),
            None => pin.to_string(),
        }
    }
}

/// core → board, every 50 ms. It carries the e-stop as a level, so a lost `ESTOP` message is
/// corrected within one heartbeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heartbeat {
    pub estop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Ok,
    Degraded,
    Fault,
}

/// Why the board last had to step in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// The heartbeat stopped: every actuator went to its safe state.
    HeartbeatLost,
    /// An actuator got no command within its lease time and stopped.
    LeaseExpired,
    /// Frames were dropped (bad CRC, bad COBS or too long).
    BadFrames,
    /// A command was refused.
    RefusedCommand,
    /// The e-stop was engaged.
    Estop,
}

/// board → core, every 100 ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub health: Health,
    /// A hardware e-stop on the board is latched: only a reset on the board clears it.
    pub hardware_estop: bool,
    /// The board is holding its actuators stopped because of an e-stop (from either side).
    pub estop: bool,
    /// The heartbeat timed out and the board is in its safe state.
    pub failsafe: bool,
    pub uptime_ms: u32,
    pub last_fault: Fault,
    /// Frames the board dropped (bad CRC, bad COBS, too long), since it started.
    pub rx_bad: u16,
    /// Frames the board missed, counted from gaps in `seq`.
    pub rx_gaps: u16,
}

/// board → core: one reading. On the wire its `t_us` is the board's clock in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct PartReading {
    pub part: u8,
    pub reading: Reading,
}

/// core → board: one command (only while the core holds a lease on the part).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartCommand {
    pub part: u8,
    pub command: Command,
}

/// Why the board's output for a part changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// A `COMMAND` was applied.
    Command,
    /// A `COMMAND` was refused (unsupported, not a number, or wrong capability).
    Refused,
    /// The heartbeat stopped.
    HeartbeatLost,
    /// The part's own lease time passed without a command.
    LeaseExpired,
    /// The core sent `SAFE`.
    Safe,
    /// The e-stop is engaged.
    Estop,
    /// A command came while the board had no live heartbeat.
    NoHeartbeat,
}

/// board → core: what the board actually does now (after its own limits).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Applied {
    pub part: u8,
    pub cause: Cause,
    /// The board changed the command to fit its limits.
    pub clamped: bool,
    pub command: Command,
}

/// core → board: put one part, or every part, in its safe state now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Safe {
    /// `None` means every part (`0xFF` on the wire).
    pub part: Option<u8>,
}

/// Either way: engage or release the e-stop. A board sends it when its own stop button is pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Estop {
    pub engaged: bool,
    pub source: estop::Source,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// board → core: a line of text for the bridge part's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    pub level: LogLevel,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Hello(Hello),
    HelloAck(HelloAck),
    Describe,
    Part(PartDescription),
    Pins(PartPins),
    Heartbeat(Heartbeat),
    Status(Status),
    Reading(PartReading),
    Command(PartCommand),
    Applied(Applied),
    Safe(Safe),
    Estop(Estop),
    Log(Log),
}

const HEALTH: &[Health] = &[Health::Ok, Health::Degraded, Health::Fault];
const FAULT: &[Fault] = &[
    Fault::None,
    Fault::HeartbeatLost,
    Fault::LeaseExpired,
    Fault::BadFrames,
    Fault::RefusedCommand,
    Fault::Estop,
];
const CAUSE: &[Cause] = &[
    Cause::Command,
    Cause::Refused,
    Cause::HeartbeatLost,
    Cause::LeaseExpired,
    Cause::Safe,
    Cause::Estop,
    Cause::NoHeartbeat,
];
const SOURCE: &[estop::Source] = &[estop::Source::Part, estop::Source::App, estop::Source::Shell];
const LEVEL: &[LogLevel] = &[LogLevel::Info, LogLevel::Warn, LogLevel::Error];

fn index<T: PartialEq>(all: &[T], v: &T) -> u8 {
    all.iter().position(|x| x == v).expect("every variant is listed") as u8
}

fn at<T: Copy>(all: &[T], i: u8, field: &'static str) -> Result<T, DecodeError> {
    all.get(i as usize).copied().ok_or(DecodeError::BadValue { field })
}

fn cap_code(c: Capability) -> Result<u8, EncodeError> {
    layout::code(c).ok_or(EncodeError::NoLayout { capability: c.id() })
}

fn cap_from(r: &mut Reader) -> Result<Capability, DecodeError> {
    let c = r.u8()?;
    layout::from_code(c).ok_or(DecodeError::UnknownCapability(c))
}

impl Message {
    /// The type byte.
    pub fn kind(&self) -> u8 {
        match self {
            Message::Hello(_) => kind::HELLO,
            Message::HelloAck(_) => kind::HELLO_ACK,
            Message::Describe => kind::DESCRIBE,
            Message::Part(_) => kind::PART,
            Message::Pins(_) => kind::PINS,
            Message::Heartbeat(_) => kind::HEARTBEAT,
            Message::Status(_) => kind::STATUS,
            Message::Reading(_) => kind::READING,
            Message::Command(_) => kind::COMMAND,
            Message::Applied(_) => kind::APPLIED,
            Message::Safe(_) => kind::SAFE,
            Message::Estop(_) => kind::ESTOP,
            Message::Log(_) => kind::LOG,
        }
    }

    /// The payload for protocol `version`.
    pub fn encode(&self, version: u8) -> Result<Vec<u8>, EncodeError> {
        let mut out = Vec::new();
        let w = &mut Writer { out: &mut out };
        match self {
            Message::Hello(h) => {
                w.u8(h.min_version);
                w.u8(h.max_version);
                w.u8(h.max_payload);
            }
            Message::HelloAck(a) => {
                w.u8(a.version);
                w.u8(a.min_version);
                w.u8(a.max_version);
                w.u8(a.max_payload);
                w.u16(a.heartbeat_timeout_ms);
                w.u8(a.part_count);
                w.str8(&a.board, "board")?;
                w.str8(&a.firmware, "firmware")?;
                w.str8(&a.firmware_version, "firmwareVersion")?;
                if let Some(id) = a.build_id {
                    w.u64(id);
                }
            }
            _ if !(MIN_VERSION..=MAX_VERSION).contains(&version) => {
                return Err(EncodeError::UnsupportedVersion(version));
            }
            Message::Describe => {}
            Message::Part(p) => {
                w.u8(p.id);
                w.u8(p.count);
                w.u16(p.lease_ms);
                w.str8(&p.capability, "capability")?;
                w.str8(&p.name, "name")?;
                match &p.info {
                    PartInfo::Known(i) => layout::encode_info(i, w.out)?,
                    PartInfo::Empty => {}
                    PartInfo::Unknown(bytes) => w.out.extend_from_slice(bytes),
                }
            }
            Message::Pins(p) => {
                w.u8(p.part);
                w.u8(p.analog_base.unwrap_or(0xFF));
                for u in &p.pins {
                    w.u8(u.role.code());
                    w.u8(u.pin);
                }
            }
            Message::Heartbeat(h) => w.u8(h.estop as u8),
            Message::Status(s) => {
                w.u8(index(HEALTH, &s.health));
                w.u8(s.hardware_estop as u8 | (s.estop as u8) << 1 | (s.failsafe as u8) << 2);
                w.u32(s.uptime_ms);
                w.u8(index(FAULT, &s.last_fault));
                w.u16(s.rx_bad);
                w.u16(s.rx_gaps);
            }
            Message::Reading(r) => {
                w.u8(r.part);
                w.u8(cap_code(r.reading.capability())?);
                // The board's clock in ms; it wraps after 49.7 days.
                w.u32((r.reading.t_us() / 1000) as u32);
                layout::encode_reading(&r.reading, w.out)?;
            }
            Message::Command(c) => {
                w.u8(c.part);
                w.u8(cap_code(c.command.capability())?);
                layout::encode_command(&c.command, w.out)?;
            }
            Message::Applied(a) => {
                w.u8(a.part);
                w.u8(cap_code(a.command.capability())?);
                w.u8(index(CAUSE, &a.cause));
                w.u8(a.clamped as u8);
                layout::encode_command(&a.command, w.out)?;
            }
            Message::Safe(s) => w.u8(s.part.unwrap_or(0xFF)),
            Message::Estop(e) => {
                w.bool(e.engaged);
                w.u8(index(SOURCE, &e.source));
                w.str8(&e.reason, "reason")?;
            }
            Message::Log(l) => {
                w.u8(index(LEVEL, &l.level));
                w.out.extend_from_slice(l.text.as_bytes());
            }
        }
        if out.len() > frame::MAX_PAYLOAD {
            return Err(EncodeError::PayloadTooLarge(out.len()));
        }
        Ok(out)
    }

    /// One frame for the wire, delimiter included.
    pub fn to_wire(&self, version: u8, seq: u8) -> Result<Vec<u8>, EncodeError> {
        let payload = self.encode(version)?;
        let mut out = vec![0; frame::MAX_WIRE];
        let n = frame::encode(self.kind(), seq, &payload, &mut out).expect("the payload size was checked");
        out.truncate(n);
        Ok(out)
    }

    /// Decodes a payload of type `kind` in protocol `version`.
    pub fn decode(version: u8, kind: u8, payload: &[u8]) -> Result<Message, DecodeError> {
        let r = &mut Reader::new(payload);
        let m = match kind {
            kind::HELLO => {
                Message::Hello(Hello { min_version: r.u8()?, max_version: r.u8()?, max_payload: r.u8()? })
            }
            kind::HELLO_ACK => Message::HelloAck(HelloAck {
                version: r.u8()?,
                min_version: r.u8()?,
                max_version: r.u8()?,
                max_payload: r.u8()?,
                heartbeat_timeout_ms: r.u16()?,
                part_count: r.u8()?,
                board: r.str8("board")?,
                firmware: r.str8("firmware")?,
                firmware_version: r.str8("firmwareVersion")?,
                // Optional fields follow the strings. Bytes after the ones this side knows are a
                // later Faber's optional fields: HELLO_ACK must stay readable across versions.
                build_id: if r.remaining() == 0 { None } else { Some(r.u64()?) },
            }),
            _ if !(MIN_VERSION..=MAX_VERSION).contains(&version) => {
                return Err(DecodeError::UnsupportedVersion(version));
            }
            kind::DESCRIBE => Message::Describe,
            kind::PART => {
                let id = r.u8()?;
                let count = r.u8()?;
                let lease_ms = r.u16()?;
                let capability = r.str8("capability")?;
                let name = r.str8("name")?;
                let rest = r.rest();
                let info = match Capability::from_id(&capability) {
                    Some(c) if layout::code(c).is_some() => match layout::decode_info(c, rest)? {
                        Some(i) => PartInfo::Known(i),
                        None => PartInfo::Empty,
                    },
                    _ => PartInfo::Unknown(rest.to_vec()),
                };
                Message::Part(PartDescription { id, count, lease_ms, capability, name, info })
            }
            kind::PINS => {
                let part = r.u8()?;
                let base = r.u8()?;
                let rest = r.rest();
                if !rest.len().is_multiple_of(2) {
                    return Err(DecodeError::Truncated);
                }
                let pins =
                    rest.chunks(2).map(|c| PinUse { role: PinRole::from_code(c[0]), pin: c[1] }).collect();
                Message::Pins(PartPins { part, analog_base: (base != 0xFF).then_some(base), pins })
            }
            kind::HEARTBEAT => {
                let f = r.u8()?;
                if f & !1 != 0 {
                    return Err(DecodeError::BadValue { field: "flags" });
                }
                Message::Heartbeat(Heartbeat { estop: f & 1 != 0 })
            }
            kind::STATUS => {
                let health = at(HEALTH, r.u8()?, "health")?;
                let f = r.u8()?;
                if f & !0b111 != 0 {
                    return Err(DecodeError::BadValue { field: "flags" });
                }
                Message::Status(Status {
                    health,
                    hardware_estop: f & 1 != 0,
                    estop: f & 2 != 0,
                    failsafe: f & 4 != 0,
                    uptime_ms: r.u32()?,
                    last_fault: at(FAULT, r.u8()?, "lastFault")?,
                    rx_bad: r.u16()?,
                    rx_gaps: r.u16()?,
                })
            }
            kind::READING => {
                let part = r.u8()?;
                let cap = cap_from(r)?;
                let t_us = r.u32()? as u64 * 1000;
                let reading = layout::decode_reading(cap, t_us, r.rest())?;
                Message::Reading(PartReading { part, reading })
            }
            kind::COMMAND => {
                let part = r.u8()?;
                let cap = cap_from(r)?;
                Message::Command(PartCommand { part, command: layout::decode_command(cap, r.rest())? })
            }
            kind::APPLIED => {
                let part = r.u8()?;
                let cap = cap_from(r)?;
                let cause = at(CAUSE, r.u8()?, "cause")?;
                let clamped = r.bool("clamped")?;
                let command = layout::decode_command(cap, r.rest())?;
                Message::Applied(Applied { part, cause, clamped, command })
            }
            kind::SAFE => {
                let p = r.u8()?;
                Message::Safe(Safe { part: (p != 0xFF).then_some(p) })
            }
            kind::ESTOP => Message::Estop(Estop {
                engaged: r.bool("engaged")?,
                source: at(SOURCE, r.u8()?, "source")?,
                reason: r.str8("reason")?,
            }),
            kind::LOG => {
                let level = at(LEVEL, r.u8()?, "level")?;
                // A board may cut a line in the middle of a character; show what arrived.
                Message::Log(Log { level, text: String::from_utf8_lossy(r.rest()).into_owned() })
            }
            other => return Err(DecodeError::UnknownKind(other)),
        };
        if kind != kind::HELLO_ACK {
            r.end()?;
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tests::{command, info, reading};
    use proptest::prelude::*;

    fn text(max: usize) -> impl Strategy<Value = String> {
        proptest::string::string_regex(&format!("[ -~]{{0,{max}}}")).unwrap()
    }

    fn message() -> impl Strategy<Value = Message> {
        prop_oneof![
            any::<(u8, u8, u8)>().prop_map(|(a, b, c)| Message::Hello(Hello {
                min_version: a,
                max_version: b,
                max_payload: c
            })),
            (any::<(u8, u8, u8, u8, u16, u8)>(), text(30), text(30), text(30), any::<Option<u64>>())
                .prop_map(|(n, board, firmware, v, build_id)| {
                    Message::HelloAck(HelloAck {
                        version: n.0,
                        min_version: n.1,
                        max_version: n.2,
                        max_payload: n.3,
                        heartbeat_timeout_ms: n.4,
                        part_count: n.5,
                        board,
                        firmware,
                        firmware_version: v,
                        build_id,
                    })
                }),
            Just(Message::Describe),
            (any::<(u8, u8, u16)>(), text(20), info()).prop_map(|((id, count, lease_ms), name, info)| {
                Message::Part(PartDescription {
                    id,
                    count,
                    lease_ms,
                    capability: info.capability().id().into(),
                    name,
                    info: PartInfo::Known(info),
                })
            }),
            (any::<(u8, u16)>(), text(10), proptest::collection::vec(any::<u8>(), 0..20)).prop_map(
                |((id, lease_ms), name, raw)| Message::Part(PartDescription {
                    id,
                    count: 9,
                    lease_ms,
                    capability: "thermal-camera@1".into(),
                    name,
                    info: PartInfo::Unknown(raw),
                })
            ),
            (any::<u8>(), proptest::option::of(0..0xFFu8), proptest::collection::vec(pin_use(), 0..20))
                .prop_map(|(part, analog_base, pins)| Message::Pins(PartPins { part, analog_base, pins })),
            any::<bool>().prop_map(|estop| Message::Heartbeat(Heartbeat { estop })),
            (0..3u8, any::<(bool, bool, bool, u32)>(), 0..6u8, any::<(u16, u16)>()).prop_map(
                |(h, f, fault, (rx_bad, rx_gaps))| Message::Status(Status {
                    health: HEALTH[h as usize],
                    hardware_estop: f.0,
                    estop: f.1,
                    failsafe: f.2,
                    uptime_ms: f.3,
                    last_fault: FAULT[fault as usize],
                    rx_bad,
                    rx_gaps,
                })
            ),
            (any::<u8>(), reading())
                .prop_map(|(part, reading)| Message::Reading(PartReading { part, reading })),
            (any::<u8>(), command())
                .prop_map(|(part, command)| Message::Command(PartCommand { part, command })),
            (any::<u8>(), 0..7u8, any::<bool>(), command()).prop_map(|(part, c, clamped, command)| {
                Message::Applied(Applied { part, cause: CAUSE[c as usize], clamped, command })
            }),
            proptest::option::of(0..0xFEu8).prop_map(|part| Message::Safe(Safe { part })),
            (any::<bool>(), 0..3u8, text(60)).prop_map(|(engaged, s, reason)| Message::Estop(Estop {
                engaged,
                source: SOURCE[s as usize],
                reason
            })),
            (0..3u8, text(200)).prop_map(|(l, text)| Message::Log(Log { level: LEVEL[l as usize], text })),
        ]
    }

    fn pin_use() -> impl Strategy<Value = PinUse> {
        let role = prop_oneof![
            (0..PIN_ROLES.len()).prop_map(|i| PIN_ROLES[i]),
            (PIN_ROLES.len() as u8 + 1..=255).prop_map(PinRole::Other),
            Just(PinRole::Other(0)),
        ];
        (role, any::<u8>()).prop_map(|(role, pin)| PinUse { role, pin })
    }

    #[test]
    fn pins_on_the_wire() {
        let m = Message::Pins(PartPins {
            part: 2,
            analog_base: Some(14),
            pins: vec![PinUse { role: PinRole::Trig, pin: 7 }, PinUse { role: PinRole::Echo, pin: 6 }],
        });
        let p = m.encode(1).unwrap();
        assert_eq!(p, [2, 14, 7, 7, 8, 6]);
        assert_eq!(Message::decode(1, kind::PINS, &p).unwrap(), m);
        assert_eq!(Message::decode(1, kind::PINS, &[2, 0xFF]).unwrap(), {
            Message::Pins(PartPins { part: 2, analog_base: None, pins: vec![] })
        });
    }

    #[test]
    fn every_pin_role_has_its_own_code_and_unknown_codes_are_kept() {
        let codes: Vec<u8> = PIN_ROLES.iter().map(|r| r.code()).collect();
        assert_eq!(codes, (1..=16).collect::<Vec<u8>>(), "the table in docs/faberlink.md §3");
        for r in PIN_ROLES {
            assert_eq!(PinRole::from_code(r.code()), *r);
        }
        // A later library's role still shows its pin; it isn't a reason to drop the message.
        assert_eq!(PinRole::from_code(0), PinRole::Other(0));
        assert_eq!(PinRole::from_code(42), PinRole::Other(42));
        let p = Message::decode(1, kind::PINS, &[0, 0xFF, 42, 3]).unwrap();
        assert_eq!(
            p,
            Message::Pins(PartPins {
                part: 0,
                analog_base: None,
                pins: vec![PinUse { role: PinRole::Other(42), pin: 3 }]
            })
        );
    }

    #[test]
    fn a_pin_list_cut_in_the_middle_of_a_pair_is_refused() {
        assert_eq!(Message::decode(1, kind::PINS, &[0, 14, 1]), Err(DecodeError::Truncated));
        assert_eq!(Message::decode(1, kind::PINS, &[0]), Err(DecodeError::Truncated));
    }

    #[test]
    fn pins_are_named_as_printed_on_the_board() {
        let uno = PartPins { part: 0, analog_base: Some(14), pins: vec![] };
        assert_eq!((uno.label(7), uno.label(13), uno.label(14), uno.label(19)), {
            ("D7".into(), "D13".into(), "A0".into(), "A5".into())
        });
        let mega = PartPins { analog_base: Some(54), ..uno.clone() };
        assert_eq!(
            (mega.label(53), mega.label(54), mega.label(69)),
            ("D53".into(), "A0".into(), "A15".into())
        );
        let unknown = PartPins { analog_base: None, ..uno };
        assert_eq!(unknown.label(36), "36");
    }

    #[test]
    fn hello_is_the_same_in_every_version() {
        let h = Message::Hello(Hello { min_version: 1, max_version: 3, max_payload: 250 });
        assert_eq!(h.encode(7).unwrap(), [1, 3, 250]);
        assert_eq!(Message::decode(0, kind::HELLO, &[1, 3, 250]).unwrap(), h);
    }

    #[test]
    fn other_messages_need_a_version_this_crate_speaks() {
        assert_eq!(Message::Describe.encode(2), Err(EncodeError::UnsupportedVersion(2)));
        assert_eq!(Message::decode(0, kind::DESCRIBE, &[]), Err(DecodeError::UnsupportedVersion(0)));
        assert_eq!(Message::decode(1, 0x7E, &[]), Err(DecodeError::UnknownKind(0x7E)));
    }

    #[test]
    fn a_status_on_the_wire() {
        let s = Message::Status(Status {
            health: Health::Degraded,
            hardware_estop: true,
            estop: true,
            failsafe: false,
            uptime_ms: 0x0102_0304,
            last_fault: Fault::Estop,
            rx_bad: 2,
            rx_gaps: 0x0100,
        });
        assert_eq!(s.encode(1).unwrap(), [1, 0b011, 4, 3, 2, 1, 5, 2, 0, 0, 1]);
    }

    #[test]
    fn a_reading_on_the_wire_carries_board_milliseconds() {
        let r = Message::Reading(PartReading {
            part: 3,
            reading: Reading::Distance(faber_capabilities::distance::Reading {
                t_us: 1_234_000,
                meters: 1.5,
                status: faber_capabilities::distance::Status::Ok,
            }),
        });
        let p = r.encode(1).unwrap();
        assert_eq!(p, [3, 1, 0xD2, 0x04, 0, 0, 0x00, 0x00, 0xC0, 0x3F, 0]);
        assert_eq!(Message::decode(1, kind::READING, &p).unwrap(), r);
    }

    #[test]
    fn an_oversized_payload_is_refused() {
        let l = Message::Log(Log { level: LogLevel::Info, text: "x".repeat(300) });
        assert_eq!(l.encode(1), Err(EncodeError::PayloadTooLarge(301)));
        let a = Message::HelloAck(HelloAck {
            version: 1,
            min_version: 1,
            max_version: 1,
            max_payload: 250,
            heartbeat_timeout_ms: 250,
            part_count: 0,
            board: "b".repeat(256),
            firmware: String::new(),
            firmware_version: String::new(),
            build_id: None,
        });
        assert_eq!(a.encode(1), Err(EncodeError::TooLong { field: "board" }));
    }

    fn uno_ack(build_id: Option<u64>) -> Message {
        Message::HelloAck(HelloAck {
            version: 1,
            min_version: 1,
            max_version: 1,
            max_payload: 64,
            heartbeat_timeout_ms: 250,
            part_count: 1,
            board: "b".into(),
            firmware: "f".into(),
            firmware_version: "1".into(),
            build_id,
        })
    }

    /// The bytes every board sent before build ids existed.
    const OLD_ACK: [u8; 13] = [1, 1, 1, 64, 250, 0, 1, 1, b'b', 1, b'f', 1, b'1'];

    #[test]
    fn a_hello_ack_without_a_build_id_is_the_same_bytes_as_before() {
        assert_eq!(uno_ack(None).encode(1).unwrap(), OLD_ACK);
        assert_eq!(Message::decode(1, kind::HELLO_ACK, &OLD_ACK).unwrap(), uno_ack(None));
    }

    #[test]
    fn a_build_id_follows_the_strings_little_endian() {
        let p = uno_ack(Some(0x0123_4567_89AB_CDEF)).encode(1).unwrap();
        assert_eq!(p[..13], OLD_ACK);
        assert_eq!(p[13..], [0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01]);
        // HELLO_ACK is read before a version is agreed, so it decodes in any version.
        assert_eq!(Message::decode(0, kind::HELLO_ACK, &p).unwrap(), uno_ack(Some(0x0123_4567_89AB_CDEF)));
    }

    #[test]
    fn a_cut_build_id_is_refused() {
        for n in 1..8 {
            let mut p = OLD_ACK.to_vec();
            p.extend(std::iter::repeat_n(0xAA, n));
            assert_eq!(Message::decode(1, kind::HELLO_ACK, &p), Err(DecodeError::Truncated), "{n} bytes");
        }
    }

    #[test]
    fn optional_fields_a_later_faber_adds_are_skipped() {
        let mut p = uno_ack(Some(7)).encode(1).unwrap();
        p.extend_from_slice(&[9, 9, 9]);
        assert_eq!(Message::decode(1, kind::HELLO_ACK, &p).unwrap(), uno_ack(Some(7)));
        // Every other message stays exact.
        assert_eq!(Message::decode(1, kind::SAFE, &[1, 2]), Err(DecodeError::TrailingBytes));
    }

    proptest! {
        #[test]
        fn every_message_round_trips(m in message(), seq in any::<u8>()) {
            let payload = m.encode(1).unwrap();
            prop_assert_eq!(&Message::decode(1, m.kind(), &payload).unwrap(), &m);
            let wire = m.to_wire(1, seq).unwrap();
            let mut d = frame::Decoder::new();
            let mut got = None;
            for &b in &wire {
                if let Some(f) = d.push(b) {
                    let f = f.unwrap();
                    prop_assert_eq!(f.seq, seq);
                    got = Some(Message::decode(1, f.kind, f.payload).unwrap());
                }
            }
            prop_assert_eq!(got, Some(m));
        }

        #[test]
        fn garbage_payloads_never_panic(kind in any::<u8>(), payload in proptest::collection::vec(any::<u8>(), 0..80)) {
            let _ = Message::decode(1, kind, &payload);
        }
    }
}
