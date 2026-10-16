import { describe, expect, it } from 'vitest';
import { numericFields, polyline, seriesId, toCsv, valueRange } from './plotter';

describe('the signal plotter', () => {
  it('finds every number and true/false in a reading, without the time or lists', () => {
    expect(numericFields({ tUs: 5, celsius: 21.5, variance: 0.01 })).toEqual([
      { path: 'celsius', value: 21.5 },
      { path: 'variance', value: 0.01 },
    ]);
    expect(numericFields({ tUs: 1, level: true })).toEqual([{ path: 'level', value: 1 }]);
    expect(
      numericFields({ tUs: 1, twist: { vx: 0.5, omega: -1 }, ranges: [1, 2], frame: 'odom' }),
    ).toEqual([
      { path: 'twist.vx', value: 0.5 },
      { path: 'twist.omega', value: -1 },
    ]);
    expect(numericFields(null)).toEqual([]);
    expect(numericFields({ x: Infinity })).toEqual([]);
  });

  it('fits the values in view, and a flat line is not at the edge', () => {
    expect(valueRange([])).toEqual({ min: 0, max: 1 });
    expect(valueRange([[{ tUs: 0, value: 21.5, ok: true }]])).toEqual({ min: 20.5, max: 22.5 });
    const r = valueRange([
      [
        { tUs: 0, value: 0, ok: true },
        { tUs: 1, value: 10, ok: true },
      ],
    ]);
    expect(r).toEqual({ min: -0.5, max: 10.5 });
  });

  it('scrolls: the newest sample is at the right edge', () => {
    const line = polyline(
      [
        { tUs: 0, value: 0, ok: true },
        { tUs: 1_000_000, value: 10, ok: true },
      ],
      1_000_000,
      2_000_000,
      { min: 0, max: 10 },
      100,
      50,
    );
    expect(line).toBe('50,50 100,0');
  });

  it('saves a long-form CSV, oldest first, quoting what needs it', () => {
    const csv = toCsv([
      { tUs: 2_000_000, address: 'cabin_temp/temperature', field: 'celsius', value: 21.5 },
      { tUs: 1_500_000, address: 'a,b', field: 'x', value: -1 },
    ]);
    expect(csv).toBe(
      'time_s,signal,field,value\n1.500000,"a,b",x,-1\n2.000000,cabin_temp/temperature,celsius,21.5\n',
    );
    expect(seriesId({ address: 'p/s', field: 'v' })).toBe('p/s v');
  });
});
