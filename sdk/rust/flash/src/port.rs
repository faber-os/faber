//! The serial port, the reset line and the clock, as the flasher sees them.

use crate::FlashError;
use std::io;
use std::time::Duration;

/// A serial port to a board, already open at the board's bootloader speed.
pub trait Port {
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// What has arrived. `Ok(0)` means nothing yet; an implementation waits at most ~50 ms.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// Sets DTR and RTS together: `true` asserts them. An Arduino's USB chip turns asserting DTR
    /// into a short pulse on the reset pin, which starts the bootloader. An error with kind
    /// `Unsupported` means the port has no such lines; the flasher then carries on, and the
    /// user presses reset by hand.
    fn set_dtr_rts(&mut self, asserted: bool) -> io::Result<()>;
    fn sleep(&mut self, d: Duration);
    /// A monotonic clock.
    fn now(&self) -> Duration;
}

/// Reads with a deadline, keeping what came early for the next read.
pub(crate) struct Link<'a> {
    pub port: &'a mut dyn Port,
    pending: Vec<u8>,
}

/// Nothing (or not enough) arrived in time.
#[derive(Debug)]
pub(crate) struct TimedOut;

pub(crate) enum Recv {
    Byte(u8),
    TimedOut,
}

impl<'a> Link<'a> {
    pub fn new(port: &'a mut dyn Port) -> Link<'a> {
        Link { port, pending: Vec::new() }
    }

    pub fn send(&mut self, data: &[u8]) -> Result<(), FlashError> {
        self.port.write(data).map_err(FlashError::port)
    }

    /// One byte, or `TimedOut` after `timeout`.
    pub fn byte(&mut self, timeout: Duration) -> Result<Recv, FlashError> {
        if !self.pending.is_empty() {
            return Ok(Recv::Byte(self.pending.remove(0)));
        }
        let start = self.port.now();
        let mut buf = [0u8; 512];
        loop {
            let n = self.port.read(&mut buf).map_err(FlashError::port)?;
            if n > 0 {
                self.pending.extend_from_slice(&buf[..n]);
                return Ok(Recv::Byte(self.pending.remove(0)));
            }
            if self.port.now().saturating_sub(start) >= timeout {
                return Ok(Recv::TimedOut);
            }
        }
    }

    /// Exactly `n` bytes, each within `timeout` of the one before.
    pub fn exact(&mut self, n: usize, timeout: Duration) -> Result<Result<Vec<u8>, TimedOut>, FlashError> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            match self.byte(timeout)? {
                Recv::Byte(b) => out.push(b),
                Recv::TimedOut => return Ok(Err(TimedOut)),
            }
        }
        Ok(Ok(out))
    }

    /// Throws away whatever arrives until the line has been quiet for `quiet`: the sketch's
    /// output before the reset, or the rest of a damaged answer.
    pub fn drain(&mut self, quiet: Duration) -> Result<(), FlashError> {
        self.pending.clear();
        let mut buf = [0u8; 512];
        let mut since = self.port.now();
        // A sketch that never stops talking must not keep the flasher here forever.
        let give_up = since + quiet * 20;
        loop {
            let n = self.port.read(&mut buf).map_err(FlashError::port)?;
            let now = self.port.now();
            if n > 0 {
                since = now;
            } else if now.saturating_sub(since) >= quiet {
                return Ok(());
            }
            if now >= give_up {
                return Ok(());
            }
        }
    }

    /// The reset dance avrdude does for an Arduino: release DTR/RTS for 250 ms, assert them
    /// (the reset pulse), then give the bootloader 50 ms to start.
    pub fn reset(&mut self) -> Result<(), FlashError> {
        for (level, wait) in [(false, 250), (true, 50)] {
            match self.port.set_dtr_rts(level) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Unsupported => return Ok(()),
                Err(e) => return Err(FlashError::port(e)),
            }
            self.port.sleep(Duration::from_millis(wait));
        }
        Ok(())
    }
}
