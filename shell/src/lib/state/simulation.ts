import type { PartInfo, Pattern } from './types';
import { canMove } from './driving';

/**
 * Manual simulated sensors (docs/api.md, "Manual simulated sensors"): one value from a slider,
 * shaped by a pattern. The Parts window shows one slider and one menu; these turn them into the
 * API's patterns and back.
 */

export type PatternKind = Pattern['kind'];
/** The patterns a distance or a laser scan offers. */
export const PATTERN_KINDS: readonly PatternKind[] = ['steady', 'sine', 'noise', 'step'];
/** A battery holds a voltage, or runs down. */
export const BATTERY_PATTERN_KINDS: readonly PatternKind[] = ['steady', 'drain'];

/** What the slider sets: its range, and what it measures (a length in m, or a voltage in V). */
export type SensorRange = { min: number; max: number; quantity: 'length' | 'voltage' };

/** How long one wave or one step takes (s). */
const PERIOD_S = 4;
/** How long a battery takes to run down from the slider's value to empty (s). */
export const DRAIN_S = 120;

/**
 * The pattern for a slider value. The value is the middle of a wave or noise, and the low half of
 * steps; the swing is a fifth of the sensor's range (a little for noise), kept inside the range.
 * A drain runs down from the value to the bottom of the range.
 */
export function patternFor(kind: PatternKind, value: number, range: SensorRange): Pattern {
  const span = range.max - range.min;
  const swing = span / 5;
  switch (kind) {
    case 'steady':
      return { kind, value };
    case 'sine':
      return { kind, center: value, amplitude: round(swing / 2), periodS: PERIOD_S };
    case 'noise':
      return { kind, center: value, amplitude: round(span / 50) };
    case 'step':
      return {
        kind,
        low: value,
        high: round(Math.min(range.max, value + swing)),
        periodS: PERIOD_S,
      };
    case 'drain':
      return { kind, from: value, to: range.min, periodS: DRAIN_S };
  }
}

/** The slider value a pattern stands for. */
export function patternValue(p: Pattern): number {
  switch (p.kind) {
    case 'steady':
      return p.value;
    case 'sine':
    case 'noise':
      return p.center;
    case 'step':
      return p.low;
    case 'drain':
      return p.from;
  }
}

const MANUAL = new Set(['distance@1', 'laser-scan@1', 'battery@1']);

/** A manual simulated sensor: its value is set by hand (actuators report their commands). */
export function isManualSensor(p: PartInfo): boolean {
  return p.simulate === 'manual' && !canMove(p) && p.signals.some((s) => MANUAL.has(s.capability));
}

/** The patterns the menu offers for this sensor. */
export function patternKinds(p: PartInfo): readonly PatternKind[] {
  return p.signals.some((s) => s.capability === 'battery@1')
    ? BATTERY_PATTERN_KINDS
    : PATTERN_KINDS;
}

/**
 * The slider's range, from the sensor's own info (its settings): 0 to the furthest it can measure,
 * or a battery from empty to full.
 */
export function sensorRange(p: PartInfo): SensorRange {
  for (const s of p.signals) {
    const i = s.info;
    if (i?.capability === 'distance@1') return { min: 0, max: i.info.maxM, quantity: 'length' };
    if (i?.capability === 'laser-scan@1')
      return { min: 0, max: i.info.rangeMaxM, quantity: 'length' };
    if (i?.capability === 'battery@1') {
      const { empty, full } = batteryVolts(i.info.chemistry, i.info.cellsInSeries);
      return { min: empty, max: full, quantity: 'voltage' };
    }
  }
  return { min: 0, max: 4, quantity: 'length' };
}

/**
 * One cell's resting voltage when empty and when full: the typical values the core's battery
 * capability uses (sdk/rust/capabilities/src/battery.rs, `Chemistry::cell`).
 */
const CELL_VOLTS: Record<string, { empty: number; full: number }> = {
  'li-ion': { empty: 3.0, full: 4.2 },
  lipo: { empty: 3.0, full: 4.2 },
  lifepo4: { empty: 2.5, full: 3.4 },
  nimh: { empty: 1.0, full: 1.4 },
  'lead-acid': { empty: 1.75, full: 2.122 },
};

/** A pack's resting voltage when empty and when full. */
export function batteryVolts(chemistry: string, cells: number): { empty: number; full: number } {
  const c = CELL_VOLTS[chemistry] ?? { empty: 3.0, full: 4.2 };
  return { empty: round(c.empty * cells), full: round(c.full * cells) };
}

function round(v: number): number {
  return Math.round(v * 1000) / 1000;
}
