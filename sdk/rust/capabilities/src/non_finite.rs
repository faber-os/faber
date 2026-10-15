//! Serde helper for float arrays that use `+inf` for "no value" (e.g. `laser-scan` ranges).
//!
//! JSON has no infinity, so human-readable formats write it as `null` and read `null` back as
//! `+inf`. Binary formats (MessagePack) keep the float as it is. See docs/api.md.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer>(values: &[f32], s: S) -> Result<S::Ok, S::Error> {
    if s.is_human_readable() {
        let mapped: Vec<Option<f32>> = values.iter().map(|v| v.is_finite().then_some(*v)).collect();
        mapped.serialize(s)
    } else {
        values.serialize(s)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f32>, D::Error> {
    if d.is_human_readable() {
        let mapped: Vec<Option<f32>> = Vec::deserialize(d)?;
        Ok(mapped.into_iter().map(|v| v.unwrap_or(f32::INFINITY)).collect())
    } else {
        Vec::deserialize(d)
    }
}
