import { describe, expect, it } from 'vitest';
import { PATTERN_KINDS, isManualSensor, patternFor, patternValue, sensorMax } from './simulation';
import type { ConnectorInfo, PartInfo } from './types';

describe('manual simulation patterns', () => {
  it('turns the slider value and the menu into a pattern', () => {
    expect(patternFor('steady', 1.5, 4)).toEqual({ kind: 'steady', value: 1.5 });
    expect(patternFor('sine', 1.5, 4)).toEqual({
      kind: 'sine',
      center: 1.5,
      amplitude: 0.4,
      periodS: 4,
    });
    expect(patternFor('noise', 1.5, 4)).toEqual({ kind: 'noise', center: 1.5, amplitude: 0.08 });
    expect(patternFor('step', 1.5, 4)).toEqual({ kind: 'step', low: 1.5, high: 2.3, periodS: 4 });
  });

  it('keeps steps inside the range', () => {
    expect(patternFor('step', 3.9, 4)).toMatchObject({ high: 4 });
  });

  it('reads the slider value back from any pattern', () => {
    for (const kind of PATTERN_KINDS) expect(patternValue(patternFor(kind, 2, 4))).toBe(2);
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
  });

  it("slide up to the sensor's range", () => {
    const lidar: ConnectorInfo = {
      id: 'c',
      name: 'c',
      version: '1',
      kind: 'built-in',
      port: { kind: 'bridge' },
      provides: [
        {
          name: 'laser-scan',
          capability: 'laser-scan@1',
          info: { capability: 'laser-scan@1', info: { rangeMinM: 0.02, rangeMaxM: 12 } },
        },
      ],
    };
    expect(sensorMax(p('manual', 'laser-scan@1'), [lidar])).toBe(12);
    expect(sensorMax(p('manual', 'distance@1'), [])).toBe(4);
  });
});
