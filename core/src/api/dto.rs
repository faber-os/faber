//! Data shapes on the wire between the core and the shell (docs/api.md).
//!
//! These types are the API contract. TypeScript versions are generated into
//! `shell/src/lib/api/generated/` by `cargo test` (ts-rs), and CI fails if they are out of date.
//! Field names are camelCase on the wire.

use crate::sim::world::{Footprint, Geometry, Pose, Twist};
use faber_capabilities::{Capability, Command, Info, Mount, Reading};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemInfo {
    pub name: String,
    pub version: String,
    /// Fingerprint of the API types this core was built with. The shell compares it with its own
    /// and asks for a restart when they differ (docs/api.md).
    pub api: String,
    /// The core's monotonic clock right now (µs). Readings use the same clock.
    #[ts(type = "number")]
    pub time_us: u64,
    /// The machine's own IANA time zone, e.g. `Europe/Copenhagen` (`UTC` if the OS doesn't say).
    /// Wall-clock times are shown in it unless the settings pick another (D42).
    pub time_zone: String,
    /// Set when parts.toml can't be read. Then no parts run and the shell must show this.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_problem: Option<ErrorBody>,
}

/// An error the user can act on: what happened, and what to try next (docs/principles.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ErrorBody {
    /// Stable, machine-readable, kebab-case, e.g. `port-claimed`.
    pub code: String,
    /// What happened, in plain words.
    pub message: String,
    /// The next thing to try.
    pub hint: String,
    /// On `lease-ended`: why it ended, as the `leases` topic says it.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<LeaseEndReason>,
}

/// The body of every non-2xx REST response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiError {
    pub error: ErrorBody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export)]
pub enum PortRequirementInfo {
    Serial {
        baud: u32,
    },
    /// Hardware like this is reached through a FaberLink board, whose own parts appear by
    /// themselves once the board is added; a part on this connector can only be simulated.
    Bridge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvideInfo {
    pub name: String,
    pub capability: Capability,
    /// Its limits with the default settings (range, top speed, …).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<Info>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// How it runs: `built-in`, `declarative` (only a faber.toml) or `wasm`; `native` later.
    pub kind: String,
    pub port: PortRequirementInfo,
    pub provides: Vec<ProvideInfo>,
    /// Every setting a part of this kind takes, in the order a form shows them. Empty: none.
    pub settings: Vec<SettingInfo>,
}

/// One setting a connector takes, as `[parts.<name>.settings]` holds it. Labels and descriptions
/// are the connector's own words, in English for now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingInfo {
    /// The key in parts.toml, e.g. `max-vx`.
    pub key: String,
    /// A short name for people, e.g. "Top speed forward".
    pub label: String,
    /// One sentence on what it does.
    pub description: String,
    /// A number's unit, SI as in parts.toml (`m`, `m/s`, `rad/s`, `Hz`).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Rarely needed: shown behind "Advanced".
    pub advanced: bool,
    #[serde(flatten)]
    pub kind: SettingKindInfo,
}

/// What a setting holds, and its default (what a part gets when parts.toml leaves it out).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[ts(export)]
pub enum SettingKindInfo {
    #[serde(rename_all = "camelCase")]
    Number {
        default: f64,
        /// The smallest value allowed; with `minExclusive`, values must be above it.
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[ts(as = "Option<bool>", optional)]
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        min_exclusive: bool,
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        /// Where a new part's form starts, when that differs from `default`.
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggested: Option<f64>,
    },
    Bool {
        default: bool,
    },
    Choice {
        default: String,
        choices: Vec<ChoiceInfo>,
    },
    String {
        default: String,
    },
    /// A list of other parts' names; empty by default.
    PartList,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChoiceInfo {
    /// What parts.toml holds, e.g. `laser-tof`.
    pub value: String,
    /// What people read, e.g. "Laser time-of-flight".
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum PortKind {
    /// A USB serial adapter, matched by vendor:product id.
    UsbSerial,
    /// A serial port without USB information (e.g. a Pi UART).
    Serial,
    /// A serial link over Bluetooth (shown so it isn't mistaken for a cable).
    Bluetooth,
    /// A recorded capture played back as if it were live.
    Replay,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsbInfo {
    pub vid: u16,
    pub pid: u16,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
}

/// "This port might be one of these" (detect-then-suggest, docs/principles.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Suggestion {
    pub connector: String,
    /// Why Faber suggests it, in plain words.
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PortInfo {
    /// Stable id used in parts.toml, e.g. `usb:10c4:ea60`, `serial:COM3`, `replay:demo-ld19`.
    pub id: String,
    pub kind: PortKind,
    /// Human-friendly name, e.g. "CP2102 USB to UART Bridge (COM3)".
    pub label: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb: Option<UsbInfo>,
    /// The part or tool currently using the port, e.g. `lidar` or `tool:serial-monitor`.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    pub suggestions: Vec<Suggestion>,
}

/// A connector installed in `<data>/connectors/` that the core couldn't load. The core runs
/// without it; this says why, so a missing connector isn't a mystery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorProblem {
    /// Its folder in `<data>/connectors/`, e.g. `my-sensor`.
    pub folder: String,
    /// What is wrong, in plain words.
    pub message: String,
    /// The next thing to try.
    pub hint: String,
}

/// What a package file would install (`POST /api/v0/packages?mode=preview`), read from the file
/// and checked in full: the connector as the core loaded it from the package.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackagePreview {
    /// The connector, as `GET /api/v0/connectors` will show it once installed. Its `port` says
    /// what it can touch: the one port a user picks for each part.
    pub connector: ConnectorInfo,
    /// Who made it, from `[package] publisher`.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Its README.md, as text (Markdown). Never HTML: show it as text or safe minimal Markdown.
    pub readme: String,
    /// The recordings in its `captures/`.
    pub captures: Vec<String>,
    /// The version installed now, which this would replace.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    /// Whether the Faber registry checked it. Always `false` until the store exists (Phase 7).
    pub checked: bool,
}

/// A package installed on this machine (`GET /api/v0/packages`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PackageInfo {
    /// The connector's id, also its folder in `<data>/connectors/`.
    pub id: String,
    pub name: String,
    pub version: String,
    /// `wasm` or `declarative`.
    pub kind: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// When it was installed (RFC 3339, UTC).
    pub installed_at: String,
    /// The file it was installed from.
    pub source: String,
    /// Whether the Faber registry checked it. Always `false` until the store exists (Phase 7).
    pub checked: bool,
    /// The parts that use its connector. It can't be uninstalled while there are any.
    pub used_by: Vec<String>,
    /// Why its connector isn't loaded, when it isn't.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<ConnectorProblem>,
}

// ---------------------------------------------------------------------------------------------
// Apps (docs/apps.md, docs/api.md "Apps").

/// One role an app requires (`[[requires]]`), and the parts picked for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppRole {
    pub role: String,
    /// What a part must provide to fill it.
    pub capability: Capability,
    /// The app may drive it (it takes a lease on it, as `app:<id>`).
    pub command: bool,
    /// `1` (one part) or `1..` (one or more).
    pub count: String,
    /// The parts picked for it, in order. Empty: none yet (the app can't run).
    pub parts: Vec<String>,
}

/// A derived signal an app publishes (`[[publishes]]`, D24): read-only, never a part's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppSignal {
    pub name: String,
    pub capability: Capability,
    /// Where it is published: `app:<id>/<name>`.
    pub address: String,
}

/// How an app's UI opens in the shell (`[window]`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppWindow {
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A URL under the app's own files, e.g. `/apps/<id>/ui/icon.svg`, when it has an icon.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub pinned: bool,
}

/// An installed app (`GET /api/v0/apps`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Turned on by the user. It runs once every role has a part.
    pub enabled: bool,
    /// The same states as a part's: `stopped` while it is off.
    pub state: PartState,
    /// It has logic that runs in the background (a wasm component).
    pub logic: bool,
    /// How often its logic runs, when it has logic.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_hz: Option<u32>,
    pub roles: Vec<AppRole>,
    pub publishes: Vec<AppSignal>,
    /// Where its UI starts (`/apps/<id>/ui/index.html`), when it has one.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<AppWindow>,
}

/// What an app's package would install (`POST /api/v0/packages?mode=preview` with an app's
/// package; a connector's answers a [`PackagePreview`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppPackagePreview {
    /// The app as the core loaded it from the package. It installs turned off, with no parts
    /// picked for its roles.
    pub app: AppInfo,
    /// Its README.md, as text (Markdown). Never HTML.
    pub readme: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    pub checked: bool,
}

/// The parts for one role: one part's name, or a list (for a `count = "1.."` role).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum RoleParts {
    One(String),
    Many(Vec<String>),
}

/// `PUT /api/v0/apps/{id}/roles`: the parts for each role. Roles left out get no part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct AppRolesRequest {
    pub roles: std::collections::BTreeMap<String, RoleParts>,
}

/// Something plugged in over USB that the computer can't talk to, so it never became a port
/// (e.g. a cable without data wires). Shown so an empty port list isn't a mystery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PortProblem {
    /// Stable, kebab-case, e.g. `usb-no-answer`.
    pub code: String,
    /// One line, e.g. "Plugged in, but the computer can't talk to it".
    pub label: String,
    /// What happened, in plain words.
    pub detail: String,
    /// The next thing to try.
    pub hint: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum PartStateKind {
    /// The config for this part is wrong (see message and hint).
    Invalid,
    /// Its port is missing or used by something else.
    Conflict,
    Starting,
    Running,
    Degraded,
    /// Stopped working; the core retries with backoff.
    Failed,
    /// Gave up after repeated failures, or stopped by the user.
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartState {
    pub kind: PartStateKind,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// `GET /api/v0/signals/{part}/{name}`: the newest reading, for programs that poll over HTTP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalValue {
    pub address: String,
    pub capability: Capability,
    /// `null` until the part has published anything.
    pub reading: Option<Reading>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalInfo {
    /// `part/name`, e.g. `lidar/laser-scan`.
    pub address: String,
    pub capability: Capability,
    /// Its limits (range, top speed, …) as this part's own settings give them. Absent when the
    /// capability has none, or the part's settings can't be read.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<Info>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartInfo {
    pub name: String,
    pub connector: String,
    /// The real part's port. Absent for a simulated part.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// Set for a simulated part (D36): where its values come from.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulate: Option<Simulate>,
    pub state: PartState,
    pub signals: Vec<SignalInfo>,
    pub mount: Mount,
    /// For a part on a FaberLink board (a sub-part, `mega.left`): the bridge it is on. In
    /// `GET /api/v0/parts` sub-parts follow their bridge.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,
    /// For a bridge (`faber-bridge`): its board and the link to it.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardLink>,
}

/// A FaberLink board as its bridge part sees it (docs/faberlink.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardLink {
    pub link: LinkState,
    /// What the board says about itself, once it has answered: its name, e.g. `sennep-mega`.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    /// The FaberLink version both sides speak.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u8>,
    /// The board stops every actuator when Faber's heartbeat is missing this long.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout_ms: Option<u16>,
    /// The board's own stop button is latched: reset it on the board, then release the e-stop.
    pub hardware_estop: bool,
    /// The board stopped its actuators because Faber's heartbeat didn't reach it in time.
    pub failsafe: bool,
}

/// Where the link to a FaberLink board stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum LinkState {
    /// Saying hello; the board hasn't answered yet.
    Connecting,
    /// The board answered; Faber is collecting its parts.
    Describing,
    Ready,
    /// No answer to hello for a while (not a FaberLink board, the wrong baud, or still starting).
    NoAnswer,
    /// The port closed or the board went quiet; Faber keeps trying.
    Lost,
    /// The board speaks another FaberLink version.
    Incompatible,
}

/// Body of `POST /api/v0/ports/check-board`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardCheckRequest {
    /// The port's id, e.g. `usb:2341:0042`.
    pub port: String,
}

/// What checking a port for a FaberLink board found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "result", rename_all = "kebab-case")]
#[ts(export)]
pub enum BoardCheck {
    /// A FaberLink board answered. It is suggested for the port from now on.
    #[serde(rename_all = "camelCase")]
    Found { board: String, firmware: String, firmware_version: String, parts: u8 },
    /// A FaberLink board answered, but it speaks another protocol version: what to update.
    OtherVersion { message: String },
    /// Nothing answered.
    Nothing,
}

/// How a simulated part gets its values (docs/simulation.md, section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Simulate {
    /// A value or pattern set by hand; actuators report the commands they get.
    Manual,
    /// From the simulated world.
    World,
}

/// Body of `POST /api/v0/parts`: a `port` for the real part, or `simulate`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewPart {
    pub name: String,
    pub connector: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulate: Option<Simulate>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<Mount>,
    /// The connector's settings, as in `[parts.<name>.settings]`, e.g. `{"max-vx": 0.6}`.
    #[ts(optional, type = "Record<string, unknown>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogEntry {
    #[ts(type = "number")]
    pub t_us: u64,
    pub level: LogLevel,
    /// Where it came from, e.g. `part:lidar` or `core::ports`.
    pub source: String,
    pub message: String,
}

/// What a WebSocket subscription is for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[ts(export)]
pub enum Topic {
    /// Readings of one signal, at most `maxHz` per second (latest-value signals skip the rest).
    #[serde(rename_all = "camelCase")]
    Signal {
        address: String,
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_hz: Option<f32>,
    },
    /// The full parts list, sent now and whenever any part changes.
    Parts,
    /// The ports list, sent now and whenever ports appear, vanish or are claimed.
    Ports,
    /// The recent log, then every new entry.
    Logs,
    /// The settings now and after every change (so the browser and the attached screen agree).
    Settings,
    /// `files-changed` whenever files change through the core.
    Files,
    /// Who drives what: a `leases` list now, then `lease-granted` and `lease-ended`.
    Leases,
    /// The e-stop: an `estop` message now and on every change (none is lost).
    Estop,
    /// The machine in the simulated world: a `world` message 20 times a second, and
    /// `world-changed` when the world is switched.
    World,
    /// The installed connector packages: a `packages` list now and after every install,
    /// uninstall or reload of the connectors (so every browser tab sees an install at once).
    Packages,
    /// The installed apps: an `apps` list now and whenever one is installed, removed, turned on
    /// or off, gets other parts, or changes state.
    Apps,
}

/// Messages from the shell to the core.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "type")]
#[ts(export)]
pub enum ClientMessage {
    Subscribe {
        id: u32,
        topic: Topic,
    },
    Unsubscribe {
        id: u32,
    },
    /// Opens a port in the serial monitor. Fails with `port-claimed` if a part uses it.
    SerialOpen {
        id: u32,
        port: String,
        baud: u32,
    },
    SerialSend {
        id: u32,
        #[serde(with = "serde_bytes")]
        #[ts(type = "Uint8Array")]
        data: Vec<u8>,
    },
    SerialClose {
        id: u32,
    },
    /// Starts saving everything the open session receives to `Recordings/<name>.fbcap`.
    SerialRecordStart {
        id: u32,
        name: String,
    },
    SerialRecordStop {
        id: u32,
    },
}

/// Messages from the core to the shell. `id` is the subscription or serial session it belongs to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "type")]
#[ts(export)]
pub enum ServerMessage {
    Signal {
        id: u32,
        address: String,
        reading: Reading,
    },
    Parts {
        id: u32,
        parts: Vec<PartInfo>,
    },
    Ports {
        id: u32,
        ports: Vec<PortInfo>,
        /// Left out when there are none.
        #[ts(as = "Option<Vec<PortProblem>>", optional)]
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        problems: Vec<PortProblem>,
    },
    Log {
        id: u32,
        entry: LogEntry,
    },
    #[serde(rename_all = "camelCase")]
    SerialData {
        id: u32,
        #[ts(type = "number")]
        t_us: u64,
        #[serde(with = "serde_bytes")]
        #[ts(type = "Uint8Array")]
        data: Vec<u8>,
    },
    /// The port asked for in `serial-open` is open. From now on only `serial-closed` ends the
    /// session; an `error` for it (e.g. a failed write) leaves it open.
    SerialOpened {
        id: u32,
        port: String,
    },
    SerialClosed {
        id: u32,
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Settings {
        id: u32,
        settings: Settings,
    },
    /// Files changed in these folders; listings of them are out of date.
    FilesChanged {
        id: u32,
        folders: Vec<String>,
    },
    /// Recording started for serial session `id`, into `path`.
    SerialRecording {
        id: u32,
        path: String,
    },
    SerialRecordingStopped {
        id: u32,
        path: String,
        #[ts(type = "number")]
        bytes: u64,
    },
    /// Every live lease (sent when the `leases` topic is subscribed).
    Leases {
        id: u32,
        leases: Vec<LeaseInfo>,
    },
    LeaseGranted {
        id: u32,
        lease: LeaseInfo,
    },
    /// A lease ended, and its parts got their safe state.
    LeaseEnded {
        id: u32,
        lease: LeaseInfo,
        reason: LeaseEndReason,
    },
    Estop {
        id: u32,
        status: EstopStatus,
    },
    /// Every installed package (the `packages` topic), as `GET /api/v0/packages` lists them.
    Packages {
        id: u32,
        packages: Vec<PackageInfo>,
    },
    /// Every installed app (the `apps` topic), as `GET /api/v0/apps` lists them.
    Apps {
        id: u32,
        apps: Vec<AppInfo>,
    },
    /// The machine in the simulated world now (the `world` topic, 20 times a second).
    #[serde(rename_all = "camelCase")]
    World {
        id: u32,
        /// The ground truth, in the world frame (m, rad).
        pose: Pose,
        /// Machine frame: `vx` forward, `vy` left (m/s), `omega` counter-clockwise (rad/s).
        velocity: Twist,
        /// Something stopped the machine during its last step.
        bumped: bool,
        /// Where the moving obstacles are now.
        movers: Vec<WorldMover>,
        /// When the world last moved on (µs, core clock).
        #[ts(type = "number")]
        t_us: u64,
    },
    /// The world was switched (the machine is at rest at its start). `GET /api/v0/world` has
    /// its geometry.
    WorldChanged {
        id: u32,
        name: String,
    },
    /// Something the client asked for failed. `id` is absent when it isn't tied to one request.
    Error {
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<u32>,
        error: ErrorBody,
    },
}

// ---------------------------------------------------------------------------------------------
// The simulated world (docs/api.md, "The simulated world").

/// The world simulated parts are in (`GET /api/v0/world`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorldView {
    pub name: String,
    pub title: String,
    pub description: String,
    /// Where the machine starts: `[world] start` in parts.toml, or the world's own start.
    pub start: Pose,
    pub geometry: Geometry,
    /// The world base's footprint (m). Absent while no part is the world's base.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Footprint>,
}

/// `PUT /api/v0/world`: which world, and where the machine starts in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorldRequest {
    pub name: String,
    /// Left out: the world's own start.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Pose>,
}

/// A moving obstacle on the `world` topic: a circle (m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorldMover {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

// ---------------------------------------------------------------------------------------------
// Files (docs/api.md, "Files"). Paths are `<root>:/<path>`, e.g. `home:/Desktop/notes.txt`.

/// A place files can live: `home` always; `machine` only with the developer setting on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileRoot {
    pub id: String,
    pub label: String,
    /// The root folder's path, e.g. `home:/`.
    pub path: String,
    /// Shown wherever this root is browsed (the whole-machine root has one).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum FileKind {
    File,
    Folder,
}

/// The home folder's fixed folders. They can't be renamed or deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum SpecialFolder {
    Desktop,
    Documents,
    Recordings,
    /// Where backups are made (`POST /api/v0/backups`).
    Backups,
    /// Shows the machine config. Saving there is validated first.
    System,
    Trash,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    #[ts(type = "number")]
    pub size: u64,
    /// Last change, milliseconds since 1970 (UTC).
    #[ts(optional, type = "number")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_ms: Option<u64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special: Option<SpecialFolder>,
    /// For files in the Trash: where they came from.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderListing {
    pub path: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PathRequest {
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveRequest {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PathsRequest {
    pub paths: Vec<String>,
}

/// `GET /api/v0/about`: what a helper asks first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AboutInfo {
    pub version: String,
    /// The API fingerprint, as in `SystemInfo`.
    pub api: String,
    /// Where Faber keeps its config and files on this computer.
    pub data_folder: String,
    /// Seconds since the core started.
    #[ts(type = "number")]
    pub uptime_s: u64,
    /// The operating system, e.g. `Debian GNU/Linux 13 (trixie) (aarch64)`.
    pub os: String,
    /// The machine's own IANA time zone.
    pub time_zone: String,
}

/// `GET /api/v0/storage`: what the home folder's big folders use, and the data folder's disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageInfo {
    #[ts(type = "number")]
    pub recordings_bytes: u64,
    #[ts(type = "number")]
    pub documents_bytes: u64,
    #[ts(type = "number")]
    pub desktop_bytes: u64,
    #[ts(type = "number")]
    pub backups_bytes: u64,
    /// Space Faber can still use on the data folder's disk.
    #[ts(type = "number")]
    pub free_bytes: u64,
    /// The size of that disk.
    #[ts(type = "number")]
    pub total_bytes: u64,
}

/// Body of `POST /api/v0/backups`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupRequest {
    /// Also back up `home:/Recordings` (they can be big).
    #[serde(default)]
    pub include_recordings: bool,
}

/// Answer of `POST /api/v0/backups`: the new backup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupInfo {
    /// e.g. `home:/Backups/faber-backup-2026-10-15-1430.zip`
    pub path: String,
    #[ts(type = "number")]
    pub bytes: u64,
}

// ---------------------------------------------------------------------------------------------
// Leases, commands and the e-stop (docs/api.md, "Driving"; docs/simulation.md, sections 5–6).

/// Body of `POST /api/v0/parts/{name}/lease`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaseRequest {
    /// Who drives, in words people recognise: "Drive window", "drive-square.py".
    pub holder: String,
    /// End whoever holds it now (operators taking over). They hear `taken-over`.
    #[ts(as = "Option<bool>", optional)]
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub take_over: bool,
}

/// Body of `POST /api/v0/parts/{name}/lease/renew`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaseRenewal {
    pub lease: String,
}

/// A lease you hold: `201` from requesting it, `200` from renewing it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaseGrant {
    /// The lease id. Send it with every command; keep it to yourself.
    pub lease: String,
    pub part: String,
    pub holder: String,
    /// When it ends unless a command or a renewal comes first (µs, core clock).
    #[ts(type = "number")]
    pub deadline_us: u64,
    /// How long each command or renewal extends it.
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// The parts this one is built from, locked too.
    pub locks: Vec<String>,
}

/// A lease as everyone may see it (no id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LeaseInfo {
    pub part: String,
    pub holder: String,
    #[ts(type = "number")]
    pub deadline_us: u64,
    pub locks: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum LeaseEndReason {
    Released,
    /// No command or renewal before the deadline.
    Expired,
    Estop,
    TakenOver,
    PartRemoved,
    PartRestarted,
    PartFailed,
    Shutdown,
}

/// Body of `POST /api/v0/parts/{name}/command`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandRequest {
    pub lease: String,
    pub command: Command,
}

/// What a command did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandResult {
    /// The command as applied: clamped to the part's limits.
    pub applied: Command,
    /// True when the command was beyond the limits and `applied` differs from it.
    pub clamped: bool,
    /// The lease's new deadline (µs, core clock).
    #[ts(type = "number")]
    pub deadline_us: u64,
}

/// How a manual simulated sensor's value changes (in the capability's unit, e.g. m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case", tag = "kind")]
#[ts(export)]
pub enum Pattern {
    Steady {
        value: f32,
    },
    /// `center ± amplitude`, one wave every `periodS`.
    #[serde(rename_all = "camelCase")]
    Sine {
        center: f32,
        amplitude: f32,
        period_s: f32,
    },
    /// `center ± amplitude`, random.
    Noise {
        center: f32,
        amplitude: f32,
    },
    /// `low` for half of `periodS`, then `high`.
    #[serde(rename_all = "camelCase")]
    Step {
        low: f32,
        high: f32,
        period_s: f32,
    },
}

/// Body and answer of `PUT /api/v0/parts/{name}/simulation`, answer of `GET`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Simulation {
    pub pattern: Pattern,
}

/// Body of `POST /api/v0/estop`. Everything is optional: engaging is always allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EstopRequest {
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Who is engaging it; `app` when left out.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<faber_capabilities::estop::Source>,
}

/// Body of `POST /api/v0/estop/release` (optional).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EstopReleaseRequest {
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<faber_capabilities::estop::Source>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EstopStatus {
    pub engaged: bool,
    /// The change that engaged it, while it is engaged: who and why.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engaged_by: Option<faber_capabilities::estop::Reading>,
    /// Recent engages and releases, oldest first. Left out on the WebSocket.
    #[ts(as = "Option<Vec<faber_capabilities::estop::Reading>>", optional)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<faber_capabilities::estop::Reading>,
}

// Settings types live in `crate::settings` (layer 2 owns its data); re-exported for the wire.
pub use crate::settings::{
    Accessibility, Appearance, ClockFormat, DesktopLayout, DeveloperChange, DeveloperSettings, IconPosition,
    LengthUnit, MachineIdentity, Preference, Settings, Taskbar, TaskbarSize, TextSize, Theme, TimeSettings,
    Units,
};
pub use crate::translations::TranslationInfo;

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::laser_scan;
    use serde_json::json;

    #[test]
    fn client_messages_use_the_documented_shape() {
        let m: ClientMessage = serde_json::from_value(json!({
            "type": "subscribe", "id": 3,
            "topic": {"kind": "signal", "address": "lidar/laser-scan", "maxHz": 15.0}
        }))
        .unwrap();
        assert_eq!(
            m,
            ClientMessage::Subscribe {
                id: 3,
                topic: Topic::Signal { address: "lidar/laser-scan".into(), max_hz: Some(15.0) }
            }
        );
        let open: ClientMessage = serde_json::from_value(
            json!({"type": "serial-open", "id": 1, "port": "serial:COM3", "baud": 9600}),
        )
        .unwrap();
        assert!(matches!(open, ClientMessage::SerialOpen { baud: 9600, .. }));
    }

    #[test]
    fn server_signal_round_trips_through_messagepack() {
        let m = ServerMessage::Signal {
            id: 7,
            address: "lidar/laser-scan".into(),
            reading: Reading::LaserScan(laser_scan::Reading::even_bins(1, 0.0, 0.1, 0.1, vec![1.0])),
        };
        let bytes = rmp_serde::to_vec_named(&m).unwrap();
        assert_eq!(rmp_serde::from_slice::<ServerMessage>(&bytes).unwrap(), m);
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["type"], "signal");
        assert_eq!(json["reading"]["capability"], "laser-scan@1");
    }

    #[test]
    fn serial_bytes_are_binary_in_messagepack() {
        let m = ServerMessage::SerialData { id: 1, t_us: 5, data: vec![0x54, 0x2c] };
        let bytes = rmp_serde::to_vec_named(&m).unwrap();
        // 0xc4 = MessagePack "bin 8": raw bytes, not an array of numbers.
        assert!(bytes.windows(2).any(|w| w == [0xc4, 2]));
        assert_eq!(rmp_serde::from_slice::<ServerMessage>(&bytes).unwrap(), m);
    }

    #[test]
    fn errors_carry_a_hint() {
        let e = ApiError {
            error: ErrorBody {
                code: "port-claimed".into(),
                message: "m".into(),
                hint: "h".into(),
                reason: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"error": {"code": "port-claimed", "message": "m", "hint": "h"}})
        );
    }
}
