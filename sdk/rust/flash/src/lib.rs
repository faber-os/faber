//! The Arduino flasher's protocol code (docs/connect-an-arduino.md, "Flash from Faber").
//!
//! - [`hex`] reads Intel HEX files, checking every record's checksum.
//! - [`board`] knows the boards: their chip signature, flash size, page size and bootloader,
//!   and guesses a board from its USB vendor and product id.
//! - [`stk500v1`] (Optiboot on the Uno and Nano) and [`stk500v2`] (the Mega 2560's bootloader)
//!   speak the two bootloader protocols.
//! - [`flash()`] puts an image on a board: reset, sync, check the signature, write, read back.
//! - [`read_back()`] reads the pages of an image back from a board and compares them, writing
//!   nothing: how Faber checks which program a board still runs.
//! - [`sim`] holds simulated bootloaders, written from the protocol specs (AVR061, AVR068), so
//!   everything above is tested without a board.
//!
//! No I/O of its own: the serial port, the reset line and the clock come in through [`Port`].

pub mod board;
mod error;
mod flasher;
pub mod hex;
mod port;
pub mod sim;
pub mod stk500v1;
pub mod stk500v2;

pub use board::{BOARDS, Board, Detected, Protocol, detect};
pub use error::FlashError;
pub use flasher::{Flashed, Progress, ReadBack, Stage, Timing, flash, read_back};
pub use hex::{HexError, Image};
pub use port::Port;

#[cfg(test)]
mod tests;
