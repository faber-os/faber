//! FaberLink: the serial protocol between the Faber core and a microcontroller (a bridge).
//! The design and the exact wire format are in docs/faberlink.md.
//!
//! - [`crc`], [`cobs`] and [`frame`] need neither `std` nor an allocator.

pub mod cobs;
pub mod crc;
pub mod frame;
