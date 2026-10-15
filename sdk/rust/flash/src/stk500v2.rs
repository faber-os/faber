//! STK500 version 2 (Atmel application note AVR068), the subset the Arduino Mega 2560's
//! bootloader (`stk500boot`) speaks.
//!
//! Every message is a frame: `MESSAGE_START` (0x1B), a sequence number the answer repeats, the
//! body's size (two bytes, big-endian), `TOKEN` (0x0E), the body, and a checksum: the XOR of
//! every byte before it. A body starts with the command; an answer's second byte is its status.
//! Addresses are in 16-bit words, big-endian; the Mega's bootloader counts on by itself after
//! every page.

use crate::FlashError;
use crate::flasher::{Bootloader, Timing};
use crate::port::{Link, Recv};

pub const MESSAGE_START: u8 = 0x1B;
pub const TOKEN: u8 = 0x0E;

pub const CMD_SIGN_ON: u8 = 0x01;
pub const CMD_SET_PARAMETER: u8 = 0x02;
pub const CMD_GET_PARAMETER: u8 = 0x03;
pub const CMD_LOAD_ADDRESS: u8 = 0x06;
pub const CMD_ENTER_PROGMODE_ISP: u8 = 0x10;
pub const CMD_LEAVE_PROGMODE_ISP: u8 = 0x11;
pub const CMD_CHIP_ERASE_ISP: u8 = 0x12;
pub const CMD_PROGRAM_FLASH_ISP: u8 = 0x13;
pub const CMD_READ_FLASH_ISP: u8 = 0x14;
pub const CMD_READ_SIGNATURE_ISP: u8 = 0x1B;
/// What an STK500 answers when a frame arrived with a bad checksum.
pub const ANSWER_CKSUM_ERROR: u8 = 0xB0;

pub const STATUS_CMD_OK: u8 = 0x00;
pub const STATUS_CMD_FAILED: u8 = 0xC0;
pub const STATUS_CKSUM_ERROR: u8 = 0xC1;

/// The largest body this side accepts: a 256-byte page and a little header.
pub const MAX_BODY: usize = 300;

/// How often a command is sent again after a damaged or missing answer.
const TRIES: u32 = 4;

/// A whole frame for `body`.
pub fn frame(seq: u8, body: &[u8]) -> Vec<u8> {
    let size = (body.len() as u16).to_be_bytes();
    let mut out = vec![MESSAGE_START, seq, size[0], size[1], TOKEN];
    out.extend_from_slice(body);
    out.push(checksum(&out));
    out
}

pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, b| a ^ b)
}

enum Answer {
    Body(Vec<u8>),
    Damaged,
    Missing,
}

pub(crate) struct Stk500v2 {
    timing: Timing,
    seq: u8,
}

impl Stk500v2 {
    pub fn new(timing: Timing) -> Self {
        Stk500v2 { timing, seq: 0 }
    }

    /// Reads one frame with sequence number `seq`, skipping whatever comes before its start.
    fn receive(&mut self, link: &mut Link, seq: u8, wait: std::time::Duration) -> Result<Answer, FlashError> {
        loop {
            match link.byte(wait)? {
                Recv::Byte(MESSAGE_START) => break,
                Recv::Byte(_) => continue,
                Recv::TimedOut => return Ok(Answer::Missing),
            }
        }
        let t = self.timing.answer;
        let Ok(head) = link.exact(4, t)? else { return Ok(Answer::Damaged) };
        let size = u16::from_be_bytes([head[1], head[2]]) as usize;
        if head[3] != TOKEN || size == 0 || size > MAX_BODY {
            return Ok(Answer::Damaged);
        }
        let Ok(rest) = link.exact(size + 1, t)? else { return Ok(Answer::Damaged) };
        let mut whole = vec![MESSAGE_START];
        whole.extend_from_slice(&head);
        whole.extend_from_slice(&rest[..size]);
        if checksum(&whole) != rest[size] || head[0] != seq {
            return Ok(Answer::Damaged);
        }
        Ok(Answer::Body(rest[..size].to_vec()))
    }

    /// Sends `bodies` one after the other and returns the last answer's body, once every
    /// status is OK. A damaged answer, or none, sends the whole sequence again with new sequence
    /// numbers: a page command after its address, since the bootloader moves on by itself after
    /// every page, and a lost answer may hide a page that was written.
    fn command(
        &mut self,
        link: &mut Link,
        bodies: &[&[u8]],
        while_: &'static str,
    ) -> Result<Vec<u8>, FlashError> {
        let mut damaged = 0;
        'tries: for _ in 0..TRIES {
            let mut last = Vec::new();
            for body in bodies {
                self.seq = self.seq.wrapping_add(1);
                link.send(&frame(self.seq, body))?;
                match self.receive(link, self.seq, self.timing.answer)? {
                    Answer::Body(a) if a[0] == ANSWER_CKSUM_ERROR => {
                        damaged += 1;
                        continue 'tries;
                    }
                    Answer::Body(a) if a[0] != body[0] || a.len() < 2 => {
                        return Err(FlashError::LostSync { while_ });
                    }
                    Answer::Body(a) if a[1] != STATUS_CMD_OK => {
                        return Err(FlashError::Refused { what: while_ });
                    }
                    Answer::Body(a) => last = a,
                    Answer::Damaged => {
                        damaged += 1;
                        link.drain(self.timing.quiet)?;
                        continue 'tries;
                    }
                    Answer::Missing => continue 'tries,
                }
            }
            return Ok(last);
        }
        Err(if damaged > 0 { FlashError::DamagedAnswers } else { FlashError::LostSync { while_ } })
    }
}

/// `LOAD_ADDRESS` for a byte address. Bit 31 asks for the extended address, needed above 128 KB
/// (the ATmega2560's upper half).
fn load_address(address: u32) -> Vec<u8> {
    let word = (address / 2) | 0x8000_0000;
    let mut body = vec![CMD_LOAD_ADDRESS];
    body.extend_from_slice(&word.to_be_bytes());
    body
}

impl Bootloader for Stk500v2 {
    fn sync(&mut self, link: &mut Link) -> Result<bool, FlashError> {
        for _ in 0..self.timing.sync_tries {
            self.seq = self.seq.wrapping_add(1);
            link.send(&frame(self.seq, &[CMD_SIGN_ON]))?;
            match self.receive(link, self.seq, self.timing.sync_wait)? {
                Answer::Body(a) if a.len() >= 2 && a[0] == CMD_SIGN_ON && a[1] == STATUS_CMD_OK => {
                    return Ok(true);
                }
                Answer::Missing => {}
                _ => link.drain(self.timing.quiet)?,
            }
        }
        Ok(false)
    }

    fn enter(&mut self, link: &mut Link) -> Result<(), FlashError> {
        // timeout, stabDelay, cmdexeDelay, synchLoops, byteDelay, pollValue, pollIndex and the
        // ISP "programming enable" command, as avrdude sends them for the ATmega2560. The
        // bootloader ignores them, but a real STK500 would need them.
        let body = [CMD_ENTER_PROGMODE_ISP, 200, 100, 25, 32, 0, 0x53, 3, 0xAC, 0x53, 0x00, 0x00];
        self.command(link, &[&body], "starting to program").map(|_| ())
    }

    fn signature(&mut self, link: &mut Link) -> Result<[u8; 3], FlashError> {
        let mut sig = [0u8; 3];
        for (i, s) in sig.iter_mut().enumerate() {
            let body = [CMD_READ_SIGNATURE_ISP, 4, 0x30, 0x00, i as u8, 0x00];
            let a = self.command(link, &[&body], "reading the chip's signature")?;
            *s = *a.get(2).ok_or(FlashError::LostSync { while_: "reading the chip's signature" })?;
        }
        Ok(sig)
    }

    fn write_page(&mut self, link: &mut Link, address: u32, data: &[u8]) -> Result<(), FlashError> {
        let while_ = "writing the program";
        let n = (data.len() as u16).to_be_bytes();
        // Page mode, write the page; delay; the ISP load/write/read commands; poll values.
        let mut body = vec![CMD_PROGRAM_FLASH_ISP, n[0], n[1], 0xC1, 10, 0x40, 0x4C, 0x20, 0x00, 0x00];
        body.extend_from_slice(data);
        self.command(link, &[&load_address(address), &body], while_).map(|_| ())
    }

    fn read_page(&mut self, link: &mut Link, address: u32, len: usize) -> Result<Vec<u8>, FlashError> {
        let while_ = "reading the program back";
        let n = (len as u16).to_be_bytes();
        let a =
            self.command(link, &[&load_address(address), &[CMD_READ_FLASH_ISP, n[0], n[1], 0x20]], while_)?;
        // [command, status, data…, status]
        if a.len() != len + 3 {
            return Err(FlashError::LostSync { while_ });
        }
        Ok(a[2..2 + len].to_vec())
    }

    fn leave(&mut self, link: &mut Link) -> Result<(), FlashError> {
        self.command(link, &[&[CMD_LEAVE_PROGMODE_ISP, 1, 1]], "starting the new program").map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_as_avr068_draws_it() {
        // SIGN_ON with sequence 1: 1B 01 00 01 0E 01 and the XOR of those.
        assert_eq!(frame(1, &[CMD_SIGN_ON]), vec![0x1B, 0x01, 0x00, 0x01, 0x0E, 0x01, 0x14]);
        let f = frame(7, &[1, 2, 3]);
        assert_eq!(checksum(&f), 0, "the checksum makes the whole frame XOR to zero");
    }
}
