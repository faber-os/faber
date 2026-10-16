import { describe, expect, it } from 'vitest';
import {
  combine,
  driveKey,
  driveValue,
  keysToStick,
  nearness,
  padToStick,
  poseOf,
  readLimits,
  ringSegments,
  scanPoints,
  stopZone,
  topView,
  velocityText,
} from '../../../../examples/apps/sennep/ui/view.js';

/**
 * The SENNEP example app's window (examples/apps/sennep/ui): its pure parts, tested here because
 * this is where the example apps' JavaScript is tested (it has no build or package of its own).
 */
describe('the SENNEP window', () => {
  it('puts a scan’s returns in the machine frame, from where the LiDAR sits', () => {
    const scan = { angleMin: -Math.PI, angleIncrement: Math.PI / 2, ranges: [1, null, 1, 0.5] };
    const pts = scanPoints(scan, { x: 0.2, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 });
    expect(pts).toHaveLength(3); // no return: left out
    expect(pts[0].x).toBeCloseTo(-0.8);
    expect(pts[1].x).toBeCloseTo(1.2);
    expect(pts[2]).toMatchObject({ x: expect.closeTo(0.2), y: expect.closeTo(0.5) });
    const turned = scanPoints(
      { ...scan, ranges: [null, null, 1, null] },
      { x: 0, y: 0, yaw: Math.PI / 2 },
    );
    expect(turned[0].y).toBeCloseTo(1); // a LiDAR mounted turned left sees "ahead" to the left
    expect(scanPoints({ angles: [0], ranges: [Infinity] }, undefined)).toEqual([]);
  });

  it('draws forward up and left left', () => {
    const v = topView(400, 4);
    expect(v.scale).toBe(50);
    expect(v.toScreen({ x: 1, y: 0 })).toEqual({ sx: 200, sy: 150 });
    expect(v.toScreen({ x: 0, y: 1 })).toEqual({ sx: 150, sy: 200 });
    expect(stopZone({ length: 0.8, width: 0.65 }, 0.35)).toEqual({
      halfLength: 0.75,
      halfWidth: 0.675,
      radius: 0.35,
    });
  });

  it('reads the keys and the pad like the Drive window', () => {
    expect(['KeyW', 'ArrowLeft', 'KeyQ', 'Space', 'KeyX'].map(driveKey)).toEqual([
      'forward',
      'left',
      'turnLeft',
      'stop',
      null,
    ]);
    const diagonal = keysToStick(new Set(['forward', 'left']));
    expect(Math.hypot(diagonal.x, diagonal.y)).toBeCloseTo(1);
    expect(keysToStick(new Set(['turnRight']))).toEqual({ x: 0, y: 0, turn: -1 });
    expect(padToStick(0, -1)).toEqual({ x: 1, y: 0, turn: 0 }); // up is forward
    expect(padToStick(1, 0)).toEqual({ x: 0, y: -1, turn: 0 }); // right is right
    expect(padToStick(0.01, 0.02)).toEqual({ x: 0, y: 0, turn: 0 }); // a resting finger
    expect(combine({ x: 1, y: 0, turn: 0 }, { x: 1, y: 0, turn: 1 })).toEqual({
      x: 1,
      y: 0,
      turn: 1,
    });
    expect(driveValue(7, { x: 1, y: -0.33333, turn: 0 })).toBe('7 1 -0.333 0');
  });

  it('cuts the proximity ring by where each sensor looks, without overlaps', () => {
    const at = (yaw) => ({ x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw });
    const corners = ringSegments([at(Math.PI / 4), at(-Math.PI / 4), at((3 * Math.PI) / 4)]);
    expect(corners.map((s) => s.yaw)).toEqual([Math.PI / 4, -Math.PI / 4, (3 * Math.PI) / 4]);
    expect(corners[0].to - corners[0].from).toBeCloseTo(Math.PI / 3);
    const close = ringSegments([at(0), at(0.2)]);
    expect(close[0].to).toBeLessThan(close[1].from);
    expect(nearness({ status: 'ok', meters: 0.3 }, 0.35)).toBe('stop');
    expect(nearness({ status: 'ok', meters: 0.6 }, 0.35)).toBe('near');
    expect(nearness({ status: 'no-target', meters: 2 }, 0.35)).toBe('clear');
    expect(nearness(null, 0.35)).toBe('clear');
  });

  it('reads the pose, the limits and the base’s velocity', () => {
    const half = Math.SQRT1_2;
    const p = poseOf({
      position: { x: 1, y: 2, z: 0 },
      orientation: { x: 0, y: 0, z: half, w: half },
    });
    expect(p).toMatchObject({ x: 1, y: 2, yawDeg: expect.closeTo(90) });
    expect(readLimits('{"driving":true,"message":"Stopped.","blocked":{"forward":true}}')).toEqual({
      driving: true,
      cap: 1,
      message: 'Stopped.',
      blocked: { forward: true, back: false, left: false, right: false, turn: false },
    });
    expect(readLimits('not json').message).toBe('');
    expect(readLimits(null).driving).toBe(false);
    expect(velocityText({ vx: 0.3, vy: 0, omega: 0 })).toBe('0.30 m/s ahead');
    expect(velocityText({ vx: 0, vy: -0.1, omega: 0.5 })).toBe('0.10 m/s to the right, turning');
    expect(velocityText({ vx: 0, vy: 0, omega: 0 })).toBe('Standing still');
    expect(velocityText({ vx: 0, vy: 0, omega: -0.4 })).toBe('Turning right');
  });
});
