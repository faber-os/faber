//! A sketch: a folder `<name>/` with `<name>.ino` and maybe more tabs (`.h`, `.cpp`, …), as the
//! Arduino IDE keeps it. Its files are read once, hashed and compiled from that copy, so what was
//! compiled is exactly what is recorded.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// The extensions of a sketch's tabs. Anything else in the folder (notes, pictures) isn't code.
pub const SOURCE_EXTENSIONS: &[&str] = &["ino", "pde", "h", "hh", "hpp", "c", "cc", "cpp", "cxx", "S"];

/// More than this is not a sketch someone typed: refuse instead of copying it around.
pub const MAX_FILES: usize = 200;
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Why a name can't be a sketch's. Arduino's rule: a letter or digit first, then letters,
/// digits, `_`, `-` and `.`, at most 63 characters.
pub fn check_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("A sketch needs a name.".into());
    };
    if name.len() > 63 {
        return Err("A sketch's name can be at most 63 characters long.".into());
    }
    if !first.is_ascii_alphanumeric() {
        return Err("A sketch's name must start with a letter or a digit.".into());
    }
    if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))) {
        return Err(format!(
            "A sketch's name can only hold letters (a–z), digits, \"_\", \"-\" and \".\", not \"{bad}\"."
        ));
    }
    Ok(())
}

/// Is this file one of a sketch's tabs?
pub fn is_source(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && SOURCE_EXTENSIONS.contains(&ext))
}

/// A sketch's files by their path inside the sketch folder (`Blink.ino`, `src/motor.cpp`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources(pub BTreeMap<String, Vec<u8>>);

/// Why a folder's files can't be used as a sketch.
#[derive(Debug)]
pub enum CollectError {
    Io(io::Error),
    TooBig,
}

impl From<io::Error> for CollectError {
    fn from(e: io::Error) -> Self {
        CollectError::Io(e)
    }
}

impl Sources {
    /// The tabs in `dir`, and everything code-like under its `src/` folder (Arduino compiles that
    /// too). Hidden files and other files are left out.
    pub fn collect(dir: &Path) -> Result<Sources, CollectError> {
        let mut out = Sources::default();
        let mut bytes = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_file() && is_source(&name) {
                out.add(name, &entry.path(), &mut bytes)?;
            } else if kind.is_dir() && name == "src" {
                out.collect_src(&entry.path(), "src", &mut bytes)?;
            }
        }
        Ok(out)
    }

    fn collect_src(&mut self, dir: &Path, prefix: &str, bytes: &mut u64) -> Result<(), CollectError> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let rel = format!("{prefix}/{name}");
            let kind = entry.file_type()?;
            if kind.is_file() && is_source(&name) {
                self.add(rel, &entry.path(), bytes)?;
            } else if kind.is_dir() {
                self.collect_src(&entry.path(), &rel, bytes)?;
            }
        }
        Ok(())
    }

    fn add(&mut self, rel: String, path: &Path, bytes: &mut u64) -> Result<(), CollectError> {
        *bytes += std::fs::metadata(path)?.len();
        if self.0.len() >= MAX_FILES || *bytes > MAX_BYTES {
            return Err(CollectError::TooBig);
        }
        self.0.insert(rel, std::fs::read(path)?);
        Ok(())
    }

    /// Writes every file into `dir` (which must exist), making `src/` folders as needed.
    pub fn write_to(&self, dir: &Path) -> io::Result<()> {
        for (rel, data) in &self.0 {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, data)?;
        }
        Ok(())
    }

    /// SHA-256 over the board and every file (its path, length and bytes, in path order), so
    /// two sketches hash the same only if every file is the same, for the same board.
    pub fn hash(&self, fqbn: &str) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"faber-sketch-v1\0");
        h.update(fqbn.as_bytes());
        h.update([0]);
        for (rel, data) in &self.0 {
            h.update(rel.as_bytes());
            h.update([0]);
            h.update((data.len() as u64).to_le_bytes());
            h.update(data);
        }
        h.finalize().into()
    }

    /// The build id: the first 8 bytes of [`Sources::hash`]. The sketch is compiled with it
    /// (`-DFABER_BUILD_ID`), and a FaberLink board says it in its `HELLO_ACK`.
    pub fn build_id(&self, fqbn: &str) -> u64 {
        let h = self.hash(fqbn);
        u64::from_be_bytes(h[..8].try_into().expect("8 bytes"))
    }
}

/// A build id as Faber writes it: 16 lowercase hex digits.
pub fn build_id_text(id: u64) -> String {
    format!("{id:016x}")
}

/// SHA-256 of some bytes, as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(files: &[(&str, &str)]) -> Sources {
        Sources(files.iter().map(|(n, t)| (n.to_string(), t.as_bytes().to_vec())).collect())
    }

    #[test]
    fn names_follow_arduinos_rules() {
        for ok in ["Blink", "motor_test", "v2.1-final", "9lives"] {
            assert_eq!(check_name(ok), Ok(()), "{ok}");
        }
        for bad in ["", "_hidden", "-x", "my sketch", "æble", "a/b"] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
        assert!(check_name(&"a".repeat(64)).is_err());
        assert!(check_name("my sketch").unwrap_err().contains("not \" \""));
    }

    #[test]
    fn only_code_files_are_tabs() {
        for yes in ["Blink.ino", "motor.h", "motor.cpp", "util.c", "asm.S", "x.hpp"] {
            assert!(is_source(yes), "{yes}");
        }
        for no in ["notes.txt", "wiring.png", ".ino", "Makefile", "x.ino.bak"] {
            assert!(!is_source(no), "{no}");
        }
    }

    #[test]
    fn the_build_id_changes_with_any_file_or_the_board() {
        let a = sources(&[("Blink.ino", "void setup(){}\nvoid loop(){}\n")]);
        let id = a.build_id("arduino:avr:uno");
        assert_eq!(id, a.clone().build_id("arduino:avr:uno"), "the same sketch has the same id");
        assert_ne!(id, a.build_id("arduino:avr:mega:cpu=atmega2560"), "another board, another build");
        let edited = sources(&[("Blink.ino", "void setup(){}\nvoid loop(){ }\n")]);
        assert_ne!(id, edited.build_id("arduino:avr:uno"));
        let with_tab = sources(&[("Blink.ino", "void setup(){}\nvoid loop(){}\n"), ("x.h", "")]);
        assert_ne!(id, with_tab.build_id("arduino:avr:uno"), "an empty extra tab counts");
        // Moving bytes from one file into another's name can't give the same hash.
        let x = sources(&[("ab", "c")]);
        let y = sources(&[("a", "bc")]);
        assert_ne!(x.hash("f"), y.hash("f"));
        assert_eq!(build_id_text(0xAB), "00000000000000ab");
    }

    #[test]
    fn collecting_takes_the_tabs_and_src_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("Robot.ino"), "a").unwrap();
        std::fs::write(d.join("motor.h"), "b").unwrap();
        std::fs::write(d.join("notes.txt"), "not code").unwrap();
        std::fs::write(d.join(".hidden.h"), "no").unwrap();
        std::fs::create_dir_all(d.join("src/drivers")).unwrap();
        std::fs::write(d.join("src/drivers/l298.cpp"), "c").unwrap();
        std::fs::create_dir_all(d.join("build")).unwrap();
        std::fs::write(d.join("build/old.cpp"), "no").unwrap();
        let s = Sources::collect(d).unwrap();
        assert_eq!(s.0.keys().collect::<Vec<_>>(), ["Robot.ino", "motor.h", "src/drivers/l298.cpp"]);
        let copy = tempfile::tempdir().unwrap();
        s.write_to(copy.path()).unwrap();
        assert_eq!(Sources::collect(copy.path()).unwrap(), s);
    }

    #[test]
    fn a_huge_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..=MAX_FILES {
            std::fs::write(dir.path().join(format!("f{i}.h")), "").unwrap();
        }
        assert!(matches!(Sources::collect(dir.path()), Err(CollectError::TooBig)));
    }
}
