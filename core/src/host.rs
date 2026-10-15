//! Layer 2: facts about the computer Faber runs on: its time zone, its operating system and the
//! space on its disks. Read-only; nothing here changes the computer.

use std::fmt::Display;
use std::io;
use std::path::Path;

/// The machine's IANA time zone, e.g. `Europe/Copenhagen` (D42). `UTC` if the OS doesn't say.
pub fn time_zone() -> String {
    // jiff (already used for zone checks and backup names) reads the system zone on every OS,
    // mapping Windows zone names to IANA ones.
    zone_or_utc(jiff::tz::TimeZone::try_system().and_then(|tz| {
        tz.iana_name()
            .map(str::to_owned)
            .ok_or_else(|| jiff::Error::from_args(format_args!("the system zone has no IANA name")))
    }))
}

fn zone_or_utc<E: Display>(found: Result<String, E>) -> String {
    match found {
        Ok(zone) if !zone.trim().is_empty() => zone,
        Ok(_) => {
            tracing::warn!(target: "core::host", "The computer gave an empty time zone; using UTC.");
            "UTC".into()
        }
        Err(e) => {
            tracing::warn!(target: "core::host", "Can't read the computer's time zone ({e}); using UTC.");
            "UTC".into()
        }
    }
}

/// The operating system in words, e.g. `Debian GNU/Linux 13 (trixie) (aarch64)` or
/// `windows (x86_64)`.
pub fn os() -> String {
    let name = os_release_name().unwrap_or_else(|| std::env::consts::OS.to_string());
    format!("{name} ({})", std::env::consts::ARCH)
}

/// Linux says which distribution it is in /etc/os-release.
fn os_release_name() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    pretty_name(&text)
}

fn pretty_name(os_release: &str) -> Option<String> {
    os_release
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
}

/// Space on the disk that holds `path`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskSpace {
    /// Bytes Faber can still use.
    pub free: u64,
    pub total: u64,
}

pub fn disk_space(path: &Path) -> io::Result<DiskSpace> {
    let stats = fs4::statvfs(path)?;
    Ok(DiskSpace { free: stats.available_space(), total: stats.total_space() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_machines_own_zone_is_a_real_iana_zone() {
        let zone = time_zone();
        assert!(jiff::tz::TimeZone::get(&zone).is_ok(), "{zone}");
    }

    #[test]
    fn a_time_zone_the_os_cant_give_becomes_utc() {
        assert_eq!(zone_or_utc::<String>(Ok("Europe/Copenhagen".into())), "Europe/Copenhagen");
        assert_eq!(zone_or_utc::<String>(Ok("  ".into())), "UTC");
        assert_eq!(zone_or_utc(Err("no zone")), "UTC");
        assert!(!time_zone().is_empty());
    }

    #[test]
    fn the_os_has_a_name_and_an_architecture() {
        assert!(os().ends_with(&format!("({})", std::env::consts::ARCH)));
        let debian = "NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\n";
        assert_eq!(pretty_name(debian).as_deref(), Some("Debian GNU/Linux 13 (trixie)"));
        assert_eq!(pretty_name("ID=x\n"), None);
    }

    #[test]
    fn disk_space_is_measured() {
        let dir = tempfile::tempdir().unwrap();
        let space = disk_space(dir.path()).unwrap();
        assert!(space.total > 0);
        assert!(space.free <= space.total);
    }
}
