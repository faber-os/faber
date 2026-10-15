//! The boards the flasher knows, and guessing one from its USB ids.
//!
//! Numbers from the chips' datasheets and the Arduino AVR core's `boards.txt`: the flash a
//! sketch may use is the chip's flash minus the bootloader (Optiboot: 512 bytes; the Nano's old
//! ATmegaBOOT: 2 KB; the Mega's STK500v2 bootloader: 8 KB).

/// Which bootloader protocol a board speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// STK500 version 1 (Atmel AVR061), as Optiboot and ATmegaBOOT speak it.
    Stk500v1,
    /// STK500 version 2 (Atmel AVR068), as the Mega 2560's bootloader speaks it.
    Stk500v2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Board {
    /// The id in the API and in a package's `firmware/<id>.hex`.
    pub id: &'static str,
    /// The board's name, as Arduino calls it.
    pub name: &'static str,
    /// Its microcontroller.
    pub chip: &'static str,
    /// The chip's signature bytes, read before anything is written.
    pub signature: [u8; 3],
    /// The flash a program may use (the chip's flash minus the bootloader), in bytes.
    pub room: u32,
    /// The flash page, the unit the bootloader writes.
    pub page_bytes: u32,
    pub protocol: Protocol,
    pub baud: u32,
}

pub const ATMEGA328P: [u8; 3] = [0x1E, 0x95, 0x0F];
pub const ATMEGA2560: [u8; 3] = [0x1E, 0x98, 0x01];

pub const UNO: Board = Board {
    id: "uno",
    name: "Arduino Uno",
    chip: "ATmega328P",
    signature: ATMEGA328P,
    room: 32_768 - 512,
    page_bytes: 128,
    protocol: Protocol::Stk500v1,
    baud: 115_200,
};

pub const NANO: Board = Board { id: "nano", name: "Arduino Nano", ..UNO };

/// A Nano (or clone) still on the bootloader it shipped with before 2018.
pub const NANO_OLD: Board =
    Board { id: "nano-old", name: "Arduino Nano (old bootloader)", room: 32_768 - 2048, baud: 57_600, ..UNO };

pub const MEGA2560: Board = Board {
    id: "mega2560",
    name: "Arduino Mega 2560",
    chip: "ATmega2560",
    signature: ATMEGA2560,
    room: 262_144 - 8192,
    page_bytes: 256,
    protocol: Protocol::Stk500v2,
    baud: 115_200,
};

/// Every board the flasher can program.
pub const BOARDS: &[Board] = &[UNO, NANO, NANO_OLD, MEGA2560];

impl Board {
    pub fn by_id(id: &str) -> Option<&'static Board> {
        BOARDS.iter().find(|b| b.id == id)
    }
}

/// The name of the chip with this signature, for "this board is an ATmega2560, not …".
pub fn chip_name(signature: [u8; 3]) -> Option<&'static str> {
    Some(match signature {
        ATMEGA328P => "ATmega328P",
        [0x1E, 0x95, 0x14] => "ATmega328",
        [0x1E, 0x94, 0x06] => "ATmega168",
        [0x1E, 0x94, 0x0B] => "ATmega168P",
        [0x1E, 0x93, 0x07] => "ATmega8",
        ATMEGA2560 => "ATmega2560",
        [0x1E, 0x97, 0x03] => "ATmega1280",
        [0x1E, 0x95, 0x87] => "ATmega32U4",
        _ => return None,
    })
}

/// What a port's USB ids say about the board on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detected {
    /// An official board: its USB chip says which one.
    Board(&'static Board),
    /// A USB-serial chip that clones (and many other devices) use. It says nothing about the
    /// board, so the user picks it.
    UsbSerial { chip: &'static str },
    /// Nothing the flasher recognises.
    Unknown,
}

/// Guesses the board from a port's USB vendor and product id.
pub fn detect(vid: u16, pid: u16) -> Detected {
    match (vid, pid) {
        // Arduino LLC (0x2341) and Arduino SRL (0x2A03), from the AVR core's boards.txt.
        (0x2341, 0x0043 | 0x0001 | 0x0243 | 0x006A) | (0x2A03, 0x0043) => Detected::Board(&UNO),
        (0x2341, 0x0010 | 0x0042 | 0x0210 | 0x0242) | (0x2A03, 0x0010 | 0x0042) => Detected::Board(&MEGA2560),
        (0x1A86, 0x7523 | 0x5523) => Detected::UsbSerial { chip: "CH340" },
        (0x0403, 0x6001) => Detected::UsbSerial { chip: "FT232" },
        _ => Detected::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_boards_are_known_by_their_ids() {
        assert_eq!(detect(0x2341, 0x0043), Detected::Board(&UNO));
        assert_eq!(detect(0x2A03, 0x0043), Detected::Board(&UNO));
        assert_eq!(detect(0x2341, 0x0042), Detected::Board(&MEGA2560));
        assert_eq!(detect(0x2341, 0x0010), Detected::Board(&MEGA2560));
    }

    #[test]
    fn clones_only_name_their_usb_chip() {
        assert_eq!(detect(0x1A86, 0x7523), Detected::UsbSerial { chip: "CH340" });
        assert_eq!(detect(0x0403, 0x6001), Detected::UsbSerial { chip: "FT232" });
        assert_eq!(detect(0x10C4, 0xEA60), Detected::Unknown);
    }

    #[test]
    fn room_leaves_the_bootloader_alone() {
        assert_eq!(UNO.room, 32_256);
        assert_eq!(NANO_OLD.room, 30_720);
        assert_eq!(MEGA2560.room, 253_952);
        for b in BOARDS {
            assert_eq!(b.room % b.page_bytes, 0, "{}", b.id);
            assert_eq!(Board::by_id(b.id), Some(b));
            assert!(chip_name(b.signature).is_some());
        }
    }
}
