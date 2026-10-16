//! What Faber flashed on each board: `<data>/arduino/boards.toml`, plus an exact copy of each
//! board's sketch and program in `<data>/arduino/flashed/<id>/` (docs/arduino-editor.md).
//!
//! The copy is taken from what was compiled, not from Files, so editing the sketch afterwards
//! changes nothing here. A record alone never says what a board runs now: the machine proves it
//! with the board's build id or by reading the program back.

use super::sketch::Sources;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "boards.toml";
const HEADER: &str = "# What Faber's Arduino editor flashed on each board (docs/arduino-editor.md).\n\
                      # Faber rewrites this file after every upload; the copies are in flashed/.\n\n";

/// Which board it is, as well as its USB port can tell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BoardIdentity {
    /// The board's own USB serial number. Official Arduinos have one; boards with a CH340 or
    /// many other cheap USB chips don't, and then the board is only known by its port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
    /// The port id it was on (`usb:2341:0043`).
    pub port: String,
}

impl BoardIdentity {
    /// `serial` is what the OS reports. Windows makes one up for a USB chip without one (from
    /// where it is plugged in, e.g. `5&1A2B3C&0&2`); that isn't the board's, so it isn't used.
    pub fn new(port: &str, usb: Option<(u16, u16, Option<&str>)>) -> BoardIdentity {
        let (vid, pid, serial) = match usb {
            Some((v, p, s)) => (Some(v), Some(p), s),
            None => (None, None, None),
        };
        let serial = serial.map(str::trim).filter(|s| !s.is_empty() && !s.contains('&')).map(str::to_string);
        BoardIdentity { serial, vid, pid, port: port.to_string() }
    }

    /// True when the board has its own serial number. Without one, an identical board on the
    /// same port can't be told apart from it.
    pub fn unique(&self) -> bool {
        self.serial.is_some()
    }

    /// The key two identities are the same board by.
    fn key(&self) -> String {
        match (&self.serial, self.vid, self.pid) {
            (Some(s), Some(v), Some(p)) => format!("usb-{v:04x}-{p:04x}-{s}"),
            (Some(s), _, _) => format!("serial-{s}"),
            (None, Some(v), Some(p)) => format!("port-{}-{v:04x}-{p:04x}", self.port),
            (None, _, _) => format!("port-{}", self.port),
        }
    }

    pub fn same_board(&self, other: &BoardIdentity) -> bool {
        self.key() == other.key()
    }

    /// A folder name for the record: the key with anything odd made `_`.
    fn folder(&self) -> String {
        self.key().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
    }
}

/// One board's last flash from Faber.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Record {
    /// Also its folder in `flashed/`.
    pub id: String,
    pub identity: BoardIdentity,
    /// The flasher's board id (`uno`, `mega2560`, …).
    pub board: String,
    /// Where the sketch was in Files (`home:/Arduino/Blink/Blink.ino`).
    pub sketch: String,
    pub sketch_name: String,
    /// 16 hex digits, as compiled into the sketch.
    pub build_id: String,
    /// SHA-256 of the programmed bytes: every page written, as written (docs/arduino-editor.md).
    pub sha256: String,
    /// Where the program ends.
    pub bytes: u32,
    /// RFC 3339, UTC.
    pub flashed_at: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "board")]
    boards: Vec<Record>,
}

/// The records, kept in memory and written through.
pub struct Records {
    dir: PathBuf,
    list: Vec<Record>,
}

/// What a new record is made from.
pub struct Flash<'a> {
    pub identity: BoardIdentity,
    pub board: &'a str,
    pub sketch: &'a str,
    pub sketch_name: &'a str,
    pub sources: &'a Sources,
    pub hex: &'a str,
    pub build_id: &'a str,
    pub sha256: &'a str,
    pub bytes: u32,
    pub flashed_at: String,
}

impl Records {
    /// Reads `<arduino dir>/boards.toml`. A file Faber can't read is logged and kept aside as
    /// `boards.toml.broken`, and Faber starts again with no records: it only means it can't
    /// vouch for any board until it flashes it again.
    pub fn open(dir: &Path) -> Records {
        let path = dir.join(FILE);
        let list = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<File>(&text) {
                Ok(f) => f.boards,
                Err(e) => {
                    tracing::warn!(target: "core::arduino", "{} can't be read ({e}); keeping it as boards.toml.broken", path.display());
                    let _ = std::fs::rename(&path, dir.join("boards.toml.broken"));
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        Records { dir: dir.to_path_buf(), list }
    }

    pub fn all(&self) -> &[Record] {
        &self.list
    }

    pub fn get(&self, id: &str) -> Option<&Record> {
        self.list.iter().find(|r| r.id == id)
    }

    /// The last flash on this board.
    pub fn for_board(&self, identity: &BoardIdentity) -> Option<&Record> {
        self.list.iter().find(|r| r.identity.same_board(identity))
    }

    /// A flash of the build with this id, on any board.
    pub fn by_build_id(&self, build_id: &str) -> Option<&Record> {
        self.list.iter().find(|r| r.build_id == build_id)
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.dir.join("flashed").join(id)
    }

    /// The sketch exactly as it was flashed.
    pub fn sources(&self, record: &Record) -> io::Result<Sources> {
        Sources::collect(&self.folder(&record.id).join(&record.sketch_name)).map_err(|e| match e {
            super::sketch::CollectError::Io(e) => e,
            super::sketch::CollectError::TooBig => io::Error::other("the recorded sketch is too big"),
        })
    }

    /// The program exactly as it was flashed (Intel HEX).
    pub fn hex(&self, record: &Record) -> io::Result<String> {
        std::fs::read_to_string(self.folder(&record.id).join("program.hex"))
    }

    /// Records a flash, replacing the board's last one. The copy is written next to the old one
    /// and swapped in, then `boards.toml` is rewritten.
    pub fn record(&mut self, f: Flash) -> io::Result<Record> {
        let id = f.identity.folder();
        let flashed = self.dir.join("flashed");
        let fresh = flashed.join(format!(".{id}.new"));
        let _ = std::fs::remove_dir_all(&fresh);
        std::fs::create_dir_all(fresh.join(f.sketch_name))?;
        f.sources.write_to(&fresh.join(f.sketch_name))?;
        std::fs::write(fresh.join("program.hex"), f.hex)?;
        let target = self.folder(&id);
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&fresh, &target)?;
        let record = Record {
            id: id.clone(),
            identity: f.identity,
            board: f.board.to_string(),
            sketch: f.sketch.to_string(),
            sketch_name: f.sketch_name.to_string(),
            build_id: f.build_id.to_string(),
            sha256: f.sha256.to_string(),
            bytes: f.bytes,
            flashed_at: f.flashed_at,
        };
        self.list.retain(|r| r.id != id);
        self.list.push(record.clone());
        self.save()?;
        Ok(record)
    }

    fn save(&self) -> io::Result<()> {
        let text = toml::to_string(&File { boards: self.list.clone() }).map_err(io::Error::other)?;
        let path = self.dir.join(FILE);
        let tmp = self.dir.join(format!("{FILE}.tmp"));
        std::fs::write(&tmp, format!("{HEADER}{text}"))?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(text: &str) -> Sources {
        Sources([("Blink.ino".to_string(), text.as_bytes().to_vec())].into())
    }

    fn flash<'a>(identity: BoardIdentity, s: &'a Sources, build: &'a str) -> Flash<'a> {
        Flash {
            identity,
            board: "uno",
            sketch: "home:/Arduino/Blink/Blink.ino",
            sketch_name: "Blink",
            sources: s,
            hex: ":00000001FF\n",
            build_id: build,
            sha256: "ab",
            bytes: 128,
            flashed_at: "2026-10-16T12:00:00Z".into(),
        }
    }

    #[test]
    fn official_boards_are_known_by_serial_number_on_any_port() {
        let a = BoardIdentity::new("usb:2341:0043", Some((0x2341, 0x0043, Some("85036313530351A0E1F1"))));
        let b = BoardIdentity::new("serial:COM9", Some((0x2341, 0x0043, Some("85036313530351A0E1F1"))));
        assert!(a.unique() && a.same_board(&b));
        let other = BoardIdentity::new("usb:2341:0043", Some((0x2341, 0x0043, Some("OTHER"))));
        assert!(!a.same_board(&other));
    }

    #[test]
    fn a_clone_without_a_serial_is_known_only_by_its_port() {
        let made_up = BoardIdentity::new("usb:1a86:7523", Some((0x1a86, 0x7523, Some("5&1A2B3C&0&2"))));
        assert_eq!(made_up.serial, None, "Windows' made-up number isn't the board's");
        assert!(!made_up.unique());
        let same_port = BoardIdentity::new("usb:1a86:7523", Some((0x1a86, 0x7523, None)));
        assert!(made_up.same_board(&same_port), "any CH340 board on that port looks the same");
        let other_port = BoardIdentity::new("serial:COM4", Some((0x1a86, 0x7523, None)));
        assert!(!made_up.same_board(&other_port));
    }

    #[test]
    fn a_record_keeps_an_exact_copy_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let id = BoardIdentity::new("usb:2341:0043", Some((0x2341, 0x0043, Some("S1"))));
        let s = sources("v1");
        let mut r = Records::open(dir.path());
        let rec = r.record(flash(id.clone(), &s, "0000000000000001")).unwrap();
        assert_eq!(rec.id, "usb-2341-0043-S1");
        let again = Records::open(dir.path());
        assert_eq!(again.for_board(&id), Some(&rec));
        assert_eq!(again.sources(&rec).unwrap(), s);
        assert_eq!(again.hex(&rec).unwrap(), ":00000001FF\n");
        assert_eq!(again.by_build_id("0000000000000001").map(|r| r.id.as_str()), Some("usb-2341-0043-S1"));
        let text = std::fs::read_to_string(dir.path().join("boards.toml")).unwrap();
        assert!(text.starts_with("# What Faber"), "{text}");
    }

    #[test]
    fn flashing_again_replaces_the_boards_record_and_copy() {
        let dir = tempfile::tempdir().unwrap();
        let id = BoardIdentity::new("usb:2341:0043", Some((0x2341, 0x0043, Some("S1"))));
        let mut r = Records::open(dir.path());
        let (v1, v2) = (sources("v1"), sources("v2"));
        r.record(flash(id.clone(), &v1, "01")).unwrap();
        let rec = r.record(flash(id.clone(), &v2, "02")).unwrap();
        assert_eq!(r.all().len(), 1);
        assert_eq!(r.sources(&rec).unwrap(), v2);
        assert_eq!(r.by_build_id("01"), None);
    }

    #[test]
    fn a_broken_file_is_kept_aside_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("boards.toml"), "this is [not toml").unwrap();
        let r = Records::open(dir.path());
        assert!(r.all().is_empty());
        assert!(dir.path().join("boards.toml.broken").exists());
    }
}
