//! `battery@1`: a battery's voltage and how much is left (ROS BatteryState,
//! docs/capabilities.md, "Power and control").
//!
//! `level` is what a user acts on: `ok`, `low` (finish up and charge) or `critical` (stop now,
//! before the cells are damaged). It comes from per-chemistry thresholds ([`Chemistry::cell`]),
//! with a little hysteresis so a voltage that wobbles around a threshold doesn't flip the level
//! back and forth (the core raises a notification on every change).
//!
//! The numbers are **typical datasheet values** for resting cells, not measurements of any one
//! battery: Li-ion and LiPo cells are full at 4.2 V and should not go below about 3.0 V; a
//! LiFePO4 cell is full at about 3.4 V resting and empty at 2.5 V; NiMH is 1.0–1.4 V; a lead-acid
//! cell (a 12 V battery has six) is 1.75–2.12 V. A battery under load reads lower than at rest, so
//! a robot that drives hard may see `low` a little early. `charge` estimated from the voltage
//! ([`Chemistry::charge_at`]) is a rough guide only; a battery monitor that counts current
//! (a fuel gauge) does better and can report its own `charge`.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// How urgent it is to charge.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "BatteryLevel"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Ok,
    /// Finish what you are doing and charge soon.
    Low,
    /// Stop and charge now: going on can damage the cells.
    Critical,
}

impl Level {
    pub const ALL: &'static [Level] = &[Level::Ok, Level::Low, Level::Critical];

    pub fn id(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Low => "low",
            Level::Critical => "critical",
        }
    }
}

/// What the cells are made of. It sets the voltage thresholds and the charge curve.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "BatteryChemistry"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Chemistry {
    /// Cylindrical lithium-ion cells such as 18650s (NMC/NCA).
    LiIon,
    /// Lithium polymer pouch packs, as in RC models.
    Lipo,
    /// Lithium iron phosphate.
    Lifepo4,
    /// Nickel-metal hydride (AA rechargeables, older RC packs).
    Nimh,
    /// Sealed lead-acid (SLA/AGM), 2 V per cell.
    LeadAcid,
}

/// One cell's voltages (V) for a chemistry: typical datasheet values for a resting cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellVoltages {
    /// Fully charged, at rest.
    pub full: f32,
    /// Below this the level is `low`.
    pub low: f32,
    /// Below this the level is `critical`.
    pub critical: f32,
    /// Empty: the bottom of the charge curve.
    pub empty: f32,
    /// How far above a threshold the voltage must climb before the level gets better again.
    pub hysteresis: f32,
}

/// Charge (0–1) at resting cell voltages, lowest first; straight lines in between.
type Curve = &'static [(f32, f32)];

const LITHIUM_CURVE: Curve = &[
    (3.00, 0.00),
    (3.30, 0.05),
    (3.50, 0.10),
    (3.60, 0.20),
    (3.70, 0.40),
    (3.80, 0.55),
    (3.90, 0.70),
    (4.00, 0.80),
    (4.10, 0.90),
    (4.20, 1.00),
];
const LIFEPO4_CURVE: Curve = &[
    (2.50, 0.00),
    (3.00, 0.09),
    (3.20, 0.17),
    (3.25, 0.30),
    (3.27, 0.40),
    (3.30, 0.70),
    (3.32, 0.90),
    (3.40, 1.00),
];
const NIMH_CURVE: Curve =
    &[(1.00, 0.00), (1.10, 0.10), (1.20, 0.50), (1.25, 0.70), (1.30, 0.90), (1.40, 1.00)];
const LEAD_ACID_CURVE: Curve = &[
    (1.750, 0.00),
    (1.918, 0.10),
    (1.943, 0.20),
    (1.968, 0.30),
    (1.993, 0.40),
    (2.017, 0.50),
    (2.040, 0.60),
    (2.062, 0.70),
    (2.083, 0.80),
    (2.103, 0.90),
    (2.122, 1.00),
];

impl Chemistry {
    pub const ALL: &'static [Chemistry] =
        &[Chemistry::LiIon, Chemistry::Lipo, Chemistry::Lifepo4, Chemistry::Nimh, Chemistry::LeadAcid];

    pub fn id(self) -> &'static str {
        match self {
            Chemistry::LiIon => "li-ion",
            Chemistry::Lipo => "lipo",
            Chemistry::Lifepo4 => "lifepo4",
            Chemistry::Nimh => "nimh",
            Chemistry::LeadAcid => "lead-acid",
        }
    }

    pub fn from_id(id: &str) -> Option<Chemistry> {
        Chemistry::ALL.iter().copied().find(|c| c.id() == id)
    }

    /// One cell's thresholds. Li-ion and LiPo: low 3.5 V (about 10 % left), critical 3.3 V
    /// (about 5 %), so a 3S pack is low at 10.5 V and critical at 9.9 V.
    pub fn cell(self) -> CellVoltages {
        match self {
            Chemistry::LiIon | Chemistry::Lipo => {
                CellVoltages { full: 4.2, low: 3.5, critical: 3.3, empty: 3.0, hysteresis: 0.05 }
            }
            Chemistry::Lifepo4 => {
                CellVoltages { full: 3.4, low: 3.0, critical: 2.8, empty: 2.5, hysteresis: 0.03 }
            }
            Chemistry::Nimh => {
                CellVoltages { full: 1.4, low: 1.1, critical: 1.0, empty: 1.0, hysteresis: 0.02 }
            }
            // A 12 V battery: low at 11.8 V (about 30 %), critical at 11.5 V (about 10 %).
            Chemistry::LeadAcid => CellVoltages {
                full: 2.122,
                low: 11.8 / 6.0,
                critical: 11.5 / 6.0,
                empty: 1.75,
                hysteresis: 0.02,
            },
        }
    }

    fn curve(self) -> Curve {
        match self {
            Chemistry::LiIon | Chemistry::Lipo => LITHIUM_CURVE,
            Chemistry::Lifepo4 => LIFEPO4_CURVE,
            Chemistry::Nimh => NIMH_CURVE,
            Chemistry::LeadAcid => LEAD_ACID_CURVE,
        }
    }

    /// Estimated charge (0–1) of a resting cell at `cell_volts`.
    pub fn charge_at(self, cell_volts: f32) -> f32 {
        interpolate(self.curve(), cell_volts, |p| p.0, |p| p.1)
    }

    /// The resting cell voltage at `charge` (0–1): the inverse of [`Chemistry::charge_at`].
    pub fn cell_volts_at(self, charge: f32) -> f32 {
        interpolate(self.curve(), charge, |p| p.1, |p| p.0)
    }
}

/// Piecewise-linear lookup in a curve that rises in both columns; clamps at the ends.
fn interpolate(curve: Curve, x: f32, from: fn(&(f32, f32)) -> f32, to: fn(&(f32, f32)) -> f32) -> f32 {
    let first = curve.first().expect("curves have points");
    let last = curve.last().expect("curves have points");
    if !x.is_finite() || x <= from(first) {
        return to(first);
    }
    if x >= from(last) {
        return to(last);
    }
    for pair in curve.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if x <= from(b) {
            let f = (x - from(a)) / (from(b) - from(a));
            return to(a) + f * (to(b) - to(a));
        }
    }
    to(last)
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "BatteryReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// The whole pack's voltage.
    pub volts: f32,
    /// Current (A): negative while discharging, positive while charging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub amps: Option<f32>,
    /// How full it is, 0–1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub charge: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub celsius: Option<f32>,
    /// Each cell's voltage, when a balance lead or battery monitor reports them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub cell_volts: Option<Vec<f32>>,
    pub level: Level,
}

impl Reading {
    /// Just a voltage and its level: what a voltage divider on an analog pin gives.
    pub fn volts(t_us: Timestamp, volts: f32, level: Level) -> Reading {
        Reading { t_us, volts, amps: None, charge: None, celsius: None, cell_volts: None, level }
    }
}

/// What a battery is, reported once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "BatteryInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub chemistry: Chemistry,
    /// How many cells are in series (the `3` in "3S6P").
    pub cells_in_series: u8,
    /// The pack's capacity (Ah), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub capacity_ah: Option<f32>,
}

impl Info {
    fn cells(&self) -> f32 {
        f32::from(self.cells_in_series.max(1))
    }

    /// The pack's voltage when full and when empty, at rest.
    pub fn range_volts(&self) -> (f32, f32) {
        let c = self.chemistry.cell();
        (c.empty * self.cells(), c.full * self.cells())
    }

    /// The pack's `low` and `critical` thresholds (V).
    pub fn thresholds_volts(&self) -> (f32, f32) {
        let c = self.chemistry.cell();
        (c.low * self.cells(), c.critical * self.cells())
    }

    /// The level at `volts` for the whole pack. `previous` is the level reported last: a level
    /// gets worse as soon as the voltage drops below a threshold, but only gets better once it is
    /// a hysteresis margin above it.
    pub fn level(&self, volts: f32, previous: Option<Level>) -> Level {
        let c = self.chemistry.cell();
        let n = self.cells();
        let at = |margin: f32| {
            if volts.is_nan() || volts < (c.critical + margin) * n {
                Level::Critical
            } else if volts < (c.low + margin) * n {
                Level::Low
            } else {
                Level::Ok
            }
        };
        let raw = at(0.0);
        match previous {
            Some(p) if raw < p => at(c.hysteresis).min(p),
            _ => raw,
        }
    }

    /// Estimated charge (0–1) of the resting pack at `volts`.
    pub fn charge_at(&self, volts: f32) -> f32 {
        self.chemistry.charge_at(volts / self.cells())
    }

    /// The resting pack voltage at `charge` (0–1).
    pub fn volts_at(&self, charge: f32) -> f32 {
        self.chemistry.cell_volts_at(charge) * self.cells()
    }

    /// A reading for a pack that only measures its voltage: level and an estimated charge.
    pub fn reading(&self, t_us: Timestamp, volts: f32, previous: Option<Level>) -> Reading {
        Reading {
            charge: Some(self.charge_at(volts)),
            ..Reading::volts(t_us, volts, self.level(volts, previous))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sennep() -> Info {
        Info { chemistry: Chemistry::LiIon, cells_in_series: 3, capacity_ah: Some(15.0) }
    }

    #[test]
    fn a_3s_li_ion_pack_is_low_at_10_5_and_critical_at_9_9_volts() {
        let (low, critical) = sennep().thresholds_volts();
        assert!((low - 10.5).abs() < 1e-4 && (critical - 9.9).abs() < 1e-4, "{low} {critical}");
        assert_eq!(sennep().level(12.0, None), Level::Ok);
        assert_eq!(sennep().level(10.5, None), Level::Ok, "at the threshold is still ok");
        assert_eq!(sennep().level(10.49, None), Level::Low);
        assert_eq!(sennep().level(9.89, None), Level::Critical);
        assert_eq!(sennep().level(f32::NAN, None), Level::Critical, "no number is never ok");
        let (empty, full) = sennep().range_volts();
        assert!((empty - 9.0).abs() < 1e-4 && (full - 12.6).abs() < 1e-4, "{empty} {full}");
    }

    #[test]
    fn a_level_gets_better_only_past_a_margin() {
        let b = sennep();
        // Hysteresis 0.05 V per cell: 0.15 V for 3S.
        assert_eq!(b.level(10.55, Some(Level::Low)), Level::Low, "a wobble above 10.5 V stays low");
        assert_eq!(b.level(10.66, Some(Level::Low)), Level::Ok);
        assert_eq!(b.level(10.0, Some(Level::Critical)), Level::Critical);
        assert_eq!(b.level(10.1, Some(Level::Critical)), Level::Low);
        assert_eq!(b.level(11.0, Some(Level::Critical)), Level::Ok);
        assert_eq!(b.level(10.4, Some(Level::Ok)), Level::Low, "getting worse needs no margin");
        assert_eq!(b.level(9.0, Some(Level::Low)), Level::Critical);
    }

    #[test]
    fn every_chemistry_has_ordered_thresholds_and_a_rising_curve() {
        for &c in Chemistry::ALL {
            let v = c.cell();
            assert!(v.empty <= v.critical && v.critical < v.low && v.low < v.full, "{c:?}");
            assert!(v.hysteresis > 0.0 && v.hysteresis < v.low - v.critical, "{c:?}");
            for pair in c.curve().windows(2) {
                assert!(pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1, "{c:?} rises");
            }
            assert_eq!(c.charge_at(v.full + 1.0), 1.0);
            assert_eq!(c.charge_at(0.0), 0.0);
            assert_eq!(Chemistry::from_id(c.id()), Some(c));
            assert_eq!(serde_json::to_value(c).unwrap(), c.id());
            for charge in [0.0, 0.25, 0.5, 0.9, 1.0] {
                assert!((c.charge_at(c.cell_volts_at(charge)) - charge).abs() < 1e-4, "{c:?} {charge}");
            }
        }
    }

    #[test]
    fn charge_follows_the_curve_between_its_points() {
        assert!((Chemistry::LiIon.charge_at(3.75) - 0.475).abs() < 1e-4);
        assert!((sennep().charge_at(12.6) - 1.0).abs() < 1e-6);
        assert!((sennep().volts_at(0.1) - 10.5).abs() < 1e-4);
    }

    #[test]
    fn json_shape_round_trips() {
        let r = Reading {
            amps: Some(-1.5),
            cell_volts: Some(vec![3.9, 3.91, 3.89]),
            ..sennep().reading(4, 11.7, None)
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["level"], "ok");
        assert_eq!(v["cellVolts"][1], serde_json::json!(3.91f32));
        assert!(v.get("celsius").is_none(), "missing values are left out");
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
        let i = serde_json::to_value(sennep()).unwrap();
        assert_eq!(i, serde_json::json!({"chemistry": "li-ion", "cellsInSeries": 3, "capacityAh": 15.0}));
        for l in Level::ALL {
            assert_eq!(serde_json::to_value(l).unwrap(), l.id());
        }
    }
}
