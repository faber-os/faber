import { describe, expect, it } from 'vitest';
import {
  MAX_WORLD_SCALE,
  Trail,
  coneLength,
  conePolygon,
  coneTone,
  fitBounds,
  footprintCorners,
  mountPose,
  panBy,
  pointsToWorld,
  screenToWorld,
  worldToScreen,
  worldTransform,
  zoomAt,
  type Camera,
} from './worldView';

const size = { width: 400, height: 300 };
const cam: Camera = { scale: 50, cx: 2, cy: 1 };
const close = (a: number[], b: number[]) => a.forEach((v, i) => expect(v).toBeCloseTo(b[i]!, 6));

describe('the world top view', () => {
  it('puts east to the right and north up, the camera centre in the middle', () => {
    expect(worldToScreen(2, 1, cam, size)).toEqual([200, 150]);
    expect(worldToScreen(3, 1, cam, size)).toEqual([250, 150]);
    expect(worldToScreen(2, 2, cam, size)).toEqual([200, 100]);
    close(Object.values(screenToWorld(250, 100, cam, size)), [3, 2]);
  });

  it('has a canvas transform that agrees with worldToScreen', () => {
    const [a, , , d, e, f] = worldTransform(cam, size, 2);
    const [sx, sy] = worldToScreen(5, -3, cam, size);
    close([(a * 5 + e) / 2, (d * -3 + f) / 2], [sx, sy]);
  });

  it('zooms around the pointer and pans with the drag', () => {
    const z = zoomAt(cam, 2, 300, 50, size);
    close(
      worldToScreen(
        ...(Object.values(screenToWorld(300, 50, cam, size)) as [number, number]),
        z,
        size,
      ),
      [300, 50],
    );
    expect(z.scale).toBe(100);
    expect(zoomAt(cam, 1e6, 0, 0, size).scale).toBe(MAX_WORLD_SCALE);
    const p = panBy(cam, 50, -50);
    // Dragging right by a metre's worth of pixels brings the point a metre west under the centre.
    close([p.cx, p.cy], [1, 0]);
  });

  it('fits a whole world in view', () => {
    const c = fitBounds({ min: [0, 0], max: [30, 20] }, size);
    expect([c.cx, c.cy]).toEqual([15, 10]);
    const [left] = worldToScreen(0, 10, c, size);
    const [, top] = worldToScreen(15, 20, c, size);
    expect(left).toBeGreaterThanOrEqual(0);
    expect(top).toBeGreaterThanOrEqual(0);
  });
});

describe('machine frame to world', () => {
  const pose = { x: 1, y: 2, yaw: Math.PI / 2 };

  it("turns the footprint with the machine, counter-clockwise from front-left like the core's", () => {
    const c = footprintCorners(pose, { length: 0.8, width: 0.6 });
    // Facing north: front-left is north-west of the centre.
    close(c[0]!, [0.7, 2.4]);
    close(c[2]!, [1.3, 1.6]);
  });

  it('places a part by its mount and moves its points with it', () => {
    const sensor = mountPose(pose, { x: 0.2, y: 0, yaw: Math.PI / 2 });
    close([sensor.x, sensor.y, sensor.yaw], [1, 2.2, Math.PI]);
    // A point 1 m in front of a sensor that faces west.
    const w = pointsToWorld(sensor, [1], [0], 1);
    close([w.x[0]!, w.y[0]!], [0, 2.2]);
  });

  it('draws a cone from the sensor to what it measured, coloured by its status', () => {
    const cone = conePolygon({ x: 0, y: 0, yaw: 0 }, Math.PI / 2, 2, 2);
    expect(cone).toHaveLength(4);
    close(cone[0]!, [0, 0]);
    close(cone[1]!, [Math.SQRT2, -Math.SQRT2]);
    close(cone[3]!, [Math.SQRT2, Math.SQRT2]);
    const info = { minM: 0.02, maxM: 4 };
    expect(coneLength({ meters: 1.5, status: 'ok' }, info)).toBe(1.5);
    expect(coneLength({ meters: Infinity, status: 'too-far' }, info)).toBe(4);
    expect(coneLength({ meters: 0.02, status: 'too-close' }, info)).toBe(0.02);
    expect(coneLength(null, info)).toBe(4);
    expect(['ok', 'too-close', 'too-far', 'no-target', 'error'].map(coneTone)).toEqual([
      'ok',
      'near',
      'none',
      'none',
      'bad',
    ]);
  });
});

describe('the trail', () => {
  it('keeps the last few seconds and finds the pose at a moment', () => {
    const trail = new Trail(1_000_000);
    for (let i = 0; i <= 30; i++) trail.push(i * 100_000, { x: i * 0.1, y: 0, yaw: 0 });
    expect(trail.poses).toHaveLength(11);
    expect(trail.poses[0]!.x).toBeCloseTo(2);
    expect(trail.poseAt(2_550_000)!.x).toBeCloseTo(2.55);
    expect(trail.poseAt(0)!.x).toBeCloseTo(2, 6);
    expect(trail.poseAt(9e9)!.x).toBeCloseTo(3);
  });

  it('turns the short way round between two poses', () => {
    const trail = new Trail(1e6);
    trail.push(0, { x: 0, y: 0, yaw: 3.1 });
    trail.push(100, { x: 0, y: 0, yaw: -3.1 });
    expect(Math.abs(trail.poseAt(50)!.yaw)).toBeGreaterThan(3.1);
  });

  it('starts afresh when the clock goes back (a reset or a restarted core)', () => {
    const trail = new Trail(1e6);
    trail.push(500, { x: 5, y: 0, yaw: 0 });
    trail.push(100, { x: 0, y: 0, yaw: 0 });
    expect(trail.poses).toEqual([{ x: 0, y: 0, yaw: 0 }]);
  });
});
