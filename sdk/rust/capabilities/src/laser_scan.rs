//! `laser-scan@1`: one revolution of a scanning LiDAR (docs/capabilities.md).
//!
//! Readings are in the part's own frame (x forward, y left, angles counter-clockwise). The
//! part's `mount` is applied by whoever displays or combines them.

use crate::Timestamp;
use crate::non_finite;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "LaserScanReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// Angle of the first point (rad).
    pub angle_min: f32,
    /// Spacing of even bins (rad). Ignored when `angles` is present.
    pub angle_increment: f32,
    /// One angle per point (rad) for raw points. `None` means even bins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub angles: Option<Vec<f32>>,
    /// Time between two points (s), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub time_increment: Option<f32>,
    /// Time of one full revolution (s).
    pub scan_time: f32,
    /// Range per point (m). `+inf` means no return.
    #[serde(with = "non_finite")]
    #[cfg_attr(feature = "typescript", ts(type = "number[]"))]
    pub ranges: Vec<f32>,
    /// Return strength per point, 0..1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub intensities: Option<Vec<f32>>,
}

/// Why a reading is not well-formed.
#[derive(Debug, PartialEq)]
pub enum Invalid {
    AnglesLengthMismatch { ranges: usize, angles: usize },
    IntensitiesLengthMismatch { ranges: usize, intensities: usize },
    NegativeRange { index: usize },
    NonFiniteAngle { index: usize },
}

impl Reading {
    /// Even bins: point `i` lies at `angle_min + i * angle_increment`.
    pub fn even_bins(
        t_us: Timestamp,
        angle_min: f32,
        angle_increment: f32,
        scan_time: f32,
        ranges: Vec<f32>,
    ) -> Reading {
        Reading {
            t_us,
            angle_min,
            angle_increment,
            angles: None,
            time_increment: None,
            scan_time,
            ranges,
            intensities: None,
        }
    }

    /// Raw points: every point carries its own angle.
    pub fn raw_points(t_us: Timestamp, scan_time: f32, angles: Vec<f32>, ranges: Vec<f32>) -> Reading {
        Reading {
            t_us,
            angle_min: angles.first().copied().unwrap_or(0.0),
            angle_increment: 0.0,
            angles: Some(angles),
            time_increment: None,
            scan_time,
            ranges,
            intensities: None,
        }
    }

    /// The angle of point `i`, whichever layout the reading uses.
    pub fn angle(&self, i: usize) -> f32 {
        match &self.angles {
            Some(a) => a[i],
            None => self.angle_min + i as f32 * self.angle_increment,
        }
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Checks the invariants every producer must keep. Connectors call this in their tests.
    pub fn validate(&self) -> Result<(), Invalid> {
        let n = self.ranges.len();
        if let Some(a) = &self.angles {
            if a.len() != n {
                return Err(Invalid::AnglesLengthMismatch { ranges: n, angles: a.len() });
            }
            if let Some(index) = a.iter().position(|x| !x.is_finite()) {
                return Err(Invalid::NonFiniteAngle { index });
            }
        }
        if let Some(i) = &self.intensities
            && i.len() != n
        {
            return Err(Invalid::IntensitiesLengthMismatch { ranges: n, intensities: i.len() });
        }
        if let Some(index) = self.ranges.iter().position(|r| *r < 0.0 || r.is_nan()) {
            return Err(Invalid::NegativeRange { index });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> Reading {
        let mut r = Reading::raw_points(42, 0.1, vec![0.0, 0.5, 1.0], vec![1.5, f32::INFINITY, 2.0]);
        r.intensities = Some(vec![0.9, 0.0, 0.4]);
        r
    }

    #[test]
    fn even_bins_compute_angles() {
        let r = Reading::even_bins(0, 1.0, 0.25, 0.1, vec![1.0, 1.0, 1.0]);
        assert_eq!(r.angle(0), 1.0);
        assert_eq!(r.angle(2), 1.5);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn raw_points_use_their_own_angles() {
        let r = raw();
        assert_eq!(r.angle(1), 0.5);
        assert_eq!(r.angle_min, 0.0);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validate_catches_mismatched_lengths() {
        let mut r = raw();
        r.angles = Some(vec![0.0]);
        assert_eq!(r.validate(), Err(Invalid::AnglesLengthMismatch { ranges: 3, angles: 1 }));
        let mut r = raw();
        r.intensities = Some(vec![]);
        assert_eq!(r.validate(), Err(Invalid::IntensitiesLengthMismatch { ranges: 3, intensities: 0 }));
    }

    #[test]
    fn validate_catches_bad_values() {
        let mut r = raw();
        r.ranges[0] = -1.0;
        assert_eq!(r.validate(), Err(Invalid::NegativeRange { index: 0 }));
        let mut r = raw();
        r.angles.as_mut().unwrap()[2] = f32::NAN;
        assert_eq!(r.validate(), Err(Invalid::NonFiniteAngle { index: 2 }));
    }

    #[test]
    fn json_writes_no_return_as_null_and_reads_it_back() {
        let r = raw();
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ranges"], serde_json::json!([1.5, null, 2.0]));
        assert_eq!(json["scanTime"].as_f64().unwrap() as f32, 0.1);
        assert!(json.get("timeIncrement").is_none(), "absent options are omitted");
        let back: Reading = serde_json::from_value(json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn messagepack_keeps_infinity_and_round_trips() {
        let r = raw();
        let bytes = rmp_serde::to_vec_named(&r).unwrap();
        let back: Reading = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(back, r);
        assert!(back.ranges[1].is_infinite());
    }
}
