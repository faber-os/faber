//! Layer 3: the declarative connector engine (docs/packages.md §6). A declarative connector is
//! only a `faber.toml`; this engine runs it as an ordinary connector.
//!
//! First cut: serial devices that send text lines, read with typed patterns or the built-in NMEA
//! reader. Register maps for I²C/SPI follow when the buses arrive (Phase 6).

pub mod lines;
pub mod nmea;
pub mod pattern;
pub mod values;
