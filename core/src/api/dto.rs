//! Data shapes on the wire between the core and the shell (docs/api.md).
//!
//! These types are the API contract. TypeScript versions are generated into
//! `shell/src/lib/api/generated/` by `cargo test` (ts-rs), and CI fails if they are out of date.
//! Field names are camelCase on the wire.

use faber_capabilities::{Capability, Mount, Reading};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemInfo {
    pub name: String,
    pub version: String,
    /// The core's monotonic clock right now (µs). Readings use the same clock.
    #[ts(type = "number")]
    pub time_us: u64,
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
    Serial { baud: u32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProvideInfo {
    pub name: String,
    pub capability: Capability,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectorInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `built-in` in Phase 1; `declarative`, `wasm`, `native` later.
    pub kind: String,
    pub port: PortRequirementInfo,
    pub provides: Vec<ProvideInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum PortKind {
    /// A USB serial adapter, matched by vendor:product id.
    UsbSerial,
    /// A serial port without USB information (e.g. a Pi UART).
    Serial,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SignalInfo {
    /// `part/name`, e.g. `lidar/laser-scan`.
    pub address: String,
    pub capability: Capability,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartInfo {
    pub name: String,
    pub connector: String,
    pub port: String,
    pub state: PartState,
    pub signals: Vec<SignalInfo>,
    pub mount: Mount,
}

/// Body of `POST /api/v0/parts`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewPart {
    pub name: String,
    pub connector: String,
    pub port: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<Mount>,
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
    /// Something the client asked for failed. `id` is absent when it isn't tied to one request.
    Error {
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<u32>,
        error: ErrorBody,
    },
}

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
            error: ErrorBody { code: "port-claimed".into(), message: "m".into(), hint: "h".into() },
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"error": {"code": "port-claimed", "message": "m", "hint": "h"}})
        );
    }
}
