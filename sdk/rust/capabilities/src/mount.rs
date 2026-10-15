//! Where a spatial part sits on the machine (docs/capabilities.md, rule 3).

use serde::{Deserialize, Serialize};

/// Position (m) and orientation (rad) of a part relative to the machine frame
/// (x forward, y left, z up). Readings stay in the part's own frame; viewers and apps apply this.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mount {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_default_to_zero() {
        let m: Mount = serde_json::from_str(r#"{"x": 0.2, "yaw": 1.5}"#).unwrap();
        assert_eq!(m, Mount { x: 0.2, yaw: 1.5, ..Mount::default() });
    }
}
