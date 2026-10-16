//! A stand-in for `arduino-cli`: the core's tests, and the end-to-end tests through
//! `faber-core --fake-arduino-cli`. It never runs a program.
//!
//! "Compiling" a sketch gives a small, made-up program that depends on every byte of the
//! sketch and on the build id, so another sketch gives another program. A line with `#error` in
//! a sketch file fails the compile with that line, in avr-gcc's words, so the real error reader
//! is exercised too.

use super::sketch::Sources;
use super::toolchain::{CompileJob, Compiled, Finder, Size, Source, ToolError, Toolchain, ToolchainInfo};
use faber_flash::Image;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct FakeToolchain {
    avr_core: AtomicBool,
    compiles: AtomicUsize,
    /// The last job, for tests.
    pub last_job: Mutex<Option<CompileJob>>,
}

impl FakeToolchain {
    /// A toolchain with the AVR core installed (`true`) or still missing.
    pub fn new(avr_core: bool) -> Arc<FakeToolchain> {
        Arc::new(FakeToolchain {
            avr_core: AtomicBool::new(avr_core),
            compiles: AtomicUsize::new(0),
            last_job: Mutex::new(None),
        })
    }

    pub fn compiles(&self) -> usize {
        self.compiles.load(Ordering::SeqCst)
    }

    /// The program a sketch compiles to: 600–1100 bytes, from its hash and build id.
    pub fn program(sources: &Sources, build_id: u64) -> Image {
        let mut h = Sha256::new();
        for (name, data) in &sources.0 {
            h.update(name.as_bytes());
            h.update(data);
        }
        h.update(build_id.to_le_bytes());
        let seed: [u8; 32] = h.finalize().into();
        let len = 600 + (u16::from_le_bytes([seed[0], seed[1]]) % 500) as usize;
        Image::from_bytes(&(0..len).map(|i| seed[i % 32].wrapping_add((i / 32) as u8)).collect::<Vec<_>>())
    }
}

impl Toolchain for FakeToolchain {
    fn info(&self) -> ToolchainInfo {
        ToolchainInfo {
            path: PathBuf::from("fake-arduino-cli"),
            source: Source::Fake,
            version: Some("fake".into()),
        }
    }

    fn has_avr_core(&self) -> Result<bool, ToolError> {
        Ok(self.avr_core.load(Ordering::SeqCst))
    }

    fn install_avr_core(&self, line: &mut dyn FnMut(&str)) -> Result<(), ToolError> {
        line("Downloading packages...");
        line("arduino:avr@1.8.8 installed");
        self.avr_core.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn compile(&self, job: &CompileJob) -> Result<Compiled, ToolError> {
        self.compiles.fetch_add(1, Ordering::SeqCst);
        *self.last_job.lock().expect("fake lock") = Some(job.clone());
        if !self.avr_core.load(Ordering::SeqCst) {
            return Ok(Compiled {
                success: false,
                hex: None,
                output: String::new(),
                size: None,
                error: Some("Error during build: Platform 'arduino:avr' not found".into()),
            });
        }
        let sources =
            Sources::collect(&job.sketch_dir).map_err(|_| ToolError("can't read the sketch".into()))?;
        let mut output = String::new();
        for (name, data) in &sources.0 {
            for (n, line) in String::from_utf8_lossy(data).lines().enumerate() {
                if let Some(at) = line.find("#error") {
                    let path = job.sketch_dir.join(name);
                    output.push_str(&format!(
                        "{}:{}:{}: error: {}\n",
                        path.display(),
                        n + 1,
                        at + 2,
                        line[at..].trim()
                    ));
                }
            }
        }
        // The Arduino core warns about itself on every build; the reader must leave it out.
        let noise = "C:\\Arduino15\\cores\\arduino\\new.cpp:59:60: warning: unused parameter 'tag'\n";
        if !output.is_empty() {
            return Ok(Compiled {
                success: false,
                hex: None,
                output: format!("{noise}{output}"),
                size: None,
                error: Some("Error during build: exit status 1".into()),
            });
        }
        let image = Self::program(&sources, job.build_id);
        let room = match job.fqbn.as_str() {
            f if f.starts_with("arduino:avr:mega") => (253_952, 8192),
            "arduino:avr:nano:cpu=atmega328old" => (30_720, 2048),
            _ => (32_256, 2048),
        };
        Ok(Compiled {
            success: true,
            hex: Some(image.to_hex()),
            output: noise.into(),
            size: Some(Size { flash: image.end(), flash_max: room.0, ram: 200, ram_max: room.1 }),
            error: None,
        })
    }
}

/// Always finds the given toolchain (`--fake-arduino-cli`), or finds it only once
/// "Install Arduino tools" has put a program in `<data>/tools/arduino-cli/` (the tests of the
/// install).
pub struct FakeFinder {
    pub toolchain: Arc<FakeToolchain>,
    /// `Some(data dir)`: only found once installed there.
    pub installed_in: Option<PathBuf>,
}

impl Finder for FakeFinder {
    fn find(&self) -> Option<Arc<dyn Toolchain>> {
        if let Some(data) = &self.installed_in
            && !super::toolchain::faber_dir(data).join(super::toolchain::exe_name()).is_file()
        {
            return None;
        }
        Some(Arc::clone(&self.toolchain) as Arc<dyn Toolchain>)
    }
}
