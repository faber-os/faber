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
  function lightness(theme: 'light' | 'dark') {
    const ramp = intensityRamp(INTENSITY_BUCKETS, theme);
    expect(ramp).toHaveLength(INTENSITY_BUCKETS);
    return ramp.map((c) => Number(/oklch\(([\d.]+)/.exec(c)?.[1]));
  }

  it('gets brighter with intensity on the dark theme', () => {
    const l = lightness('dark');
    for (let i = 1; i < l.length; i++) expect(l[i]).toBeGreaterThanOrEqual(l[i - 1]!);
    expect(l.at(-1)!).toBeGreaterThan(l[0]!);
  });

  it('gets darker with intensity on the light theme, so strong returns stand out', () => {
    const l = lightness('light');
    for (let i = 1; i < l.length; i++) expect(l[i]).toBeLessThanOrEqual(l[i - 1]!);
    expect(l.at(-1)!).toBeLessThan(l[0]!);
  });
});
