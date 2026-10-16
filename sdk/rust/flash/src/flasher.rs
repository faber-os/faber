//! Putting a program on a board: reset, sync, check the signature, write, read back.

use crate::FlashError;
use crate::board::{Board, Protocol};
use crate::hex::Image;
use crate::port::{Link, Port};
use crate::stk500v1::Stk500v1;
use crate::stk500v2::Stk500v2;
use std::time::Duration;

/// How long to wait for the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// For each answer once the bootloader talks.
    pub answer: Duration,
    /// For an answer to each sync attempt.
    pub sync_wait: Duration,
    /// How many times to try to sync. Optiboot gives up after 1 s without a command, so the
    /// attempts together cover a bit more than that (and a reset pressed by hand).
    pub sync_tries: u32,
    /// How long the line must be quiet before the flasher talks (it throws the rest away).
    pub quiet: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            answer: Duration::from_millis(1000),
            sync_wait: Duration::from_millis(200),
            sync_tries: 10,
            quiet: Duration::from_millis(50),
        }
    }
}

/// Where flashing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Resetting the board and waiting for its bootloader.
    Connecting,
    Writing,
    Verifying,
    Done,
}

/// Reported after every page, so a progress bar can follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    /// Bytes written or read back so far, in this stage.
    pub done: u32,
    /// Bytes to write (and to read back): whole pages.
    pub total: u32,
}

/// What was put on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flashed {
    /// Where the program ends.
    pub bytes: u32,
    /// Pages written and read back.
    pub pages: u32,
}

/// What reading a program back found ([`read_back`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadBack {
    /// Pages read.
    pub pages: u32,
    /// The first address where the board differs from the program, or `None`: the same.
    pub first_difference: Option<u32>,
    /// Every byte read, page after page in address order: the pages the program would be
    /// written in, exactly as [`flash()`] writes them (unused bytes of a page are erased, `0xFF`).
    pub read: Vec<u8>,
}

/// One bootloader protocol.
pub(crate) trait Bootloader {
    /// Tries to get the bootloader's attention. `false`: it never answered.
    fn sync(&mut self, link: &mut Link) -> Result<bool, FlashError>;
    fn enter(&mut self, link: &mut Link) -> Result<(), FlashError>;
    fn signature(&mut self, link: &mut Link) -> Result<[u8; 3], FlashError>;
    fn write_page(&mut self, link: &mut Link, address: u32, data: &[u8]) -> Result<(), FlashError>;
    fn read_page(&mut self, link: &mut Link, address: u32, len: usize) -> Result<Vec<u8>, FlashError>;
    /// Leaves the bootloader, which starts the new program.
    fn leave(&mut self, link: &mut Link) -> Result<(), FlashError>;
}

/// Puts `image` on the `board` on `port` (open at `board.baud`). Nothing is written before the
/// program is known to fit, the bootloader has answered and the chip's signature is the board's.
pub fn flash(
    port: &mut dyn Port,
    board: &Board,
    image: &Image,
    timing: &Timing,
    progress: &mut dyn FnMut(Progress),
) -> Result<Flashed, FlashError> {
    if image.end() > board.room {
        return Err(FlashError::TooLarge { board: board.name, end: image.end(), room: board.room });
    }
    let pages = image.pages(board.page_bytes);
    let total = pages.len() as u32 * board.page_bytes;
    let report = |p: &mut dyn FnMut(Progress), stage, done| p(Progress { stage, done, total });
    report(progress, Stage::Connecting, 0);

    let mut link = Link::new(port);
    let mut boot = connect(&mut link, board, timing)?;

    let result = (|| {
        let page = board.page_bytes;
        for (i, &address) in pages.iter().enumerate() {
            boot.write_page(&mut link, address, &image.page(address, page))?;
            report(progress, Stage::Writing, (i as u32 + 1) * page);
        }
        for (i, &address) in pages.iter().enumerate() {
            let wrote = image.page(address, page);
            let read = boot.read_page(&mut link, address, page as usize)?;
            if let Some(k) = (0..wrote.len()).find(|&k| read.get(k) != Some(&wrote[k])) {
                return Err(FlashError::VerifyFailed {
                    address: address + k as u32,
                    wrote: wrote[k],
                    read: read.get(k).copied().unwrap_or(0),
                });
            }
            report(progress, Stage::Verifying, (i as u32 + 1) * page);
        }
        Ok(())
    })();
    match result {
        Ok(()) => boot.leave(&mut link)?,
        // Leave the bootloader if it still listens, so the board doesn't sit in it.
        Err(e @ (FlashError::VerifyFailed { .. } | FlashError::Refused { .. })) => {
            let _ = boot.leave(&mut link);
            return Err(e);
        }
        Err(e) => return Err(e),
    }
    report(progress, Stage::Done, total);
    Ok(Flashed { bytes: image.end(), pages: pages.len() as u32 })
}

/// Resets the board into its bootloader, gets its attention and checks the chip. Nothing is read
/// or written before the chip is known to be the board's.
fn connect(link: &mut Link, board: &Board, timing: &Timing) -> Result<Box<dyn Bootloader>, FlashError> {
    let mut boot: Box<dyn Bootloader> = match board.protocol {
        Protocol::Stk500v1 => Box::new(Stk500v1::new(*timing)),
        Protocol::Stk500v2 => Box::new(Stk500v2::new(*timing)),
    };
    link.drain(timing.quiet)?;
    link.reset()?;
    link.drain(timing.quiet)?;
    if !boot.sync(link)? {
        return Err(FlashError::NoSync { board: board.name });
    }
    boot.enter(link)?;
    let found = boot.signature(link)?;
    if found != board.signature {
        let _ = boot.leave(link);
        return Err(FlashError::wrong_signature(board, found));
    }
    Ok(boot)
}

/// Reads back the pages `image` would be written in, from the `board` on `port`, and compares
/// them with it. It writes nothing, but it resets the board (the sketch restarts) to reach the
/// bootloader, like [`flash()`] does. The progress goes Connecting → Verifying → Done.
pub fn read_back(
    port: &mut dyn Port,
    board: &Board,
    image: &Image,
    timing: &Timing,
    progress: &mut dyn FnMut(Progress),
) -> Result<ReadBack, FlashError> {
    if image.end() > board.room {
        return Err(FlashError::TooLarge { board: board.name, end: image.end(), room: board.room });
    }
    let page = board.page_bytes;
    let pages = image.pages(page);
    let total = pages.len() as u32 * page;
    progress(Progress { stage: Stage::Connecting, done: 0, total });
    let mut link = Link::new(port);
    let mut boot = connect(&mut link, board, timing)?;
    let mut read = Vec::with_capacity(total as usize);
    let mut first_difference = None;
    for (i, &address) in pages.iter().enumerate() {
        let got = match boot.read_page(&mut link, address, page as usize) {
            Ok(got) => got,
            Err(e) => {
                let _ = boot.leave(&mut link);
                return Err(e);
            }
        };
        let wanted = image.page(address, page);
        if first_difference.is_none()
            && let Some(k) = (0..wanted.len()).find(|&k| got.get(k) != Some(&wanted[k]))
        {
            first_difference = Some(address + k as u32);
        }
        read.extend_from_slice(&got);
        progress(Progress { stage: Stage::Verifying, done: (i as u32 + 1) * page, total });
    }
    boot.leave(&mut link)?;
    progress(Progress { stage: Stage::Done, done: total, total });
    Ok(ReadBack { pages: pages.len() as u32, first_difference, read })
}
