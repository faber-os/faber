//! `distance@1`: one range to the nearest target (ROS Range, docs/capabilities.md).

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// Whether `meters` can be trusted.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DistanceStatus"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    /// Closer than `min-m`; `meters` is `min-m`.
    TooClose,
    /// Further than `max-m`; `meters` is `max-m`.
    TooFar,
    /// Nothing reflected.
    NoTarget,
    /// The sensor reported a fault.
    Error,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DistanceReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// Always a finite number (m); `status` says whether it is a real measurement.
    pub meters: f32,
    pub status: Status,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DistanceKind"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Ultrasonic,
    LaserTof,
    Infrared,
    Radar,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[Kind::Ultrasonic, Kind::LaserTof, Kind::Infrared, Kind::Radar];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Ultrasonic => "ultrasonic",
            Kind::LaserTof => "laser-tof",
            Kind::Infrared => "infrared",
            Kind::Radar => "radar",
        }
    }
}

/// What a distance sensor can see, reported once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DistanceInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub min_m: f32,
    pub max_m: f32,
    pub field_of_view_rad: f32,
    pub kind: Kind,
}

impl Info {
    /// The reading a sensor with this info gives for a target `meters` away: values outside the
    /// range are reported as `too-close` / `too-far` at the range's edge.
    pub fn reading(&self, t_us: Timestamp, meters: f32) -> Reading {
        let (meters, status) = if !meters.is_finite() {
            (self.max_m, Status::NoTarget)
        } else if meters < self.min_m {
            (self.min_m, Status::TooClose)
        } else if meters > self.max_m {
            (self.max_m, Status::TooFar)
        } else {
            (meters, Status::Ok)
        };
        Reading { t_us, meters, status }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info { min_m: 0.02, max_m: 4.0, field_of_view_rad: 0.26, kind: Kind::Ultrasonic }
    }

    #[test]
    fn readings_outside_the_range_say_so() {
        assert_eq!(info().reading(1, 1.5), Reading { t_us: 1, meters: 1.5, status: Status::Ok });
        assert_eq!(info().reading(1, 0.0).status, Status::TooClose);
        assert_eq!(info().reading(1, 0.0).meters, 0.02);
        assert_eq!(info().reading(1, 9.0), Reading { t_us: 1, meters: 4.0, status: Status::TooFar });
        assert_eq!(info().reading(1, f32::INFINITY).status, Status::NoTarget);
    }

    #[test]
    fn json_shape_round_trips() {
        let r = info().reading(7, 0.0);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "too-close");
        assert_eq!(v["tUs"], 7);
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let i = serde_json::to_value(info()).unwrap();
        assert_eq!(i["kind"], "ultrasonic");
        assert!(i.get("fieldOfViewRad").is_some());
        assert_eq!(serde_json::from_value::<Info>(i).unwrap(), info());
        for k in Kind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.id());
        }
    }
}
