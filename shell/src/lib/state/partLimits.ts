import type {
  DistanceInfo,
  Info,
  LaserScanInfo,
  MotorInfo,
  PartInfo,
  PlanarBaseInfo,
} from './types';

/**
 * A part's limits (its capability info: range, top speed, …), as its own settings give them. The
 * core sends them with each signal (`SignalInfo.info`); the core also clamps every command to
 * them, so these are what the part really does.
 */
export function partInfo(part: PartInfo): Info[] {
  return part.signals.flatMap((s) => (s.info ? [s.info] : []));
}

export function baseInfo(part: PartInfo): PlanarBaseInfo {
  for (const i of partInfo(part)) if (i.capability === 'planar-base@1') return i.info;
  return FALLBACK_BASE;
}

export function motorInfo(part: PartInfo): MotorInfo {
  for (const i of partInfo(part)) if (i.capability === 'motor@1') return i.info;
  return FALLBACK_MOTOR;
}

export function distanceInfo(part: PartInfo): DistanceInfo {
  for (const i of partInfo(part)) if (i.capability === 'distance@1') return i.info;
  return FALLBACK_DISTANCE;
}

/** A laser scanner's range, or null when the part doesn't say. */
export function laserScanInfo(part: PartInfo): LaserScanInfo | null {
  for (const i of partInfo(part)) if (i.capability === 'laser-scan@1') return i.info;
  return null;
}

/**
 * Limits to use when a part doesn't say (its settings can't be read): the generic connectors'
 * own defaults.
 */
export const FALLBACK_BASE: PlanarBaseInfo = {
  holonomic: true,
  maxVelocity: { vx: 0.5, vy: 0.5, omega: 1.5 },
  maxAcceleration: { vx: 1, vy: 1, omega: 1.5 },
  builtFrom: [],
};

export const FALLBACK_MOTOR: MotorInfo = {
  kind: 'dc',
  modes: { velocity: true, position: false, duty: true },
  maxVelocityRadS: 10,
};

export const FALLBACK_DISTANCE: DistanceInfo = {
  minM: 0.02,
  maxM: 4,
  fieldOfViewRad: 0.26,
  kind: 'ultrasonic',
};
