//! `temperature@1`: one temperature (ROS Temperature, docs/capabilities.md, "Environment"). In °C,
//! the one exception to SI that ROS makes too (rule 2).

use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "TemperatureReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    pub celsius: f32,
    /// How uncertain it is (°C²), when the sensor says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub variance: Option<f32>,
}

impl Reading {
    pub fn celsius(t_us: Timestamp, celsius: f32) -> Reading {
        Reading { t_us, celsius, variance: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_round_trips_and_leaves_out_an_unknown_variance() {
        let r = Reading::celsius(4, 21.5);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v, serde_json::json!({"tUs": 4, "celsius": 21.5}));
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let r = Reading { variance: Some(0.25), ..r };
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }
}
