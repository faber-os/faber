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
 * or a battery from empty to full (voltages the core works out from its chemistry and cells).
 */
export function sensorRange(p: PartInfo): SensorRange {
  for (const s of p.signals) {
    const i = s.info;
    if (i?.capability === 'distance@1') return { min: 0, max: i.info.maxM, quantity: 'length' };
    if (i?.capability === 'laser-scan@1')
      return { min: 0, max: i.info.rangeMaxM, quantity: 'length' };
    // Volts travel as 32-bit floats (12.6 arrives as 12.5999994…): a slider wants round ends.
    if (s.battery)
      return { min: round(s.battery.empty), max: round(s.battery.full), quantity: 'voltage' };
  }
  return { min: 0, max: 4, quantity: 'length' };
}

function round(v: number): number {
  return Math.round(v * 1000) / 1000;
}
