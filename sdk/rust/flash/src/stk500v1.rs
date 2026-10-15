//! STK500 version 1 (Atmel application note AVR061), the subset Optiboot speaks: the bootloader
//! on the Uno and on Nanos since 2018 (and ATmegaBOOT on older Nanos, at 57 600 baud).
//!
//! Every command ends with `CRC_EOP` (0x20, a space; there is no checksum). Every answer starts
//! with `STK_INSYNC` (0x14) and ends with `STK_OK` (0x10). Addresses are in 16-bit words,
//! little-endian.

use crate::FlashError;
use crate::flasher::{Bootloader, Timing};
use crate::port::{Link, Recv};

pub const STK_OK: u8 = 0x10;
pub const STK_FAILED: u8 = 0x11;
pub const STK_INSYNC: u8 = 0x14;
pub const STK_NOSYNC: u8 = 0x15;
pub const CRC_EOP: u8 = 0x20;

pub const GET_SYNC: u8 = 0x30;
pub const GET_PARAMETER: u8 = 0x41;
pub const SET_DEVICE: u8 = 0x42;
pub const SET_DEVICE_EXT: u8 = 0x45;
pub const ENTER_PROGMODE: u8 = 0x50;
pub const LEAVE_PROGMODE: u8 = 0x51;
pub const LOAD_ADDRESS: u8 = 0x55;
pub const UNIVERSAL: u8 = 0x56;
pub const PROG_PAGE: u8 = 0x64;
pub const READ_PAGE: u8 = 0x74;
pub const READ_SIGN: u8 = 0x75;

/// The memory type in `PROG_PAGE` and `READ_PAGE`: flash.
pub const FLASH: u8 = b'F';

pub(crate) struct Stk500v1 {
    timing: Timing,
}

impl Stk500v1 {
    pub fn new(timing: Timing) -> Self {
        Stk500v1 { timing }
    }

    /// Sends a command and reads its answer: `INSYNC`, `len` bytes, `OK`.
    fn command(
        &mut self,
        link: &mut Link,
        bytes: &[u8],
        len: usize,
        while_: &'static str,
    ) -> Result<Vec<u8>, FlashError> {
        link.send(bytes)?;
        match link.byte(self.timing.answer)? {
            Recv::Byte(STK_INSYNC) => {}
            _ => return Err(FlashError::LostSync { while_ }),
        }
        let Ok(body) = link.exact(len, self.timing.answer)? else {
            return Err(FlashError::LostSync { while_ });
        };
        match link.byte(self.timing.answer)? {
            Recv::Byte(STK_OK) => Ok(body),
            Recv::Byte(STK_FAILED) => Err(FlashError::Refused { what: while_ }),
            _ => Err(FlashError::LostSync { while_ }),
        }
    }

    fn load_address(
        &mut self,
        link: &mut Link,
        address: u32,
        while_: &'static str,
    ) -> Result<(), FlashError> {
        let word = ((address / 2) as u16).to_le_bytes();
        self.command(link, &[LOAD_ADDRESS, word[0], word[1], CRC_EOP], 0, while_).map(|_| ())
    }
}

impl Bootloader for Stk500v1 {
    fn sync(&mut self, link: &mut Link) -> Result<bool, FlashError> {
        for _ in 0..self.timing.sync_tries {
            link.send(&[GET_SYNC, CRC_EOP])?;
            if let Recv::Byte(STK_INSYNC) = link.byte(self.timing.sync_wait)?
                && let Recv::Byte(STK_OK) = link.byte(self.timing.answer)?
            {
                return Ok(true);
            }
            // Something else (the old sketch talking, half an answer): start clean.
            link.drain(self.timing.quiet)?;
        }
        Ok(false)
    }

    fn enter(&mut self, link: &mut Link) -> Result<(), FlashError> {
        self.command(link, &[ENTER_PROGMODE, CRC_EOP], 0, "starting to program").map(|_| ())
    }

    fn signature(&mut self, link: &mut Link) -> Result<[u8; 3], FlashError> {
        let s = self.command(link, &[READ_SIGN, CRC_EOP], 3, "reading the chip's signature")?;
        Ok([s[0], s[1], s[2]])
    }

    fn write_page(&mut self, link: &mut Link, address: u32, data: &[u8]) -> Result<(), FlashError> {
        let while_ = "writing the program";
        self.load_address(link, address, while_)?;
        let len = (data.len() as u16).to_be_bytes();
        let mut cmd = vec![PROG_PAGE, len[0], len[1], FLASH];
        cmd.extend_from_slice(data);
        cmd.push(CRC_EOP);
        self.command(link, &cmd, 0, while_).map(|_| ())
    }

    fn read_page(&mut self, link: &mut Link, address: u32, len: usize) -> Result<Vec<u8>, FlashError> {
        let while_ = "reading the program back";
        self.load_address(link, address, while_)?;
        let n = (len as u16).to_be_bytes();
        self.command(link, &[READ_PAGE, n[0], n[1], FLASH, CRC_EOP], len, while_)
    }

    fn leave(&mut self, link: &mut Link) -> Result<(), FlashError> {
        self.command(link, &[LEAVE_PROGMODE, CRC_EOP], 0, "starting the new program").map(|_| ())
    }
}
