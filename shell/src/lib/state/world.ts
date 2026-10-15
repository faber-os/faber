import { distanceInfo, laserScanInfo } from './partLimits';
import type { DistanceInfo, LaserScanInfo, Mount, PartInfo } from './types';

/**
 * Which parts live in the simulated world (docs/simulation.md, section 7): the parts with
 * `simulate = "world"`, and among them the base (the machine) and the sensors the World window
 * draws. Plain functions of the parts list.
 */

export function worldParts(parts: readonly PartInfo[] | null): PartInfo[] {
  return (parts ?? []).filter((p) => p.simulate === 'world');
}

/** The machine: the world part that is a drive base. The core allows only one. */
export function worldBase(parts: readonly PartInfo[] | null): PartInfo | null {
  return (
    worldParts(parts).find(
      (p) => p.state.kind !== 'invalid' && p.signals.some((s) => s.capability === 'planar-base@1'),
    ) ?? null
  );
}

export interface WorldSensor {
  part: string;
  address: string;
  mount: Mount;
}

export interface WorldScanner extends WorldSensor {
  info: LaserScanInfo | null;
}

export interface WorldDistance extends WorldSensor {
  info: DistanceInfo;
}

/** Every world `laser-scan` signal, with where its part is mounted. */
export function worldScanners(parts: readonly PartInfo[] | null): WorldScanner[] {
  return worldParts(parts).flatMap((p) =>
    p.signals
      .filter((s) => s.capability === 'laser-scan@1')
      .map((s) => ({ part: p.name, address: s.address, mount: p.mount, info: laserScanInfo(p) })),
  );
}

/** Every world `distance` signal, with its mount and its own range and field of view. */
export function worldDistances(parts: readonly PartInfo[] | null): WorldDistance[] {
  return worldParts(parts).flatMap((p) =>
    p.signals
      .filter((s) => s.capability === 'distance@1')
      .map((s) => ({ part: p.name, address: s.address, mount: p.mount, info: distanceInfo(p) })),
  );
}
