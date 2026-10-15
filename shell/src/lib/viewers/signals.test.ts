import { describe, expect, it } from 'vitest';
import { History, barFraction, gaugeFraction, plotRuns } from './signals';
import { formatSpeed, formatTurnRate } from './units';

describe('History', () => {
  it('keeps only the recent window, oldest first', () => {
    const h = new History(1_000);
    for (const tUs of [0, 400, 800, 1_200, 1_600]) h.push({ tUs, value: tUs, ok: true });
    expect(h.samples.map((s) => s.tUs)).toEqual([800, 1_200, 1_600]);
    expect(h.latest?.tUs).toBe(1_600);
  });

  it('starts afresh when the clock goes back (the part restarted)', () => {
    const h = new History(1_000);
    h.push({ tUs: 500, value: 1, ok: true });
    h.push({ tUs: 10, value: 2, ok: true });
    expect(h.samples.map((s) => s.value)).toEqual([2]);
  });
});

describe('bars and gauges', () => {
  it('places a value on a bar, clamped', () => {
    expect(barFraction(1, 4)).toBe(0.25);
    expect(barFraction(9, 4)).toBe(1);
    expect(barFraction(-1, 4)).toBe(0);
    expect(barFraction(1, 0)).toBe(0);
  });

  it('places a signed value on a gauge with zero in the middle', () => {
    expect(gaugeFraction(-0.3, 0.6)).toBe(-0.5);
    expect(gaugeFraction(2, 0.6)).toBe(1);
    expect(gaugeFraction(Number.NaN, 0.6)).toBe(0);
  });
});

describe('plotRuns', () => {
  it('draws the newest sample at the right edge and breaks the line at gaps', () => {
    const samples = [
      { tUs: 0, value: 0, ok: true },
      { tUs: 500, value: 2, ok: true },
      { tUs: 600, value: 4, ok: false },
      { tUs: 1_000, value: 4, ok: true },
    ];
    expect(plotRuns(samples, 1_000, 1_000, 4, 100, 40)).toEqual(['0,40 50,20', '100,0']);
  });
});

describe('speeds', () => {
  it('shows speeds in the chosen units, and turn rates in rad/s', () => {
    expect(formatSpeed(0.3, 'metric')).toBe('0.3 m/s');
    expect(formatSpeed(0.3048, 'imperial')).toBe('1 ft/s');
    expect(formatTurnRate(-1.5)).toBe('-1.5 rad/s');
  });
});
