//! Machine types → wire types (docs/api.md). The only place that knows both.

use super::dto;
use crate::apps::AppStatus;
use crate::bridge::{BoardPin, BoardView, LinkState, PinRole};
use crate::config::Simulate;
use crate::connectors::LoadProblem;
use crate::files::{Entry, Kind, Special, VPath};
use crate::leases::{EndReason, Lease};
use crate::logs;
use crate::machine::EstopView;
use crate::machine::WorldView;
use crate::machine::{
    AppView, BoardCheck, MachineError, PackagePreview, PackagePreviewOf, PartView, PortView,
};
use crate::names::SignalAddress;
use crate::notices::Notice;
use crate::ports::{PortKind, PortProblem};
use crate::runtime::PartStatus;
use crate::simparts::WorldState;
use crate::simulation::Pattern;
use faber_connector::{ConnectorKind, Manifest, PortRequirement, Problem, SettingKind, SettingSpec};

pub fn error_body(e: &MachineError) -> dto::ErrorBody {
    dto::ErrorBody {
        code: e.code.to_string(),
        message: e.message.clone(),
        hint: e.hint.clone(),
        reason: e.lease_ended.map(lease_end_reason),
        needs: None,
        problems: (!e.plan_problems.is_empty()).then(|| e.plan_problems.clone()),
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
        PartStatus::Paused => state(K::Paused, None),
    }
}

pub fn part(p: &PartView) -> dto::PartInfo {
    dto::PartInfo {
        name: p.name.to_string(),
        connector: p.connector.clone(),
        port: p.source.port().map(str::to_string),
        address: p.address,
        simulate: p.source.simulate().map(simulate),
        state: part_state(&p.status),
        signals: p
            .signals
            .iter()
            .map(|s| dto::SignalInfo {
                address: s.address.to_string(),
                capability: s.capability,
                info: s.info.clone(),
                battery: battery_volts(s.info.as_ref()),
            })
            .collect(),
        mount: p.mount,
        bridge: p.bridge.as_ref().map(ToString::to_string),
        pins: p.pins.as_ref().map(|pins| pins.iter().map(board_pin).collect()),
        board: p.board.as_ref().map(|b| board_link(b, p.code.as_ref())),
    }
}

fn board_pin(p: &BoardPin) -> dto::BoardPinInfo {
    use dto::PinRole as R;
    let role = match p.role {
        PinRole::Pwm => R::Pwm,
        PinRole::Dir => R::Dir,
        PinRole::En => R::En,
        PinRole::In1 => R::In1,
        PinRole::In2 => R::In2,
        PinRole::Stby => R::Stby,
        PinRole::Trig => R::Trig,
        PinRole::Echo => R::Echo,
        PinRole::Signal => R::Signal,
        PinRole::Sense => R::Sense,
        PinRole::Other(_) => R::Other,
    };
    dto::BoardPinInfo { role, pin: p.pin, label: p.label.clone() }
}

pub fn board_link(b: &BoardView, code: Option<&crate::machine::FlashedCode>) -> dto::BoardLink {
    let info = b.board.as_ref();
    dto::BoardLink {
        link: match b.link {
            LinkState::Connecting => dto::LinkState::Connecting,
            LinkState::Describing => dto::LinkState::Describing,
            LinkState::Ready => dto::LinkState::Ready,
            LinkState::NoAnswer => dto::LinkState::NoAnswer,
            LinkState::Lost => dto::LinkState::Lost,
            LinkState::Incompatible => dto::LinkState::Incompatible,
        },
        board: info.map(|i| i.board.clone()),
        firmware: info.map(|i| i.firmware.clone()),
        firmware_version: info.map(|i| i.firmware_version.clone()),
        protocol: info.map(|i| i.version),
        heartbeat_timeout_ms: info.map(|i| i.heartbeat_timeout_ms),
        hardware_estop: b.hardware_estop,
        failsafe: b.failsafe,
        code: code.map(|c| dto::BoardCodeLink {
            build_id: c.build_id.clone(),
            record: c.record.as_ref().map(super::arduino::flash_record),
        }),
    }
}

pub fn board_check(c: &BoardCheck) -> dto::BoardCheck {
    match c.clone() {
        BoardCheck::Found { board, firmware, firmware_version, parts } => {
            dto::BoardCheck::Found { board, firmware, firmware_version, parts }
        }
        BoardCheck::OtherVersion { message } => dto::BoardCheck::OtherVersion { message },
        BoardCheck::Nothing => dto::BoardCheck::Nothing,
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
        EndReason::PartPaused => R::PartPaused,
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

/// The voltages a battery's info implies, for screens to show next to its level.
pub fn battery_volts(info: Option<&faber_capabilities::Info>) -> Option<dto::BatteryVolts> {
    let Some(faber_capabilities::Info::Battery(i)) = info else { return None };
    let (empty, full) = i.range_volts();
    let (low, critical) = i.thresholds_volts();
    Some(dto::BatteryVolts { empty, full, low, critical })
}

pub fn notice(n: &Notice) -> dto::Notice {
    match n {
        Notice::BatteryLevel { t_us, address, level, previous, volts } => dto::Notice::BatteryLevel {
            t_us: *t_us,
            address: address.to_string(),
            level: *level,
            previous: *previous,
            volts: *volts,
        },
    }
}

pub fn pattern(p: Pattern) -> dto::Pattern {
    match p {
        Pattern::Steady { value } => dto::Pattern::Steady { value },
        Pattern::Sine { center, amplitude, period_s } => dto::Pattern::Sine { center, amplitude, period_s },
        Pattern::Noise { center, amplitude } => dto::Pattern::Noise { center, amplitude },
        Pattern::Step { low, high, period_s } => dto::Pattern::Step { low, high, period_s },
        Pattern::Drain { from, to, period_s } => dto::Pattern::Drain { from, to, period_s },
    }
}

pub fn pattern_from(p: dto::Pattern) -> Pattern {
    match p {
        dto::Pattern::Steady { value } => Pattern::Steady { value },
        dto::Pattern::Sine { center, amplitude, period_s } => Pattern::Sine { center, amplitude, period_s },
        dto::Pattern::Noise { center, amplitude } => Pattern::Noise { center, amplitude },
        dto::Pattern::Step { low, high, period_s } => Pattern::Step { low, high, period_s },
        dto::Pattern::Drain { from, to, period_s } => Pattern::Drain { from, to, period_s },
    }
}

pub fn i2c_scan(s: &crate::machine::I2cScan) -> dto::I2cScan {
    dto::I2cScan {
        port: s.port.clone(),
        first: crate::ports::scan::FIRST,
        last: crate::ports::scan::LAST,
        found: s
            .found
            .iter()
            .map(|f| dto::I2cFound {
                address: f.address,
                used_by: f.used_by.clone(),
                suggestions: f
                    .suggestions
                    .iter()
                    .map(|g| dto::I2cSuggestion {
                        chip: g.chip.clone(),
                        connector: g.connector.clone(),
                        reason: g.reason.clone(),
                    })
                    .collect(),
            })
            .collect(),
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
            PortKind::I2c => dto::PortKind::I2c,
            PortKind::Spi => dto::PortKind::Spi,
            PortKind::Gpio => dto::PortKind::Gpio,
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
        addresses: p
            .addresses
            .iter()
            .map(|(address, who)| dto::AddressClaim { address: *address, claimed_by: who.clone() })
            .collect(),
        reserved: p.entry.reserved.clone(),
        suggestions: p
            .suggestions
            .iter()
            .map(|s| dto::Suggestion { connector: s.connector.clone(), reason: s.reason.clone() })
            .collect(),
        board: match p.board {
            faber_flash::Detected::Board(b) => {
                Some(dto::BoardGuess { board: Some(b.id.into()), usb_chip: None })
            }
            faber_flash::Detected::UsbSerial { chip } => {
                Some(dto::BoardGuess { board: None, usb_chip: Some(chip.into()) })
            }
            faber_flash::Detected::Unknown => None,
        },
    }
}

pub fn flash_stage(s: faber_flash::Stage) -> dto::FlashStage {
    match s {
        faber_flash::Stage::Connecting => dto::FlashStage::Connecting,
        faber_flash::Stage::Writing => dto::FlashStage::Writing,
        faber_flash::Stage::Verifying => dto::FlashStage::Verifying,
        faber_flash::Stage::Done => dto::FlashStage::Done,
    }
}

pub fn connector_problem(p: &LoadProblem) -> dto::ConnectorProblem {
    dto::ConnectorProblem {
        folder: p.folder.clone(),
        message: p.problem.message.clone(),
        hint: p.problem.hint.clone(),
    }
}

/// A connector's package answers a `PackagePreview`, an app's an `AppPackagePreview`.
pub fn package_preview(p: &PackagePreview) -> Result<dto::PackagePreview, Box<dto::AppPackagePreview>> {
    match &p.of {
        PackagePreviewOf::Connector(m) => {
            let kind = if p.header.kind == "wasm" { ConnectorKind::Wasm } else { ConnectorKind::Declarative };
            Ok(dto::PackagePreview {
                connector: connector(m, kind),
                publisher: p.header.publisher.clone(),
                readme: p.readme.clone(),
                captures: p.captures.clone(),
                replaces: p.replaces.clone(),
                checked: false,
            })
        }
        PackagePreviewOf::App(m) => Err(Box::new(dto::AppPackagePreview {
            app: app_info(&AppView {
                manifest: m.clone(),
                status: AppStatus::Stopped(Problem::new("The app is turned off.", "Turn it on to start it.")),
                enabled: false,
                bindings: Default::default(),
                has_icon: false,
            }),
            readme: p.readme.clone(),
            replaces: p.replaces.clone(),
            checked: false,
        })),
    }
}

pub fn app_state(s: &AppStatus) -> dto::PartState {
    use dto::PartStateKind as K;
    match s {
        AppStatus::Invalid(p) => state(K::Invalid, Some(p)),
        AppStatus::Conflict(p) => state(K::Conflict, Some(p)),
        AppStatus::Starting => state(K::Starting, None),
        AppStatus::Running => state(K::Running, None),
        AppStatus::Failed(p) => state(K::Failed, Some(p)),
        AppStatus::Stopped(p) => state(K::Stopped, Some(p)),
    }
}

pub fn app_info(a: &AppView) -> dto::AppInfo {
    let m = &a.manifest;
    let ui_url = |rel: &str| format!("/apps/{}/{rel}", m.id);
    dto::AppInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        publisher: m.publisher.clone(),
        enabled: a.enabled,
        state: app_state(&a.status),
        logic: m.module.is_some(),
        tick_hz: m.module.as_ref().map(|_| m.tick_hz),
        roles: m
            .requires
            .iter()
            .map(|r| dto::AppRole {
                role: r.role.clone(),
                capability: r.capability,
                command: r.command,
                count: r.count.id().to_string(),
                drive: match r.drive {
                    crate::apps::manifest::DrivenBy::Logic => dto::AppDrive::Logic,
                    crate::apps::manifest::DrivenBy::Window => dto::AppDrive::Window,
                },
                parts: a
                    .bindings
                    .get(&r.role)
                    .map(|ps| ps.iter().map(|p| p.to_string()).collect())
                    .unwrap_or_default(),
            })
            .collect(),
        publishes: m
            .publishes
            .iter()
            .map(|p| dto::AppSignal {
                name: p.name.clone(),
                capability: p.capability,
                address: SignalAddress::derived(&m.id, p.name.clone()).to_string(),
            })
            .collect(),
        ui: m.ui.then(|| ui_url("ui/index.html")),
        icon: m
            .window
            .as_ref()
            .and_then(|w| w.icon.as_deref())
            .filter(|i| i.starts_with("ui/"))
            .map(ui_url)
            .or_else(|| a.has_icon.then(|| ui_url(crate::packages::ICON))),
        window: m.window.as_ref().map(|w| dto::AppWindow {
            title: w.title.clone(),
            icon: w.icon.as_deref().filter(|i| i.starts_with("ui/")).map(ui_url),
            width: w.width,
            height: w.height,
            pinned: w.pinned,
        }),
    }
}

pub fn package(p: &crate::machine::PackageView) -> dto::PackageInfo {
    let i = &p.installed;
    dto::PackageInfo {
        id: i.id.clone(),
        name: i.name.clone(),
        version: i.version.clone(),
        kind: i.kind.clone(),
        publisher: i.publisher.clone(),
        installed_at: i.installed_at.clone(),
        source: i.source.clone(),
        checked: i.checked,
        used_by: p.used_by.iter().map(|n| n.to_string()).collect(),
        problem: p.problem.as_ref().map(|pr| dto::ConnectorProblem {
            folder: i.id.clone(),
            message: pr.message.clone(),
            hint: pr.hint.clone(),
        }),
        firmware: p.firmware.clone(),
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

pub fn connector(m: &Manifest, kind: ConnectorKind) -> dto::ConnectorInfo {
    dto::ConnectorInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        kind: kind.id().into(),
        port: match &m.port {
            PortRequirement::Serial { baud } => dto::PortRequirementInfo::Serial { baud: *baud },
            PortRequirement::Bridge => dto::PortRequirementInfo::Bridge,
            PortRequirement::I2c { addresses, default } => {
                dto::PortRequirementInfo::I2c { addresses: addresses.clone(), default: *default }
            }
            PortRequirement::Spi => dto::PortRequirementInfo::Spi,
            PortRequirement::Gpio => dto::PortRequirementInfo::Gpio,
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
    fn a_battery_signal_carries_its_voltages_and_nothing_else_does() {
        let info = faber_capabilities::Info::Battery(faber_capabilities::battery::Info {
            chemistry: faber_capabilities::battery::Chemistry::LiIon,
            cells_in_series: 3,
            capacity_ah: None,
        });
        let v = battery_volts(Some(&info)).unwrap();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(
            close(v.empty, 9.0) && close(v.full, 12.6) && close(v.low, 10.5) && close(v.critical, 9.9),
            "{v:?}"
        );
        assert_eq!(battery_volts(None), None);
    }

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
        let c = connector(&ldrobot_ld19::manifest(), ConnectorKind::BuiltIn);
        assert_eq!(c.kind, "built-in");
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
        let json = serde_json::to_value(connector(&base, ConnectorKind::BuiltIn)).unwrap();
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
        assert_eq!(back, connector(&base, ConnectorKind::BuiltIn), "it reads back the same");
    }
}
