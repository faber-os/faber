//! Layer 2: removable disks (USB sticks, SD cards) that Files shows as their own places.
//!
//! Discovery only: which disks are connected, and where they are mounted. The files module
//! does the safe path handling; the machine passes the list across on every port scan.

use std::path::PathBuf;

/// A connected removable disk.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Disk {
    /// Stable id used in paths: `disk-<name>`, e.g. `disk-e` (`disk-e:/photos`).
    pub id: String,
    /// What the user sees, e.g. "SENNEP SD (E:)".
    pub label: String,
    pub mount: PathBuf,
}

/// Lists removable disks. A trait so tests can plug disks in and out.
pub trait DiskDiscovery: Send + Sync {
    fn list(&self) -> Vec<Disk>;
}

/// The real OS discovery.
pub struct SystemDisks;

impl DiskDiscovery for SystemDisks {
    fn list(&self) -> Vec<Disk> {
        let mut disks = platform::list();
        disks.sort();
        disks
    }
}

/// `disk-` plus the name in lowercase letters, digits and single dashes: "E:" → `disk-e`,
/// "SD CARD" → `disk-sd-card`. `None` when nothing usable is left.
pub fn disk_id(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    (!out.is_empty() && out.len() <= 32).then(|| format!("disk-{out}"))
}

/// Linux: removable media is mounted under `/media/<user>/<label>` or `/run/media/<user>/<label>`
/// by the desktop's automounter. `/proc/mounts` escapes spaces as `\040`.
pub fn parse_proc_mounts(text: &str) -> Vec<Disk> {
    text.lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(|m| m.replace("\\040", " "))
        .filter(|m| m.starts_with("/media/") || m.starts_with("/run/media/"))
        .filter_map(|m| {
            let label = m.rsplit('/').next()?.to_string();
            Some(Disk { id: disk_id(&label)?, label, mount: PathBuf::from(m) })
        })
        .collect()
}

/// Windows: one WMI query for logical disks of type 2 ("removable"): USB sticks and SD cards.
/// External hard drives report as fixed disks (type 3) and aren't listed; that's deliberate, as
/// they look like internal drives to Windows too. Same safe-WMI reasoning as `ports::usb_problems`.
#[cfg(windows)]
mod platform {
    use super::{Disk, disk_id};
    use serde::Deserialize;
    use std::cell::RefCell;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        #[serde(rename = "DeviceID")]
        device_id: String,
        volume_name: Option<String>,
    }

    const QUERY: &str = "SELECT DeviceID, VolumeName FROM Win32_LogicalDisk WHERE DriveType = 2";

    thread_local! {
        static CONNECTION: RefCell<Option<wmi::WMIConnection>> = const { RefCell::new(None) };
    }

    pub fn list() -> Vec<Disk> {
        let rows: wmi::WMIResult<Vec<Row>> = CONNECTION.with(|cell| {
            let mut cell = cell.borrow_mut();
            let conn = match cell.as_ref() {
                Some(c) => c,
                None => cell.insert(wmi::WMIConnection::new()?),
            };
            conn.raw_query(QUERY)
        });
        match rows {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|r| {
                    let id = disk_id(&r.device_id)?;
                    let label = match r.volume_name.filter(|v| !v.trim().is_empty()) {
                        Some(v) => format!("{v} ({})", r.device_id),
                        None => format!("Removable disk ({})", r.device_id),
                    };
                    Some(Disk { id, label, mount: PathBuf::from(format!("{}\\", r.device_id)) })
                })
                .collect(),
            Err(e) => {
                tracing::warn!(target: "core::files", "could not ask Windows for removable disks: {e}");
                CONNECTION.with(|c| c.borrow_mut().take());
                Vec::new()
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Disk, parse_proc_mounts};

    pub fn list() -> Vec<Disk> {
        std::fs::read_to_string("/proc/mounts").map(|t| parse_proc_mounts(&t)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_safe_path_roots() {
        assert_eq!(disk_id("E:").as_deref(), Some("disk-e"));
        assert_eq!(disk_id("SD CARD").as_deref(), Some("disk-sd-card"));
        assert_eq!(disk_id("  My--Stick!! ").as_deref(), Some("disk-my-stick"));
        assert_eq!(disk_id("***"), None);
    }

    #[test]
    fn linux_media_mounts_are_found() {
        let mounts = "/dev/mmcblk0p2 / ext4 rw 0 0\n\
                      /dev/sda1 /media/mikkel/SENNEP\\040SD vfat rw 0 0\n\
                      /dev/sdb1 /run/media/pi/STICK exfat rw 0 0\n\
                      tmpfs /run tmpfs rw 0 0\n";
        let disks = parse_proc_mounts(mounts);
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].id, "disk-sennep-sd");
        assert_eq!(disks[0].label, "SENNEP SD");
        assert_eq!(disks[0].mount, PathBuf::from("/media/mikkel/SENNEP SD"));
        assert_eq!(disks[1].id, "disk-stick");
    }

    #[cfg(windows)]
    #[test]
    fn the_real_windows_query_runs() {
        // No assumption about what is plugged in; it must just not fail or panic.
        let _ = SystemDisks.list();
    }
}
