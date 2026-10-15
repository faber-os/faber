//! FaberLink: the serial protocol between the Faber core and a microcontroller (a bridge).
//! The design and the exact wire format are in docs/faberlink.md.
//!
//! - [`crc`], [`cobs`] and [`frame`] need neither `std` nor an allocator.
//! - [`message`] holds every message, [`layout`] the fixed binary layout of each capability.
//! - [`session`] is the core side of a connection: a pure state machine with the clock passed in.

mod bytes;
pub mod cobs;
pub mod crc;
pub mod frame;
pub mod layout;
pub mod message;
pub mod session;

pub use message::{MAX_VERSION, MIN_VERSION, Message};
pub use session::{Event, Session, SessionConfig, State};

use std::fmt;

/// Why a payload can't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload ends before its layout does.
    Truncated,
    /// Bytes are left after the layout.
    TrailingBytes,
    /// A field holds a value this protocol version doesn't define.
    BadValue { field: &'static str },
    /// An unknown message type.
    UnknownKind(u8),
    /// An unknown capability code.
    UnknownCapability(u8),
    /// The capability has no FaberLink layout yet.
    NoLayout { capability: &'static str },
    /// A protocol version this side doesn't speak.
    UnsupportedVersion(u8),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Truncated => write!(f, "the message is shorter than its layout"),
            DecodeError::TrailingBytes => write!(f, "the message is longer than its layout"),
            DecodeError::BadValue { field } => write!(f, "`{field}` has a value this version doesn't define"),
            DecodeError::UnknownKind(k) => write!(f, "unknown message type 0x{k:02X}"),
            DecodeError::UnknownCapability(c) => write!(f, "unknown capability code {c}"),
            DecodeError::NoLayout { capability } => {
                write!(f, "{capability} can't be sent over FaberLink yet")
            }
            DecodeError::UnsupportedVersion(v) => write!(f, "FaberLink version {v} isn't supported"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why a message can't be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// A string or list is longer than its length byte allows (255).
    TooLong { field: &'static str },
    /// The capability has no FaberLink layout yet.
    NoLayout { capability: &'static str },
    /// The payload is longer than a frame allows (250 bytes).
    PayloadTooLarge(usize),
    /// A protocol version this side doesn't speak.
    UnsupportedVersion(u8),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::TooLong { field } => write!(f, "`{field}` is longer than 255 bytes"),
            EncodeError::NoLayout { capability } => {
                write!(f, "{capability} can't be sent over FaberLink yet")
            }
            EncodeError::PayloadTooLarge(n) => {
                write!(
                    f,
                    "the message is {n} bytes; a FaberLink frame carries at most {}",
                    frame::MAX_PAYLOAD
                )
            }
            EncodeError::UnsupportedVersion(v) => write!(f, "FaberLink version {v} isn't supported"),
        }
    }
}

impl std::error::Error for EncodeError {}
