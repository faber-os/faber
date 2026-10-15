//! Faber capability data types: the Rust mirror of `wit/capabilities/`.
//!
//! Only capabilities that have a connector are implemented here; the rest of the starter set is
//! specified in `docs/capabilities.md` and added when its first connector arrives.
//!
//! Wire conventions (docs/api.md): field names are camelCase, every reading carries `tUs`, and
//! a [`Reading`] is tagged with its capability id, e.g. `"laser-scan@1"`.

pub mod laser_scan;
mod mount;
mod non_finite;

pub use mount::Mount;

use serde::{Deserialize, Serialize};

/// Microseconds on the core's monotonic clock.
pub type Timestamp = u64;

/// How the router delivers a capability's signal (docs/capabilities.md, rule 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// A slow subscriber gets the newest reading and skips the rest.
    LatestValue,
    /// Events: no reading may be lost.
    EveryMessage,
}

/// Every capability the core knows, with its versioned id.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "laser-scan@1")]
    LaserScan,
}

impl Capability {
    pub const ALL: &'static [Capability] = &[Capability::LaserScan];

    /// The versioned id used in manifests, addresses and on the wire.
    pub fn id(self) -> &'static str {
        match self {
            Capability::LaserScan => "laser-scan@1",
        }
    }

    /// The id without its version, used as the default signal name.
    pub fn name(self) -> &'static str {
        match self {
            Capability::LaserScan => "laser-scan",
        }
    }

    pub fn delivery(self) -> Delivery {
        match self {
            Capability::LaserScan => Delivery::LatestValue,
        }
    }

    pub fn from_id(id: &str) -> Option<Capability> {
        Capability::ALL.iter().copied().find(|c| c.id() == id)
    }
}

/// One reading of any capability, tagged with the capability it belongs to.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "capability", content = "reading")]
pub enum Reading {
    #[serde(rename = "laser-scan@1")]
    LaserScan(laser_scan::Reading),
}

impl Reading {
    pub fn capability(&self) -> Capability {
        match self {
            Reading::LaserScan(_) => Capability::LaserScan,
        }
    }

    pub fn t_us(&self) -> Timestamp {
        match self {
            Reading::LaserScan(r) => r.t_us,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ids_round_trip() {
        for &c in Capability::ALL {
            assert_eq!(Capability::from_id(c.id()), Some(c));
            assert!(c.id().starts_with(c.name()));
        }
        assert_eq!(Capability::from_id("laser-scan@2"), None);
    }

    #[test]
    fn capability_serialises_as_its_id() {
        let json = serde_json::to_string(&Capability::LaserScan).unwrap();
        assert_eq!(json, "\"laser-scan@1\"");
    }

    #[test]
    fn reading_is_tagged_with_its_capability() {
        let r = Reading::LaserScan(laser_scan::Reading::even_bins(5, 0.0, 0.1, 0.1, vec![1.0]));
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["capability"], "laser-scan@1");
        assert_eq!(v["reading"]["tUs"], 5);
        assert_eq!(r.capability(), Capability::LaserScan);
        assert_eq!(r.t_us(), 5);
    }

    #[test]
    fn tagged_reading_round_trips_through_messagepack() {
        let r = Reading::LaserScan(laser_scan::Reading::even_bins(9, 0.0, 0.1, 0.1, vec![f32::INFINITY]));
        let bytes = rmp_serde::to_vec_named(&r).unwrap();
        assert_eq!(rmp_serde::from_slice::<Reading>(&bytes).unwrap(), r);
    }
}
