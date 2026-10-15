//! Simulated boards with their bootloaders, written from the protocol specs (AVR061 for
//! STK500v1 as Optiboot implements it, AVR068 for STK500v2 as the Mega's `stk500boot` does), so
//! the flasher is tested without hardware. Each has a flash memory to write and read back, and
//! faults to switch on: no bootloader, falling silent, a bad flash cell, damaged answers.
//!
//! [`SimBoard`] takes the time from its caller, so it works behind the core's fake ports too;
//! [`SimPort`] puts one behind a [`Port`](crate::Port) with a fake clock, for the unit tests.

use crate::board::{Board, Protocol};
use crate::port::Port;
use crate::{stk500v1 as v1, stk500v2 as v2};
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

/// How long a bootloader waits for its first command (and between commands) before it starts
/// the sketch. Optiboot's watchdog is set to 1 s.
pub const BOOT_WINDOW: Duration = Duration::from_millis(1000);

/// What can go wrong on the simulated board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Faults {
    /// The reset line doesn't reach the chip (or the bootloader is gone): it never answers.
    pub no_bootloader: bool,
    /// The board stops answering after this many page writes, as if the cable were pulled.
    pub silent_after_pages: Option<usize>,
    /// This flash address keeps bit 0 set, whatever is written.
    pub bad_cell: Option<u32>,
    /// STK500v2: this many answers leave with a broken checksum…
    pub damaged_answers: usize,
    /// …after this many intact ones.
    pub intact_answers: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The sketch runs; the bootloader isn't listening.
    Sketch,
    Bootloader,
    /// Gone silent (a fault).
    Dead,
}

/// A board: chip, flash, bootloader.
#[derive(Clone, Debug)]
pub struct SimBoard {
    protocol: Protocol,
    signature: [u8; 3],
    flash: Vec<u8>,
    pub faults: Faults,
    mode: Mode,
    dtr: bool,
    last_rx: Duration,
    rx: Vec<u8>,
    tx: VecDeque<u8>,
    /// The bootloader's address (bytes).
    address: u32,
    pages_written: usize,
    resets: u32,
    left_bootloader: bool,
}

impl SimBoard {
    /// A board like `board`, with an erased flash. The bootloader is the one the board ships
    /// with (Optiboot or `stk500boot`).
    pub fn new(board: &Board) -> SimBoard {
        SimBoard::with_chip(board.protocol, board.signature, board.room + 8192)
    }

    /// A board speaking `protocol` with any chip, e.g. an old Nano's ATmega168 for a wrong
    /// signature.
    pub fn with_chip(protocol: Protocol, signature: [u8; 3], flash_bytes: u32) -> SimBoard {
        SimBoard {
            protocol,
            signature,
            flash: vec![0xFF; flash_bytes as usize],
            faults: Faults::default(),
            mode: Mode::Sketch,
            dtr: true, // the OS asserts DTR when it opens the port
            last_rx: Duration::ZERO,
            rx: Vec::new(),
            tx: VecDeque::new(),
            address: 0,
            pages_written: 0,
            resets: 0,
            left_bootloader: false,
        }
    }

    pub fn with_faults(mut self, faults: Faults) -> SimBoard {
        self.faults = faults;
        self
    }

    /// The whole flash.
    pub fn flash(&self) -> &[u8] {
        &self.flash
    }

    /// How often the chip was reset.
    pub fn resets(&self) -> u32 {
        self.resets
    }

    /// How many pages the bootloader wrote.
    pub fn pages_written(&self) -> usize {
        self.pages_written
    }

    /// True once the bootloader was told to leave and started the new sketch.
    pub fn started_new_sketch(&self) -> bool {
        self.left_bootloader && self.mode == Mode::Sketch
    }

    /// DTR/RTS changed. Asserting them pulses the reset pin through the board's capacitor.
    pub fn set_dtr_rts(&mut self, asserted: bool, now: Duration) {
        if asserted && !self.dtr {
            self.resets += 1;
            self.rx.clear();
            self.tx.clear();
            self.last_rx = now;
            self.mode = if self.faults.no_bootloader { Mode::Sketch } else { Mode::Bootloader };
        }
        self.dtr = asserted;
    }

    /// Bytes from the computer.
    pub fn receive(&mut self, bytes: &[u8], now: Duration) {
        self.tick(now);
        if self.mode != Mode::Bootloader {
            return;
        }
        self.last_rx = now;
        self.rx.extend_from_slice(bytes);
        match self.protocol {
            Protocol::Stk500v1 => self.run_v1(),
            Protocol::Stk500v2 => self.run_v2(),
        }
    }

    /// Bytes for the computer.
    pub fn take_output(&mut self, now: Duration) -> Vec<u8> {
        self.tick(now);
        self.tx.drain(..).collect()
    }

    fn tick(&mut self, now: Duration) {
        if self.mode == Mode::Bootloader && now.saturating_sub(self.last_rx) > BOOT_WINDOW {
            self.mode = Mode::Sketch;
        }
    }

    fn write(&mut self, address: u32, data: &[u8]) {
        if let Some(n) = self.faults.silent_after_pages
            && self.pages_written >= n
        {
            self.mode = Mode::Dead;
            return;
        }
        for (k, b) in data.iter().enumerate() {
            let a = (address as usize + k) % self.flash.len();
            self.flash[a] = *b;
            if self.faults.bad_cell == Some(a as u32) {
                self.flash[a] |= 1;
            }
        }
        self.pages_written += 1;
    }

    fn read(&self, address: u32, len: usize) -> Vec<u8> {
        (0..len).map(|k| self.flash[(address as usize + k) % self.flash.len()]).collect()
    }

    /// Optiboot's main loop: one command at a time, each ending in `CRC_EOP`. A command that
    /// doesn't end in it makes Optiboot reset through its watchdog, which starts the sketch.
    fn run_v1(&mut self) {
        while let Some(&cmd) = self.rx.first() {
            let need = match cmd {
                v1::GET_PARAMETER => 3,
                v1::SET_DEVICE => 22,
                v1::SET_DEVICE_EXT => 7,
                v1::LOAD_ADDRESS => 4,
                v1::UNIVERSAL => 6,
                v1::READ_PAGE => 5,
                v1::PROG_PAGE if self.rx.len() >= 3 => {
                    5 + u16::from_be_bytes([self.rx[1], self.rx[2]]) as usize
                }
                v1::PROG_PAGE => 3,
                _ => 2,
            };
            if self.rx.len() < need {
                return;
            }
            let c: Vec<u8> = self.rx.drain(..need).collect();
            if c[need - 1] != v1::CRC_EOP {
                self.mode = Mode::Sketch;
                self.rx.clear();
                return;
            }
            let mut answer = vec![];
            match cmd {
                v1::GET_PARAMETER => answer.push(0x03),
                v1::UNIVERSAL => answer.push(0x00),
                v1::LOAD_ADDRESS => self.address = u16::from_le_bytes([c[1], c[2]]) as u32 * 2,
                v1::PROG_PAGE => {
                    self.write(self.address, &c[4..need - 1]);
                    if self.mode == Mode::Dead {
                        return;
                    }
                }
                v1::READ_PAGE => {
                    answer = self.read(self.address, u16::from_be_bytes([c[1], c[2]]) as usize);
                }
                v1::READ_SIGN => answer.extend_from_slice(&self.signature),
                _ => {}
            }
            self.tx.push_back(v1::STK_INSYNC);
            self.tx.extend(answer);
            self.tx.push_back(v1::STK_OK);
            if cmd == v1::LEAVE_PROGMODE {
                self.mode = Mode::Sketch;
                self.left_bootloader = true;
                return;
            }
        }
    }

    /// `stk500boot`'s loop: frames in, frames out. A frame with a bad checksum is dropped
    /// without an answer, as the Mega's bootloader does.
    fn run_v2(&mut self) {
        loop {
            match self.rx.iter().position(|&b| b == v2::MESSAGE_START) {
                Some(i) => drop(self.rx.drain(..i)),
                None => {
                    self.rx.clear();
                    return;
                }
            }
            if self.rx.len() < 5 {
                return;
            }
            let size = u16::from_be_bytes([self.rx[2], self.rx[3]]) as usize;
            if self.rx[4] != v2::TOKEN || size > 285 {
                self.rx.remove(0);
                continue;
            }
            if self.rx.len() < 6 + size {
                return;
            }
            let f: Vec<u8> = self.rx.drain(..6 + size).collect();
            if v2::checksum(&f) != 0 {
                continue;
            }
            let (seq, body) = (f[1], &f[5..5 + size]);
            let answer = self.answer_v2(body);
            if self.mode == Mode::Dead {
                return;
            }
            let mut out = v2::frame(seq, &answer);
            if self.faults.intact_answers > 0 {
                self.faults.intact_answers -= 1;
            } else if self.faults.damaged_answers > 0 {
                self.faults.damaged_answers -= 1;
                *out.last_mut().expect("a frame has a checksum") ^= 0xFF;
            }
            self.tx.extend(out);
            if body[0] == v2::CMD_LEAVE_PROGMODE_ISP {
                self.mode = Mode::Sketch;
                self.left_bootloader = true;
                return;
            }
        }
    }

    fn answer_v2(&mut self, body: &[u8]) -> Vec<u8> {
        let ok = v2::STATUS_CMD_OK;
        let n = |b: &[u8]| u16::from_be_bytes([b[1], b[2]]) as usize;
        match body[0] {
            v2::CMD_SIGN_ON => {
                let mut a = vec![v2::CMD_SIGN_ON, ok, 8];
                a.extend_from_slice(b"AVRISP_2");
                a
            }
            v2::CMD_GET_PARAMETER => vec![body[0], ok, 0],
            v2::CMD_LOAD_ADDRESS if body.len() == 5 => {
                let word = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) & 0x7FFF_FFFF;
                self.address = word << 1;
                vec![body[0], ok]
            }
            v2::CMD_PROGRAM_FLASH_ISP if body.len() >= 10 && body.len() == 10 + n(body) => {
                self.write(self.address, &body[10..]);
                self.address += n(body) as u32;
                vec![body[0], ok]
            }
            v2::CMD_READ_FLASH_ISP if body.len() == 4 => {
                let mut a = vec![body[0], ok];
                a.extend(self.read(self.address, n(body)));
                a.push(ok);
                self.address += n(body) as u32;
                a
            }
            v2::CMD_READ_SIGNATURE_ISP if body.len() == 6 => {
                vec![body[0], ok, self.signature[(body[4] as usize).min(2)], ok]
            }
            v2::CMD_SET_PARAMETER
            | v2::CMD_ENTER_PROGMODE_ISP
            | v2::CMD_LEAVE_PROGMODE_ISP
            | v2::CMD_CHIP_ERASE_ISP => vec![body[0], ok],
            other => vec![other, v2::STATUS_CMD_FAILED],
        }
    }
}

/// A [`SimBoard`] behind a [`Port`], with a fake clock: reading with nothing to read moves time
/// on by 5 ms, sleeping by the time asked. A whole flash takes no real time at all.
pub struct SimPort {
    pub board: SimBoard,
    now: Duration,
    /// Every byte the flasher sent, in order.
    pub sent: Vec<u8>,
}

impl SimPort {
    pub fn new(board: SimBoard) -> SimPort {
        SimPort { board, now: Duration::from_secs(10), sent: Vec::new() }
    }
}

impl Port for SimPort {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.sent.extend_from_slice(data);
        self.board.receive(data, self.now);
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let out = self.board.take_output(self.now);
        if out.is_empty() {
            self.now += Duration::from_millis(5);
            return Ok(0);
        }
        let n = out.len().min(buf.len());
        buf[..n].copy_from_slice(&out[..n]);
        // Whatever doesn't fit waits for the next read.
        let mut rest: VecDeque<u8> = out[n..].iter().copied().collect();
        rest.extend(self.board.tx.drain(..));
        self.board.tx = rest;
        Ok(n)
    }

    fn set_dtr_rts(&mut self, asserted: bool) -> io::Result<()> {
        self.board.set_dtr_rts(asserted, self.now);
        Ok(())
    }

    fn sleep(&mut self, d: Duration) {
        self.now += d;
    }

    fn now(&self) -> Duration {
        self.now
    }
}
