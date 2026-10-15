import { describe, expect, it } from 'vitest';
import {
  BATTERY_PATTERN_KINDS,
  PATTERN_KINDS,
  isManualSensor,
  patternFor,
  patternKinds,
  patternValue,
  sensorRange,
  type SensorRange,
} from './simulation';
import type { PartInfo } from './types';

const FOUR: SensorRange = { min: 0, max: 4, quantity: 'length' };
const PACK: SensorRange = { min: 9, max: 12.6, quantity: 'voltage' };

describe('manual simulation patterns', () => {
  it('turns the slider value and the menu into a pattern', () => {
    expect(patternFor('steady', 1.5, FOUR)).toEqual({ kind: 'steady', value: 1.5 });
    expect(patternFor('sine', 1.5, FOUR)).toEqual({
      kind: 'sine',
      center: 1.5,
      amplitude: 0.4,
      periodS: 4,
    });
    expect(patternFor('noise', 1.5, FOUR)).toEqual({ kind: 'noise', center: 1.5, amplitude: 0.08 });
    expect(patternFor('step', 1.5, FOUR)).toEqual({
      kind: 'step',
      low: 1.5,
      high: 2.3,
      periodS: 4,
    });
  });

  it('keeps steps inside the range', () => {
    expect(patternFor('step', 3.9, FOUR)).toMatchObject({ high: 4 });
  });

  it('reads the slider value back from any pattern', () => {
    for (const kind of PATTERN_KINDS) expect(patternValue(patternFor(kind, 2, FOUR))).toBe(2);
    for (const kind of BATTERY_PATTERN_KINDS)
      expect(patternValue(patternFor(kind, 11, PACK))).toBe(11);
  });

  it('drains a battery from the slider to empty in two minutes', () => {
    expect(patternFor('drain', 12, PACK)).toEqual({ kind: 'drain', from: 12, to: 9, periodS: 120 });
  });
});

describe('manual sensors', () => {
  const p = (simulate: PartInfo['simulate'], capability: string) =>
    ({ connector: 'c', simulate, signals: [{ address: 'x/y', capability }] }) as PartInfo;

  it('are simulated by hand and measure something', () => {
    expect(isManualSensor(p('manual', 'distance@1'))).toBe(true);
    expect(isManualSensor(p('manual', 'laser-scan@1'))).toBe(true);
    expect(isManualSensor(p('world', 'distance@1'))).toBe(false);
    expect(isManualSensor(p(undefined, 'distance@1'))).toBe(false);
    expect(isManualSensor(p('manual', 'planar-base@1'))).toBe(false);
    expect(isManualSensor(p('manual', 'battery@1'))).toBe(true);
  });

  it('offer a battery a steady voltage or a drain, and other sensors the four shapes', () => {
    expect(patternKinds(p('manual', 'battery@1'))).toEqual(['steady', 'drain']);
    expect(patternKinds(p('manual', 'distance@1'))).toEqual(PATTERN_KINDS);
  });

  it('slide a battery from empty to full', () => {
    const pack = {
      connector: 'generic-battery',
      simulate: 'manual',
      signals: [
        {
          address: 'b/battery',
          capability: 'battery@1',
          info: {
            capability: 'battery@1',
            info: { chemistry: 'li-ion', cellsInSeries: 3, capacityAh: 2.2 },
          },
          battery: { empty: 9, full: 12.6, low: 10.5, critical: 9.9 },
        },
      ],
    } as PartInfo;
    expect(sensorRange(pack)).toEqual(PACK);
  });

  it("slide up to the sensor's own range", () => {
    const lidar = {
      connector: 'c',
      simulate: 'manual',
      signals: [
        {
          address: 'x/laser-scan',
          capability: 'laser-scan@1',
          info: { capability: 'laser-scan@1', info: { rangeMinM: 0.02, rangeMaxM: 12 } },
        },
      ],
    } as PartInfo;
    expect(sensorRange(lidar)).toEqual({ min: 0, max: 12, quantity: 'length' });
    expect(sensorRange(p('manual', 'distance@1'))).toEqual(FOUR);
  });
});
