import { describe, expect, it } from 'vitest';
import { scanStats, scanToPoints, type ScanInput } from './laserScan';

const even: ScanInput = {
  angleMin: 0,
  angleIncrement: Math.PI / 2,
  scanTime: 0.1,
  ranges: [1, 2, Infinity, 4],
  intensities: [0.1, 0.5, 0, 1],
};

describe('scanToPoints', () => {
  it('places even bins at angleMin + i × angleIncrement, x forward and y left', () => {
    const p = scanToPoints(even);
    expect(p.count).toBe(3);
    // i=0: angle 0 → straight ahead.
    expect(p.x[0]).toBeCloseTo(1);
    expect(p.y[0]).toBeCloseTo(0);
    // i=1: angle +90° → to the left.
    expect(p.x[1]).toBeCloseTo(0);
    expect(p.y[1]).toBeCloseTo(2);
    // i=2 is a no-return and is skipped, so the next point is i=3: angle 270° → to the right.
    expect(p.x[2]).toBeCloseTo(0);
    expect(p.y[2]).toBeCloseTo(-4);
  });

  it('keeps intensities with their points, skipping no-returns', () => {
    const p = scanToPoints(even);
    expect(p.hasIntensity).toBe(true);
    expect([...p.intensity.subarray(0, p.count)].map((v) => +v.toFixed(2))).toEqual([0.1, 0.5, 1]);
  });

  it('uses the per-point angles for raw points', () => {
    const p = scanToPoints({
      angleMin: 99,
      angleIncrement: 99,
      angles: [Math.PI, -Math.PI / 2],
      scanTime: 0.1,
      ranges: [3, 1],
    });
    expect(p.x[0]).toBeCloseTo(-3);
    expect(p.y[0]).toBeCloseTo(0);
    expect(p.x[1]).toBeCloseTo(0);
    expect(p.y[1]).toBeCloseTo(-1);
    expect(p.hasIntensity).toBe(false);
  });

  it('skips NaN, zero and negative ranges as well as Infinity', () => {
    const p = scanToPoints({
      angleMin: 0,
      angleIncrement: 0.1,
      scanTime: 0.1,
      ranges: [NaN, 0, -1, 1],
    });
    expect(p.count).toBe(1);
  });
});

describe('scanStats', () => {
  it('counts points with a return, no-returns and the revolution rate', () => {
    expect(scanStats(even)).toEqual({ points: 3, noReturn: 1, rateHz: 10 });
  });

  it('gives no rate when the scan time is unknown or zero', () => {
    expect(scanStats({ ...even, scanTime: 0 }).rateHz).toBeNull();
  });
});
