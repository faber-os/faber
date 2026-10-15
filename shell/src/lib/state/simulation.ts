import type { ConnectorInfo, PartInfo, Pattern } from './types';
import { canMove } from './driving';

/**
 * Manual simulated sensors (docs/api.md, "Manual simulated sensors"): one value from a slider,
 * shaped by a pattern. The Parts window shows one slider and one menu; these turn them into the
 * API's patterns and back.
 */

export type PatternKind = Pattern['kind'];
export const PATTERN_KINDS: readonly PatternKind[] = ['steady', 'sine', 'noise', 'step'];

/** How long one wave or one step takes (s). */
const PERIOD_S = 4;

/**
 * The pattern for a slider value. The value is the middle of a wave or noise, and the low half of
 * steps; the swing is a fifth of the sensor's range (a little for noise), kept inside 0..max.
 */
export function patternFor(kind: PatternKind, value: number, max: number): Pattern {
  const swing = max / 5;
  switch (kind) {
    case 'steady':
      return { kind, value };
    case 'sine':
      return { kind, center: value, amplitude: round(swing / 2), periodS: PERIOD_S };
    case 'noise':
      return { kind, center: value, amplitude: round(max / 50) };
    case 'step':
      return { kind, low: value, high: round(Math.min(max, value + swing)), periodS: PERIOD_S };
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
  }
}

/** A manual simulated sensor: its value is set by hand (actuators report their commands). */
export function isManualSensor(p: PartInfo): boolean {
  return (
    p.simulate === 'manual' &&
    !canMove(p) &&
    p.signals.some((s) => s.capability === 'distance@1' || s.capability === 'laser-scan@1')
  );
}

/** The slider's top: the sensor's range, from its connector's info. */
export function sensorMax(p: PartInfo, connectors: readonly ConnectorInfo[] | null): number {
  const c = connectors?.find((x) => x.id === p.connector);
  for (const provide of c?.provides ?? []) {
    const i = provide.info;
    if (i?.capability === 'distance@1') return i.info.maxM;
    if (i?.capability === 'laser-scan@1') return i.info.rangeMaxM;
  }
  return 4;
}

function round(v: number): number {
  return Math.round(v * 1000) / 1000;
}
