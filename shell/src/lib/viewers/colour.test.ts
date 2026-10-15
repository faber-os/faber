import { describe, expect, it } from 'vitest';
import { INTENSITY_BUCKETS, intensityBucket, intensityRamp } from './colour';

describe('intensityBucket', () => {
  it('maps 0..1 onto bucket numbers', () => {
    expect(intensityBucket(0, 4)).toBe(0);
    expect(intensityBucket(0.3, 4)).toBe(1);
    expect(intensityBucket(0.99, 4)).toBe(3);
    expect(intensityBucket(1, 4)).toBe(3);
  });

  it('clamps out-of-range and missing values', () => {
    expect(intensityBucket(-1, 4)).toBe(0);
    expect(intensityBucket(7, 4)).toBe(3);
    expect(intensityBucket(NaN, 4)).toBe(0);
  });
});

describe('intensityRamp', () => {
  it('gives one colour per bucket, getting lighter with intensity', () => {
    for (const theme of ['light', 'dark'] as const) {
      const ramp = intensityRamp(INTENSITY_BUCKETS, theme);
      expect(ramp).toHaveLength(INTENSITY_BUCKETS);
      const lightness = ramp.map((c) => Number(/oklch\(([\d.]+)/.exec(c)?.[1]));
      for (let i = 1; i < lightness.length; i++) {
        expect(lightness[i]).toBeGreaterThanOrEqual(lightness[i - 1]!);
      }
      expect(lightness.at(-1)!).toBeGreaterThan(lightness[0]!);
    }
  });
});
