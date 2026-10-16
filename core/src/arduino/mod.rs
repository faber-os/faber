//! Layer 2: the Arduino editor's toolchain and its memory of what went on which board
//! (docs/arduino-editor.md).
//!
//! - [`sketch`]: a sketch's files, its name rules and its build id.
//! - [`toolchain`]: compiling through `arduino-cli`, behind the [`Toolchain`] trait so tests use
//!   a fake ([`fake`]); finding an `arduino-cli` that is already on the computer.
//! - [`diagnostics`]: the compiler's errors as file, line and message.
//! - [`install`]: downloading `arduino-cli` from Arduino's releases (checked against their
//!   published SHA-256) through the [`Downloader`] trait, and unpacking it.
//! - [`records`]: `<data>/arduino/boards.toml` and the exact copies of what was flashed.
//! - [`library`]: the FaberLink library and the templates, built into the core.
//!
//! It knows nothing about ports, parts or Files: the machine hands it plain data.

pub mod boards;
pub mod diagnostics;
pub mod fake;
pub mod install;
pub mod library;
pub mod records;
pub mod sketch;
pub mod toolchain;

pub use boards::fqbn;
pub use diagnostics::{Diagnostic, Severity};
pub use install::{Downloader, HttpDownloader, InstallError, Release};
pub use records::{BoardIdentity, Record, Records};
pub use sketch::Sources;
pub use toolchain::{CompileJob, Compiled, Finder, Size, SystemFinder, ToolError, Toolchain, ToolchainInfo};
