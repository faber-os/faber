//! Machine types → wire types (docs/api.md). The only place that knows both.

use super::dto;
use crate::files::{Entry, Kind, Root, Special, VPath};
use crate::logs;
use crate::machine::{MachineError, PartView, PortView};
use crate::ports::PortKind;
use crate::runtime::PartStatus;
use faber_connector::{Manifest, PortRequirement, Problem};

pub fn error_body(e: &MachineError) -> dto::ErrorBody {
    dto::ErrorBody { code: e.code.to_string(), message: e.message.clone(), hint: e.hint.clone() }
}

fn state(kind: dto::PartStateKind, p: Option<&Problem>) -> dto::PartState {
    dto::PartState { kind, message: p.map(|p| p.message.clone()), hint: p.map(|p| p.hint.clone()) }
}

pub fn part_state(s: &PartStatus) -> dto::PartState {
    use dto::PartStateKind as K;
    match s {
        PartStatus::Invalid(p) => state(K::Invalid, Some(p)),
        PartStatus::Conflict(p) => state(K::Conflict, Some(p)),
        PartStatus::Starting => state(K::Starting, None),
        PartStatus::Running => state(K::Running, None),
        PartStatus::Degraded(p) => state(K::Degraded, Some(p)),
        PartStatus::Failed(p) => state(K::Failed, Some(p)),
        PartStatus::Stopped(p) => state(K::Stopped, Some(p)),
    }
}

pub fn part(p: &PartView) -> dto::PartInfo {
    dto::PartInfo {
        name: p.name.to_string(),
        connector: p.connector.clone(),
        port: p.port.clone(),
        state: part_state(&p.status),
        signals: p
            .signals
            .iter()
            .map(|(addr, cap)| dto::SignalInfo { address: addr.to_string(), capability: *cap })
            .collect(),
        mount: p.mount,
    }
}

pub fn port(p: &PortView) -> dto::PortInfo {
    dto::PortInfo {
        id: p.entry.id.to_string(),
        kind: match p.entry.kind {
            PortKind::UsbSerial => dto::PortKind::UsbSerial,
            PortKind::Serial => dto::PortKind::Serial,
            PortKind::Bluetooth => dto::PortKind::Bluetooth,
            PortKind::Replay => dto::PortKind::Replay,
        },
        label: p.entry.label.clone(),
        usb: p.entry.usb.as_ref().map(|u| dto::UsbInfo {
            vid: u.vid,
            pid: u.pid,
            serial_number: u.serial_number.clone(),
            manufacturer: u.manufacturer.clone(),
            product: u.product.clone(),
        }),
        claimed_by: p.claimed_by.clone(),
        suggestions: p
            .suggestions
            .iter()
            .map(|s| dto::Suggestion { connector: s.connector.clone(), reason: s.reason.clone() })
            .collect(),
    }
}

pub fn connector(m: &Manifest) -> dto::ConnectorInfo {
    dto::ConnectorInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        kind: "built-in".into(),
        port: match m.port {
            PortRequirement::Serial { baud } => dto::PortRequirementInfo::Serial { baud },
        },
        provides: m
            .provides
            .iter()
            .map(|p| dto::ProvideInfo { name: p.name.clone(), capability: p.capability })
            .collect(),
    }
}

pub fn file_root(p: &VPath, label: &str, warning: Option<&str>) -> dto::FileRoot {
    let id = match p.root() {
        Root::Home => "home",
        Root::Machine => "machine",
    };
    dto::FileRoot {
        id: id.into(),
        label: label.into(),
        path: p.to_string(),
        warning: warning.map(str::to_string),
    }
}

pub fn file_entry(e: &Entry) -> dto::FileEntry {
    dto::FileEntry {
        name: e.name.clone(),
        path: e.path.to_string(),
        kind: match e.kind {
            Kind::File => dto::FileKind::File,
            Kind::Folder => dto::FileKind::Folder,
        },
        size: e.size,
        modified_ms: e.modified_ms,
        special: e.special.map(|s| match s {
            Special::Desktop => dto::SpecialFolder::Desktop,
            Special::Documents => dto::SpecialFolder::Documents,
            Special::Recordings => dto::SpecialFolder::Recordings,
            Special::System => dto::SpecialFolder::System,
            Special::Trash => dto::SpecialFolder::Trash,
        }),
        original_path: e.original_path.clone(),
    }
}

/// `path` as given; parsing it again can't fail because listing it succeeded.
pub fn folder_listing(path: &str, entries: &[Entry]) -> dto::FolderListing {
    let parsed = VPath::parse(path).expect("listed paths parse");
    dto::FolderListing {
        path: parsed.to_string(),
        parent: parsed.parent().map(|p| p.to_string()),
        entries: entries.iter().map(file_entry).collect(),
    }
}

pub fn log_entry(e: &logs::Entry) -> dto::LogEntry {
    dto::LogEntry {
        t_us: e.t_us,
        level: match e.level {
            logs::Level::Error => dto::LogLevel::Error,
            logs::Level::Warn => dto::LogLevel::Warn,
            logs::Level::Info => dto::LogLevel::Info,
            logs::Level::Debug => dto::LogLevel::Debug,
        },
        source: e.source.clone(),
        message: e.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_become_message_and_hint() {
        let s = part_state(&PartStatus::Failed(Problem::new("m", "h")));
        assert_eq!(s.kind, dto::PartStateKind::Failed);
        assert_eq!(s.message.as_deref(), Some("m"));
        assert_eq!(s.hint.as_deref(), Some("h"));
        assert_eq!(part_state(&PartStatus::Running).message, None);
    }

    #[test]
    fn connectors_are_described_from_their_manifest() {
        let c = connector(&ldrobot_ld19::manifest());
        assert_eq!(c.id, "ldrobot-ld19");
        assert_eq!(c.port, dto::PortRequirementInfo::Serial { baud: 230_400 });
        assert_eq!(c.provides[0].name, "laser-scan");
    }
}
