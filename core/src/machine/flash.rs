//! The built-in Arduino flasher (docs/api.md, "The Arduino flasher"): puts a program on an Uno,
//! Nano or Mega over its USB port, with `faber-flash` doing the protocol.
//!
//! Flashing replaces the program on the board, so it only ever happens when someone asks
//! (`POST /api/v0/ports/{id}/flash`), never by itself, and it writes nothing before the
//! bootloader has answered and the chip is the board's. It refuses a port a part uses; a
//! FaberLink board (a bridge) on the port is stopped only when the request says `stopPart`. The
//! flasher holds the port's claim the whole time, so nothing else opens it; the bridge waits
//! (its port "is open in the arduino flasher") and starts again by itself afterwards, on the new
//! firmware.

use super::{ErrorKind, Machine, MachineError, port_claimed};
use crate::config::BRIDGE_CONNECTOR;
use crate::leases::EndReason;
use crate::names::PartName;
use crate::packages::Store;
use crate::ports::{Claimant, PortEntry, PortId, PortKind, PortReader, PortWriter};
use faber_flash::{Board, Detected, FlashError, Image, Progress};
use std::io;
use std::time::{Duration, Instant};

/// The flasher's claim on a port: `tool:arduino-flasher`.
pub const FLASHER: &str = "arduino-flasher";

/// What the port is taken for, so a refusal says what would happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Use {
    Flash,
    ReadBack,
}

/// Where the program comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirmwareSource {
    /// A `.hex` file in Files, e.g. `home:/Documents/blink.hex`.
    File(String),
    /// An installed package's `firmware/<board>.hex`.
    Package(String),
    /// A program the Arduino editor just compiled (docs/arduino-editor.md).
    Compiled(Image),
}

/// A request to flash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashRequest {
    pub port: String,
    /// A board id (`uno`, `mega2560`, …). `None`: the one the port's USB ids name.
    pub board: Option<String>,
    pub firmware: FirmwareSource,
    /// Stop the FaberLink board (bridge) that uses the port. Without it, such a port is refused.
    pub stop_part: bool,
}

/// What was flashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flashed {
    pub port: String,
    pub board: &'static Board,
    /// Where the program ends.
    pub bytes: u32,
    pub pages: u32,
    /// The bridge that was stopped for it, and starts again by itself.
    pub stopped: Option<PartName>,
}

/// Progress, for the `flash` topic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashProgress {
    pub port: String,
    pub progress: Progress,
}

fn bad_request(code: &'static str, message: String, hint: impl Into<String>) -> MachineError {
    MachineError::new(ErrorKind::BadRequest, code, message, hint)
}

pub(super) fn from_flash(e: &FlashError) -> MachineError {
    let kind = match e {
        FlashError::Port(_) => ErrorKind::Conflict,
        _ => ErrorKind::Unprocessable,
    };
    MachineError::new(kind, e.code(), e.message(), e.hint())
}

fn board_list() -> String {
    faber_flash::BOARDS.iter().map(|b| format!("{} ({})", b.id, b.name)).collect::<Vec<_>>().join(", ")
}

/// The board the request names, or else the one the port's USB ids name.
fn pick_board(requested: Option<&str>, port: &PortEntry) -> Result<&'static Board, MachineError> {
    if let Some(id) = requested {
        return Board::by_id(id).ok_or_else(|| {
            bad_request(
                "unknown-board",
                format!("The flasher doesn't know the board \"{id}\"."),
                format!("Pick one of: {}.", board_list()),
            )
        });
    }
    match guess(port) {
        Detected::Board(b) => Ok(b),
        Detected::UsbSerial { chip } => Err(bad_request(
            "board-needed",
            format!(
                "{} has a {chip} USB chip, which many clone boards use, so Faber can't tell which board it is.",
                port.label
            ),
            "Pick the board.",
        )),
        Detected::Unknown => Err(bad_request(
            "board-needed",
            format!("Faber can't tell which board is on {}.", port.label),
            "Pick the board.",
        )),
    }
}

/// What a port's USB ids say about the board on it.
pub fn guess(port: &PortEntry) -> Detected {
    port.usb.as_ref().map_or(Detected::Unknown, |u| faber_flash::detect(u.vid, u.pid))
}

/// The serial port, as `faber-flash` sees it: the port manager's reader and writer, and the
/// real clock (the bootloader keeps real time, whatever the core's clock does).
pub(super) struct FlashPort {
    pub(super) reader: Box<dyn PortReader>,
    pub(super) writer: Box<dyn PortWriter>,
    pub(super) start: Instant,
}

impl faber_flash::Port for FlashPort {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write(data)
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.reader.read(buf)?;
        if n == 0 {
            // A reader that never blocks (a test's) must not spin.
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(n)
    }

    fn set_dtr_rts(&mut self, asserted: bool) -> io::Result<()> {
        self.writer.set_dtr_rts(asserted)
    }

    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Machine {
    /// Progress of every flash, for the `flash` topic.
    pub fn flash_events(&self) -> tokio::sync::broadcast::Receiver<FlashProgress> {
        self.flash_events.subscribe()
    }

    /// Puts a program on the board on a port (`POST /api/v0/ports/{id}/flash`). Takes as long
    /// as the board needs: seconds for an Uno, up to a minute for a full Mega.
    pub fn flash_port(&self, req: FlashRequest) -> Result<Flashed, MachineError> {
        let entry = {
            let st = self.state.lock();
            PortId::parse(&req.port).and_then(|id| st.ports.iter().find(|p| p.id == id).cloned()).ok_or_else(
                || {
                    bad_request(
                        "unknown-port",
                        format!("There is no port called \"{}\" right now.", req.port),
                        "Check that the board is plugged in, then pick it from the list of ports.",
                    )
                },
            )?
        };
        if entry.kind == PortKind::Replay {
            return Err(bad_request(
                "not-a-serial-port",
                format!("{} is a recording, so there is no board on it to flash.", entry.label),
                "Pick the port the board is plugged into.",
            ));
        }
        let board = pick_board(req.board.as_deref(), &entry)?;
        let text = match &req.firmware {
            FirmwareSource::File(path) => {
                let (_, bytes) = self.read_file(path)?;
                String::from_utf8(bytes)
                    .map_err(|_| from_flash(&FlashError::BadHex(faber_flash::HexError::NotHex { line: 1 })))?
            }
            FirmwareSource::Package(id) => Store::new(&self.data_dir).firmware(id, board.id)?,
            FirmwareSource::Compiled(image) => image.to_hex(),
        };
        let image = Image::parse(&text).map_err(|e| from_flash(&FlashError::BadHex(e)))?;
        if image.end() > board.room {
            return Err(from_flash(&FlashError::TooLarge {
                board: board.name,
                end: image.end(),
                room: board.room,
            }));
        }

        let me = Claimant::Tool(FLASHER);
        let stopped = self.claim_for_flashing(&entry, &me, req.stop_part, Use::Flash)?;
        let result = match self.opener.open(&entry, board.baud) {
            Ok((reader, writer)) => {
                let mut port = FlashPort { reader, writer, start: Instant::now() };
                tracing::info!(target: "core::flash", "flashing {} as {} ({} bytes)", entry.label, board.name, image.end());
                let id = entry.id.to_string();
                let events = self.flash_events.clone();
                faber_flash::flash(&mut port, board, &image, &faber_flash::Timing::default(), &mut |p| {
                    let _ = events.send(FlashProgress { port: id.clone(), progress: p });
                })
                .map_err(|e| from_flash(&e))
            }
            Err(e) => Err(MachineError::new(
                ErrorKind::Conflict,
                "port-unavailable",
                format!("Could not open {}: {e}", entry.label),
                "Check that no other program (for example the Arduino IDE) uses it.",
            )),
        };
        {
            let mut st = self.state.lock();
            st.claims.release(&entry.id, &me);
            self.ports_changed.send_modify(|n| *n += 1);
            self.reconcile(&mut st); // the bridge stopped for this starts again, on the new firmware
        }
        match &result {
            Ok(f) => {
                tracing::info!(target: "core::flash", "flashed {} as {}: {} pages", entry.label, board.name, f.pages)
            }
            Err(e) => {
                tracing::warn!(target: "core::flash", "flashing {} failed: {} {}", entry.label, e.message, e.hint)
            }
        }
        let f = result?;
        Ok(Flashed { port: entry.id.to_string(), board, bytes: f.bytes, pages: f.pages, stopped })
    }

    /// Takes the port for the flasher (or for reading a program back). A part on it is refused,
    /// except a bridge when `stop_part` says so: it is stopped (its leases end, its motors get
    /// `SAFE`) and waits for the port.
    pub(super) fn claim_for_flashing(
        &self,
        entry: &PortEntry,
        me: &Claimant,
        stop_part: bool,
        why: Use,
    ) -> Result<Option<PartName>, MachineError> {
        let mut st = self.state.lock();
        let mut stopped = None;
        match st.claims.holder(&entry.id).cloned() {
            None => {}
            Some(Claimant::Part(name)) => {
                let bridge = st.config.parts.get(&name).is_some_and(|c| c.connector == BRIDGE_CONNECTOR);
                if !bridge {
                    return Err(port_claimed(entry, &name.to_string()));
                }
                if !stop_part {
                    return Err(MachineError::new(
                        ErrorKind::Conflict,
                        "port-used-by-bridge",
                        format!("{} is used by the FaberLink board \"{name}\".", entry.label),
                        match why {
                            Use::Flash => format!(
                                "Flashing stops \"{name}\" (its motors stop) and it starts again on the new firmware. Ask again with stopPart to go ahead."
                            ),
                            Use::ReadBack => format!(
                                "Checking the code stops \"{name}\" (its motors stop) and resets the board; it starts again afterwards. Ask again with stopPart to go ahead."
                            ),
                        },
                    ));
                }
                tracing::info!(target: "core::flash", part = %name, "stopping the board's bridge to flash it");
                self.end_part(&mut st, &name, EndReason::PartRestarted);
                self.stop_part(&mut st, &name);
                stopped = Some(name);
            }
            Some(holder) => return Err(port_claimed(entry, &holder.to_string())),
        }
        st.claims.claim(&entry.id, me.clone()).expect("the port is free now");
        self.ports_changed.send_modify(|n| *n += 1);
        // The stopped bridge tries its port again, finds the flasher on it, and says so.
        self.reconcile(&mut st);
        Ok(stopped)
    }
}
