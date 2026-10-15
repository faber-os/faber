//! Why flashing failed, in words that say what to do (docs/principles.md, "Errors say what to do").

use crate::board::{Board, chip_name};
use crate::hex::HexError;
use std::fmt;
use std::io;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlashError {
    /// The firmware file isn't valid Intel HEX.
    BadHex(HexError),
    /// The program doesn't fit next to the bootloader.
    TooLarge { board: &'static str, end: u32, room: u32 },
    /// The bootloader never answered: the board didn't reset, or isn't this kind of board.
    NoSync { board: &'static str },
    /// The bootloader answered, then stopped in the middle (`while` says what it was doing).
    LostSync { while_: &'static str },
    /// The chip isn't the one the board should have.
    WrongSignature { board: &'static str, expected: [u8; 3], found: [u8; 3] },
    /// What was read back differs from what was written.
    VerifyFailed { address: u32, wrote: u8, read: u8 },
    /// The board's answers kept arriving damaged (STK500v2 checksums).
    DamagedAnswers,
    /// The bootloader said no to a command.
    Refused { what: &'static str },
    /// The serial port itself failed (unplugged, …).
    Port(String),
}

impl FlashError {
    pub(crate) fn port(e: io::Error) -> FlashError {
        FlashError::Port(e.to_string())
    }

    pub(crate) fn wrong_signature(board: &Board, found: [u8; 3]) -> FlashError {
        FlashError::WrongSignature { board: board.name, expected: board.signature, found }
    }

    /// Stable, kebab-case, for the API.
    pub fn code(&self) -> &'static str {
        match self {
            FlashError::BadHex(_) => "firmware-invalid",
            FlashError::TooLarge { .. } => "firmware-too-large",
            FlashError::NoSync { .. } => "board-not-answering",
            FlashError::LostSync { .. } => "board-stopped-answering",
            FlashError::WrongSignature { .. } => "wrong-board",
            FlashError::VerifyFailed { .. } => "verify-failed",
            FlashError::DamagedAnswers => "damaged-answers",
            FlashError::Refused { .. } => "board-refused",
            FlashError::Port(_) => "port-failed",
        }
    }

    /// What happened.
    pub fn message(&self) -> String {
        match self {
            FlashError::BadHex(e) => format!("The firmware file can't be used: {e}."),
            FlashError::TooLarge { board, end, room } => {
                format!("The program is {end} bytes, but the {board} has room for {room}.")
            }
            FlashError::NoSync { .. } => "The board didn't answer the bootloader.".to_string(),
            FlashError::LostSync { while_ } => {
                format!("The board stopped answering while {while_}.")
            }
            FlashError::WrongSignature { board, expected, found } => format!(
                "This board has an {} ({}), but an {board} has an {} ({}).",
                chip_name(*found).unwrap_or("unknown chip"),
                sig(*found),
                chip_name(*expected).unwrap_or("unknown chip"),
                sig(*expected),
            ),
            FlashError::VerifyFailed { address, wrote, read } => format!(
                "Reading the program back found a difference at address 0x{address:05X}: wrote 0x{wrote:02X}, read 0x{read:02X}."
            ),
            FlashError::DamagedAnswers => "The board's answers kept arriving damaged.".to_string(),
            FlashError::Refused { what } => format!("The bootloader refused to {what}."),
            FlashError::Port(e) => format!("The port stopped working: {e}."),
        }
    }

    /// The next thing to try.
    pub fn hint(&self) -> String {
        match self {
            FlashError::BadHex(_) => {
                "Export it again from the Arduino IDE (Sketch → Export Compiled Binary) and pick the .hex without \"with_bootloader\" in its name.".to_string()
            }
            FlashError::TooLarge { .. } => {
                "Check that you picked the right board, or make the sketch smaller.".to_string()
            }
            FlashError::NoSync { board } => format!(
                "Try pressing reset just as you press Flash. Also check that the board really is an {board}: an Uno and a Mega speak different bootloader languages. Nothing was written."
            ),
            FlashError::LostSync { .. } => {
                "Check the USB cable, then flash again. The old program may be partly overwritten; flashing again fixes that.".to_string()
            }
            FlashError::WrongSignature { .. } => {
                "Pick the board that is really plugged in, with firmware made for it. Nothing was written.".to_string()
            }
            FlashError::VerifyFailed { .. } => {
                "Flash again. If it keeps failing at the same address, the chip's flash may be worn out.".to_string()
            }
            FlashError::DamagedAnswers => {
                "Try another USB cable or port, away from motors and their power wires.".to_string()
            }
            FlashError::Refused { .. } => "Press reset on the board, then flash again.".to_string(),
            FlashError::Port(_) => "Check that the board is still plugged in, then flash again.".to_string(),
        }
    }
}

fn sig(s: [u8; 3]) -> String {
    format!("{:02X} {:02X} {:02X}", s[0], s[1], s[2])
}

impl fmt::Display for FlashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.message(), self.hint())
    }
}

impl std::error::Error for FlashError {}

impl From<HexError> for FlashError {
    fn from(e: HexError) -> Self {
        FlashError::BadHex(e)
    }
}
