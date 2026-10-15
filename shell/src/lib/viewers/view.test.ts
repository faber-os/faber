import { describe, expect, it } from 'vitest';
import {
  MAX_SCALE,
  MIN_SCALE,
  fitView,
  panBy,
  screenToWorld,
  worldToScreen,
  zoomAt,
  zoomCentre,
  type View,
} from './view';

const size = { width: 400, height: 300 };
const view: View = { scale: 100, panX: 0, panY: 0 };

describe('worldToScreen', () => {
  it('puts the origin in the middle', () => {
    expect(worldToScreen(0, 0, view, size)).toEqual([200, 150]);
  });

  it('draws x forward as up and y left as left', () => {
    expect(worldToScreen(1, 0, view, size)).toEqual([200, 50]);
    expect(worldToScreen(0, 1, view, size)).toEqual([100, 150]);
  });

  it('applies the pan offset in screen pixels', () => {
    expect(worldToScreen(0, 0, { ...view, panX: 10, panY: -20 }, size)).toEqual([210, 130]);
  });

  it('is undone by screenToWorld', () => {
    const v = { scale: 37, panX: 12, panY: -7 };
    const [sx, sy] = worldToScreen(1.25, -0.5, v, size);
    const w = screenToWorld(sx, sy, v, size);
    expect(w.x).toBeCloseTo(1.25);
    expect(w.y).toBeCloseTo(-0.5);
  });
});

describe('zoomAt', () => {
  it('keeps the world point under the cursor in place', () => {
    const before = screenToWorld(300, 60, view, size);
    const z = zoomAt(view, 2, 300, 60, size);
    expect(z.scale).toBe(200);
    const after = screenToWorld(300, 60, z, size);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it('stays within the scale limits', () => {
    expect(zoomAt(view, 1e9, 0, 0, size).scale).toBe(MAX_SCALE);
    expect(zoomAt(view, 1e-9, 0, 0, size).scale).toBe(MIN_SCALE);
  });

  it('zooms around the centre for the buttons', () => {
    const z = zoomCentre({ ...view, panX: 40 }, 0.5, size);
    expect(z.scale).toBe(50);
    expect(z.panX).toBe(20);
  });
});

describe('panBy and fitView', () => {
  it('pans by screen pixels', () => {
    expect(panBy(view, 5, -3)).toEqual({ scale: 100, panX: 5, panY: -3 });
  });

  it('fits a range into the smaller side with a margin', () => {
    const v = fitView(6, size);
    expect(v.panX).toBe(0);
    expect(v.panY).toBe(0);
    // 6 m from the centre fits into half of the 300 px height.
    expect(6 * v.scale).toBeLessThan(150);
    expect(6 * v.scale).toBeGreaterThan(120);
  });
});
