import { describe, expect, it } from 'vitest';
import {
  FALLBACK_BASE,
  FALLBACK_DISTANCE,
  baseInfo,
  distanceInfo,
  laserScanInfo,
  motorInfo,
} from './partLimits';
import type { PartInfo, SignalInfo } from './types';

const part = (signals: SignalInfo[]) => ({ name: 'p', connector: 'x', signals }) as PartInfo;

describe('part limits', () => {
  it("come from the part's own signal info, as its settings give them", () => {
    const p = part([
      {
        address: 'p/planar-base',
        capability: 'planar-base@1',
        info: {
          capability: 'planar-base@1',
          info: { ...FALLBACK_BASE, maxVelocity: { vx: 0.6, vy: 0.6, omega: 1.5 } },
        },
      },
    ]);
    expect(baseInfo(p).maxVelocity.vx).toBe(0.6);
  });

  it('fall back to the generic defaults when the part says nothing', () => {
    const bare = part([{ address: 'p/distance', capability: 'distance@1' }]);
    expect(baseInfo(bare)).toEqual(FALLBACK_BASE);
    expect(distanceInfo(bare)).toEqual(FALLBACK_DISTANCE);
    expect(motorInfo(bare).maxVelocityRadS).toBe(10);
    expect(laserScanInfo(bare)).toBeNull();
  });

  it('find the info of the capability asked for', () => {
    const p = part([
      {
        address: 'p/laser-scan',
        capability: 'laser-scan@1',
        info: { capability: 'laser-scan@1', info: { rangeMinM: 0.02, rangeMaxM: 12 } },
      },
      {
        address: 'p/distance',
        capability: 'distance@1',
        info: {
          capability: 'distance@1',
          info: { ...FALLBACK_DISTANCE, maxM: 2 },
        },
      },
    ]);
    expect(laserScanInfo(p)?.rangeMaxM).toBe(12);
    expect(distanceInfo(p).maxM).toBe(2);
  });
});
