//! Machine types → wire types (docs/api.md). The only place that knows both.

use super::dto;
use crate::config::Simulate;
use crate::files::{Entry, Kind, Special, VPath};
use crate::leases::{EndReason, Lease};
use crate::logs;
use crate::machine::EstopView;
use crate::machine::WorldView;
use crate::machine::{MachineError, PartView, PortView};
use crate::ports::{PortKind, PortProblem};
use crate::runtime::PartStatus;
use crate::simparts::WorldState;
use crate::simulation::Pattern;
use faber_connector::{Manifest, PortRequirement, Problem, SettingKind, SettingSpec};

pub fn error_body(e: &MachineError) -> dto::ErrorBody {
    dto::ErrorBody {
        code: e.code.to_string(),
        message: e.message.clone(),
        hint: e.hint.clone(),
        reason: e.lease_ended.map(lease_end_reason),
    }
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
        port: p.source.port().map(str::to_string),
        simulate: p.source.simulate().map(simulate),
        state: part_state(&p.status),
        signals: p
            .signals
            .iter()
            .map(|s| dto::SignalInfo {
                address: s.address.to_string(),
                capability: s.capability,
                info: s.info.clone(),
            })
            .collect(),
        mount: p.mount,
    }
}

pub fn simulate(s: Simulate) -> dto::Simulate {
    match s {
        Simulate::Manual => dto::Simulate::Manual,
        Simulate::World => dto::Simulate::World,
    }
}

pub fn simulate_from(s: dto::Simulate) -> Simulate {
    match s {
        dto::Simulate::Manual => Simulate::Manual,
        dto::Simulate::World => Simulate::World,
    }
}

/// JSON settings from the wire → a TOML table for parts.toml.
pub fn settings(json: serde_json::Map<String, serde_json::Value>) -> Result<toml::Table, String> {
    toml::Table::try_from(serde_json::Value::Object(json)).map_err(|e| e.to_string())
}

pub fn lease_grant(l: &Lease) -> dto::LeaseGrant {
    dto::LeaseGrant {
        lease: l.id.clone(),
        part: l.part.to_string(),
        holder: l.holder.clone(),
        deadline_us: l.deadline_us,
        duration_ms: l.duration_us / 1000,
        locks: l.locks.iter().map(ToString::to_string).collect(),
    }
}

pub fn lease_info(l: &Lease) -> dto::LeaseInfo {
    dto::LeaseInfo {
        part: l.part.to_string(),
        holder: l.holder.clone(),
        deadline_us: l.deadline_us,
        locks: l.locks.iter().map(ToString::to_string).collect(),
    }
}

pub fn lease_end_reason(r: EndReason) -> dto::LeaseEndReason {
    use dto::LeaseEndReason as R;
    match r {
        EndReason::Released => R::Released,
        EndReason::Expired => R::Expired,
        EndReason::Estop => R::Estop,
        EndReason::TakenOver => R::TakenOver,
        EndReason::PartRemoved => R::PartRemoved,
        EndReason::PartRestarted => R::PartRestarted,
        EndReason::PartFailed => R::PartFailed,
        EndReason::Shutdown => R::Shutdown,
    }
}

pub fn estop_status(v: &EstopView) -> dto::EstopStatus {
    dto::EstopStatus {
        engaged: v.engaged.is_some(),
        engaged_by: v.engaged.clone(),
        history: v.history.clone(),
    }
}

pub fn pattern(p: Pattern) -> dto::Pattern {
    match p {
        Pattern::Steady { value } => dto::Pattern::Steady { value },
        Pattern::Sine { center, amplitude, period_s } => dto::Pattern::Sine { center, amplitude, period_s },
        Pattern::Noise { center, amplitude } => dto::Pattern::Noise { center, amplitude },
        Pattern::Step { low, high, period_s } => dto::Pattern::Step { low, high, period_s },
    }
}

pub fn pattern_from(p: dto::Pattern) -> Pattern {
    match p {
        dto::Pattern::Steady { value } => Pattern::Steady { value },
        dto::Pattern::Sine { center, amplitude, period_s } => Pattern::Sine { center, amplitude, period_s },
        dto::Pattern::Noise { center, amplitude } => Pattern::Noise { center, amplitude },
        dto::Pattern::Step { low, high, period_s } => Pattern::Step { low, high, period_s },
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

pub fn port_problem(p: &PortProblem) -> dto::PortProblem {
    dto::PortProblem {
        code: p.code.to_string(),
        label: p.label.clone(),
        detail: p.detail.clone(),
        hint: p.hint.clone(),
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
            PortRequirement::Bridge => dto::PortRequirementInfo::Bridge,
        },
        provides: m
            .provides
            .iter()
            .map(|p| dto::ProvideInfo {
                name: p.name.clone(),
                capability: p.capability,
                info: p.info.clone(),
            })
            .collect(),
        settings: m.settings.iter().map(setting).collect(),
    }
}

fn setting(s: &SettingSpec) -> dto::SettingInfo {
    use dto::SettingKindInfo as K;
    dto::SettingInfo {
        key: s.key.clone(),
        label: s.label.clone(),
        description: s.description.clone(),
        unit: s.unit.clone(),
        advanced: s.advanced,
        kind: match &s.kind {
            SettingKind::Number(n) => K::Number {
                default: n.default,
                min: n.min,
                min_exclusive: n.min_exclusive,
                max: n.max,
                suggested: n.suggested,
            },
            SettingKind::Bool { default } => K::Bool { default: *default },
            SettingKind::Choice { default, choices } => K::Choice {
                default: default.clone(),
                choices: choices
                    .iter()
                    .map(|c| dto::ChoiceInfo { value: c.value.clone(), label: c.label.clone() })
                    .collect(),
            },
            SettingKind::String { default } => K::String { default: default.clone() },
            SettingKind::PartList => K::PartList,
        },
    }
}

pub fn file_root(p: &VPath, label: &str, warning: Option<&str>) -> dto::FileRoot {
    dto::FileRoot {
        id: p.root_id(),
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
            Special::Backups => dto::SpecialFolder::Backups,
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

pub fn world_view(v: WorldView) -> dto::WorldView {
    dto::WorldView {
        name: v.world.name,
        title: v.world.title,
        description: v.world.description,
        start: v.start,
        geometry: v.world.geometry,
        footprint: v.footprint,
    }
}

pub fn world_message(id: u32, s: WorldState) -> dto::ServerMessage {
    dto::ServerMessage::World {
        id,
        pose: s.pose,
        velocity: s.velocity,
        bumped: s.bumped,
        movers: s
            .movers
            .iter()
            .map(|m| dto::WorldMover { x: m.position.x, y: m.position.y, radius: m.radius })
            .collect(),
        t_us: s.t_us,
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
        assert!(
            matches!(c.provides[0].info, Some(faber_capabilities::Info::LaserScan(_))),
            "the range is shown"
        );
        assert_eq!(c.settings, [], "the LD19 has nothing to set");
    }

    #[test]
    fn connectors_describe_their_settings_on_the_wire() {
        let base = crate::connectors::generic::definitions().remove(0).manifest;
        let json = serde_json::to_value(connector(&base)).unwrap();
        let vx = json["settings"].as_array().unwrap().iter().find(|s| s["key"] == "max-vx").unwrap();
        assert_eq!(
            *vx,
            serde_json::json!({
                "key": "max-vx", "label": "Top speed forward",
                "description": "Faster commands are slowed down to this.", "unit": "m/s",
                "advanced": false, "kind": "number", "default": 0.5, "min": 0.0, "minExclusive": true,
                "suggested": 0.6
            })
        );
        let kinds: Vec<_> = json["settings"].as_array().unwrap().iter().map(|s| s["kind"].clone()).collect();
        assert!(kinds.contains(&"bool".into()) && kinds.contains(&"part-list".into()), "{kinds:?}");
        let back: dto::ConnectorInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, connector(&base), "it reads back the same");
    }
}
