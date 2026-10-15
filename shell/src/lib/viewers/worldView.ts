/**
 * The World window's top view (docs/simulation.md, sections 7 and 9): pure maths, no drawing.
 *
 * The world frame has x east and y north (m), so on screen east is right and north is up. A
 * `Camera` says which world point sits at the canvas centre and how many pixels a metre is.
 * Machine-frame things (the footprint, a sensor's mount, its scan) are turned into world
 * coordinates with the machine's pose, the same way the core's simulation does it.
 */

import type { CanvasSize } from './view';

export interface Camera {
  /** Pixels per metre. */
  scale: number;
  /** The world point (m) at the centre of the canvas. */
  cx: number;
  cy: number;
}

/** x east, y north (m), yaw counter-clockwise from east (rad). */
export interface Pose2 {
  x: number;
  y: number;
  yaw: number;
}

/** 5 px/m shows a 100 m world on the 7" screen; 2000 px/m shows millimetres. */
export const MIN_WORLD_SCALE = 5;
export const MAX_WORLD_SCALE = 2000;

function clampScale(s: number): number {
  return Math.min(MAX_WORLD_SCALE, Math.max(MIN_WORLD_SCALE, s));
}

export function worldToScreen(x: number, y: number, c: Camera, size: CanvasSize): [number, number] {
  return [size.width / 2 + (x - c.cx) * c.scale, size.height / 2 - (y - c.cy) * c.scale];
}

export function screenToWorld(
  sx: number,
  sy: number,
  c: Camera,
  size: CanvasSize,
): { x: number; y: number } {
  return { x: c.cx + (sx - size.width / 2) / c.scale, y: c.cy - (sy - size.height / 2) / c.scale };
}

/**
 * The canvas transform that draws world coordinates (m) straight onto a canvas of `size` CSS
 * pixels at `dpr`: `ctx.setTransform(...worldTransform(...))`.
 */
export function worldTransform(
  c: Camera,
  size: CanvasSize,
  dpr: number,
): [number, number, number, number, number, number] {
  const k = c.scale * dpr;
  return [
    k,
    0,
    0,
    -k,
    dpr * (size.width / 2 - c.cx * c.scale),
    dpr * (size.height / 2 + c.cy * c.scale),
  ];
}

/** Zoom by `factor`, keeping the world point under (sx, sy) where it is on screen. */
export function zoomAt(
  c: Camera,
  factor: number,
  sx: number,
  sy: number,
  size: CanvasSize,
): Camera {
  const anchor = screenToWorld(sx, sy, c, size);
  const scale = clampScale(c.scale * factor);
  return {
    scale,
    cx: anchor.x - (sx - size.width / 2) / scale,
    cy: anchor.y + (sy - size.height / 2) / scale,
  };
}

/** Move the view with the pointer: the world follows a drag of (dx, dy) pixels. */
export function panBy(c: Camera, dx: number, dy: number): Camera {
  return { ...c, cx: c.cx - dx / c.scale, cy: c.cy + dy / c.scale };
}

/** The camera centred on (x, y), at the same zoom. */
export function centreOn(c: Camera, x: number, y: number): Camera {
  return { ...c, cx: x, cy: y };
}

/** The whole of `bounds` (m) in view, with a small margin. */
export function fitBounds(
  bounds: { min: [number, number]; max: [number, number] },
  size: CanvasSize,
): Camera {
  const w = Math.max(0.5, bounds.max[0] - bounds.min[0]);
  const h = Math.max(0.5, bounds.max[1] - bounds.min[1]);
  const scale = clampScale(Math.min(size.width / w, size.height / h) * 0.9);
  return {
    scale,
    cx: (bounds.min[0] + bounds.max[0]) / 2,
    cy: (bounds.min[1] + bounds.max[1]) / 2,
  };
}

/** A machine-frame point (x forward, y left) in the world, for the machine at `pose`. */
export function toWorld(pose: Pose2, x: number, y: number): [number, number] {
  const c = Math.cos(pose.yaw);
  const s = Math.sin(pose.yaw);
  return [pose.x + c * x - s * y, pose.y + s * x + c * y];
}

/** Where a part mounted at `mount` (machine frame: x, y, yaw) is in the world. */
export function mountPose(pose: Pose2, mount: { x: number; y: number; yaw: number }): Pose2 {
  const [x, y] = toWorld(pose, mount.x, mount.y);
  return { x, y, yaw: pose.yaw + mount.yaw };
}

/** The footprint's corners in the world, counter-clockwise from front-left (as the core). */
export function footprintCorners(
  pose: Pose2,
  footprint: { length: number; width: number },
): [number, number][] {
  const l = footprint.length / 2;
  const w = footprint.width / 2;
  return [toWorld(pose, l, w), toWorld(pose, -l, w), toWorld(pose, -l, -w), toWorld(pose, l, -w)];
}

/** Scan points in the part's frame (x forward, y left), moved into the world. */
export function pointsToWorld(
  sensor: Pose2,
  xs: ArrayLike<number>,
  ys: ArrayLike<number>,
  count: number,
): { x: Float32Array; y: Float32Array } {
  const c = Math.cos(sensor.yaw);
  const s = Math.sin(sensor.yaw);
  const x = new Float32Array(count);
  const y = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const px = xs[i]!;
    const py = ys[i]!;
    x[i] = sensor.x + c * px - s * py;
    y[i] = sensor.y + s * px + c * py;
  }
  return { x, y };
}

/**
 * A distance sensor's cone as a polygon in the world: its apex, then an arc of `steps` + 1
 * points `range` metres out, from the right edge to the left edge.
 */
export function conePolygon(
  sensor: Pose2,
  fovRad: number,
  range: number,
  steps = 12,
): [number, number][] {
  const points: [number, number][] = [[sensor.x, sensor.y]];
  const n = fovRad > 0 ? Math.max(1, steps) : 1;
  for (let i = 0; i <= n; i++) {
    const a = sensor.yaw - fovRad / 2 + (fovRad * i) / n;
    points.push([sensor.x + range * Math.cos(a), sensor.y + range * Math.sin(a)]);
  }
  return points;
}

/** How far a distance cone is drawn: to what it measured, else to its range. */
export function coneLength(
  reading: { meters: number; status: string } | null,
  info: { minM: number; maxM: number },
): number {
  if (!reading) return info.maxM;
  if (reading.status === 'ok' && Number.isFinite(reading.meters)) return reading.meters;
  if (reading.status === 'too-close') return info.minM;
  return info.maxM;
}

/** Which colour a cone gets: a target in range, too close, nothing (too far) or a fault. */
export type ConeTone = 'ok' | 'near' | 'none' | 'bad';

export function coneTone(status: string | null): ConeTone {
  switch (status) {
    case 'ok':
      return 'ok';
    case 'too-close':
      return 'near';
    case 'error':
      return 'bad';
    default:
      return 'none';
  }
}

/** Where the machine was over the last `windowUs`, oldest first. */
export class Trail {
  private t: number[] = [];
  private p: Pose2[] = [];

  constructor(readonly windowUs: number) {}

  push(tUs: number, pose: Pose2): void {
    const last = this.t.at(-1);
    // The world was reset or the core restarted: its clock went back, so start afresh.
    if (last !== undefined && tUs < last) this.clear();
    if (last === tUs) return;
    this.t.push(tUs);
    this.p.push(pose);
    const oldest = tUs - this.windowUs;
    let drop = 0;
    while (drop < this.t.length && this.t[drop]! < oldest) drop++;
    if (drop > 0) {
      this.t.splice(0, drop);
      this.p.splice(0, drop);
    }
  }

  clear(): void {
    this.t = [];
    this.p = [];
  }

  get poses(): readonly Pose2[] {
    return this.p;
  }

  get latest(): Pose2 | null {
    return this.p.at(-1) ?? null;
  }

  /**
   * The pose at `tUs`, between the two nearest samples, so a scan is drawn where the machine was
   * when it was taken. Outside the trail: its nearest end.
   */
  poseAt(tUs: number): Pose2 | null {
    const n = this.t.length;
    if (n === 0) return null;
    if (tUs <= this.t[0]!) return this.p[0]!;
    if (tUs >= this.t[n - 1]!) return this.p[n - 1]!;
    let hi = 1;
    while (this.t[hi]! < tUs) hi++;
    const t0 = this.t[hi - 1]!;
    const t1 = this.t[hi]!;
    const a = this.p[hi - 1]!;
    const b = this.p[hi]!;
    const k = (tUs - t0) / (t1 - t0);
    const turn = Math.atan2(Math.sin(b.yaw - a.yaw), Math.cos(b.yaw - a.yaw));
    return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k, yaw: a.yaw + turn * k };
  }
}
