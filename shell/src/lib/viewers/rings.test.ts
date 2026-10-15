import { describe, expect, it } from 'vitest';
import { ringRadii, ringStep } from './rings';

describe('ringStep', () => {
  it('picks the smallest 1-2-5 step that is at least the minimum spacing apart', () => {
    expect(ringStep(100, 60)).toBe(1); // 1 m = 100 px
    expect(ringStep(40, 60)).toBe(2); // 2 m = 80 px
    expect(ringStep(20, 60)).toBe(5); // 5 m = 100 px
    expect(ringStep(1000, 60)).toBe(0.1); // 10 cm = 100 px
    expect(ringStep(500, 60)).toBe(0.2);
  });

  it('uses round feet and inches when the units are imperial', () => {
    expect(ringStep(100, 60, 'imperial')).toBeCloseTo(0.6096, 6); // 2 ft = 61 px
    expect(ringStep(1000, 60, 'imperial')).toBeCloseTo(0.0762, 6); // 3 in = 76 px
  });
});

describe('ringRadii', () => {
  it('covers the canvas out to its farthest corner', () => {
    const size = { width: 400, height: 300 };
    const radii = ringRadii({ scale: 100, panX: 0, panY: 0 }, size, 1);
    // The farthest corner is 250 px = 2.5 m away, so rings at 1, 2 and 3 m.
    expect(radii).toEqual([1, 2, 3]);
  });

  it('follows the pan', () => {
    const radii = ringRadii({ scale: 100, panX: 200, panY: 150 }, { width: 400, height: 300 }, 1);
    expect(radii).toEqual([1, 2, 3, 4, 5]);
  });

  it('never returns an absurd number of rings', () => {
    expect(
      ringRadii({ scale: 1, panX: 0, panY: 0 }, { width: 4000, height: 4000 }, 0.001).length,
    ).toBeLessThanOrEqual(100);
  });
});
