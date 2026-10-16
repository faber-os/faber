//! `digital-out@1` 🔒: one digital output, such as a GPIO line driving a relay or an LED
//! (docs/capabilities.md, "GPIO level"). Leased, because an output can switch something that
//! moves; the lease lasts 1 s, and when it ends the output goes to its safe level: *inactive*.
//!
//! `level` is the logical level: `true` is *active*. An active-low part drives its pin low for
//! `true`, so the safe level (inactive) is a high pin there.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// The level as last set.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DigitalOutReading"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    pub level: bool,
}

/// Leased commands.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DigitalOutCommand"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    Set { level: bool },
}

impl Command {
    /// The safe state: inactive.
    pub const SAFE: Command = Command::Set { level: false };

    pub fn kind(&self) -> &'static str {
        match self {
            Command::Set { .. } => "set",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_tagged_by_kind() {
        let c: Command = serde_json::from_value(serde_json::json!({"kind": "set", "level": true})).unwrap();
        assert_eq!(c, Command::Set { level: true });
        assert_eq!(c.kind(), "set");
        assert_eq!(
            serde_json::to_value(Command::SAFE).unwrap(),
            serde_json::json!({"kind": "set", "level": false})
        );
        let r = Reading { t_us: 1, level: false };
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }
}
