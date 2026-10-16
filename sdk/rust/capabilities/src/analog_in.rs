//! `analog-in@1`: one analog input, such as an ADC channel (docs/capabilities.md, "GPIO level").
//! `fraction` is the reading as a part of the converter's full scale (0–1), so it means the
//! same whatever the converter's bits; `volts` is there when the part knows its reference.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "AnalogInReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// 0 (nothing) to 1 (full scale).
    pub fraction: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub volts: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_round_trips() {
        let r = Reading { t_us: 2, fraction: 0.5, volts: Some(1.65) };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["fraction"], 0.5);
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let bare = Reading { volts: None, ..r };
        assert!(serde_json::to_value(&bare).unwrap().get("volts").is_none());
    }
}
