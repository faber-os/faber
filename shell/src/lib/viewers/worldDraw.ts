import { formatLength, niceLengthStep, type LengthUnit } from './units';
import type { CanvasSize } from './view';
import {
  footprintCorners,
  toWorld,
  worldToScreen,
  worldTransform,
  type Camera,
  type ConeTone,
  type Pose2,
} from './worldView';

/** Colours of the World view, read from the theme's tokens by the window. */
export interface WorldPalette {
  background: string;
  floor: string;
  grid: string;
  wall: string;
  shape: string;
  overhead: string;
  label: string;
  machine: string;
  heading: string;
  trail: string;
  mover: string;
  scan: string;
  bumped: string;
  cone: Record<ConeTone, string>;
}

type Vec2 = [number, number];

/** The world's geometry as the API gives it (`WorldView.geometry`), as far as drawing needs. */
export interface GeometryInput {
  bounds: { min: Vec2; max: Vec2 };
  walls: { a: Vec2; b: Vec2 }[];
  shapes: { points: Vec2[] }[];
  overhead: { points: Vec2[] }[];
}

/** What a Path2D offers that the static layer uses (tests pass a recorder). */
export interface PathLike {
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  closePath(): void;
  rect(x: number, y: number, w: number, h: number): void;
}

/**
 * The world's fixed geometry as paths in world coordinates (m), built once per world. Every frame
 * then fills and strokes them under the camera's transform, so a 30 × 20 m warehouse costs a
 * handful of canvas calls, whatever the zoom.
 */
export interface StaticLayer<P extends PathLike = PathLike> {
  floor: P;
  walls: P;
  shapes: P;
  overhead: P;
  bounds: { min: Vec2; max: Vec2 };
}

export function buildStatic<P extends PathLike>(g: GeometryInput, make: () => P): StaticLayer<P> {
  const floor = make();
  const { min, max } = g.bounds;
  floor.rect(min[0], min[1], max[0] - min[0], max[1] - min[1]);
  const walls = make();
  for (const w of g.walls) {
    walls.moveTo(w.a[0], w.a[1]);
    walls.lineTo(w.b[0], w.b[1]);
  }
  const polygons = (list: { points: Vec2[] }[]) => {
    const p = make();
    for (const s of list) {
      s.points.forEach(([x, y], i) => (i === 0 ? p.moveTo(x, y) : p.lineTo(x, y)));
      p.closePath();
    }
    return p;
  };
  return {
    floor,
    walls,
    shapes: polygons(g.shapes),
    overhead: polygons(g.overhead),
    bounds: g.bounds,
  };
}

/** Everything that moves, for one frame. */
export interface WorldScene {
  /** Where the machine is; `null` before the first `world` message. */
  pose: Pose2 | null;
  /** The base's footprint; `null` without a world base (a small marker is drawn instead). */
  footprint: { length: number; width: number } | null;
  bumped: boolean;
  movers: readonly { x: number; y: number; radius: number }[];
  trail: readonly Pose2[];
  /** Laser points, already in the world (m). */
  scans: readonly { x: Float32Array; y: Float32Array }[];
  cones: readonly { polygon: Vec2[]; tone: ConeTone }[];
}

const POINT_PX = 3;
const WALL_PX = 2.5;
/** Grid lines at least this far apart on screen. */
const GRID_MIN_PX = 48;
/** The machine marker's radius (m) when there is no base to draw. */
const MARKER_M = 0.15;

/**
 * Draws one frame of the World view. `size` is in CSS pixels; the canvas backing store is
 * `size × dpr`.
 */
export function drawWorld<P extends PathLike>(
  ctx: CanvasRenderingContext2D,
  layer: StaticLayer<P> | null,
  scene: WorldScene,
  cam: Camera,
  size: CanvasSize,
  dpr: number,
  palette: WorldPalette,
  unit: LengthUnit = 'metric',
): void {
  const px = 1 / cam.scale; // one CSS pixel, in metres
  const screen = () => ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const world = () => ctx.setTransform(...worldTransform(cam, size, dpr));
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
  screen();
  ctx.fillStyle = palette.background;
  ctx.fillRect(0, 0, size.width, size.height);

  world();
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  const step = niceLengthStep(GRID_MIN_PX * px, unit);
  if (layer) {
    ctx.fillStyle = palette.floor;
    ctx.fill(layer.floor as unknown as Path2D);
    drawGrid(ctx, layer.bounds, step, px, palette.grid);
    ctx.fillStyle = palette.shape;
    ctx.fill(layer.shapes as unknown as Path2D, 'nonzero');
    // Table tops and the like: the machine drives under them, so they are only a shade.
    ctx.globalAlpha = 0.45;
    ctx.fillStyle = palette.overhead;
    ctx.fill(layer.overhead as unknown as Path2D, 'nonzero');
    ctx.globalAlpha = 1;
    ctx.setLineDash([4 * px, 3 * px]);
    ctx.strokeStyle = palette.overhead;
    ctx.lineWidth = px;
    ctx.stroke(layer.overhead as unknown as Path2D);
    ctx.setLineDash([]);
    ctx.strokeStyle = palette.wall;
    ctx.lineWidth = Math.max(WALL_PX * px, 0.04);
    ctx.stroke(layer.walls as unknown as Path2D);
  }

  drawTrail(ctx, scene.trail, px, palette.trail);
  for (const cone of scene.cones) drawCone(ctx, cone.polygon, px, palette.cone[cone.tone]);

  ctx.fillStyle = palette.mover;
  for (const m of scene.movers) {
    ctx.beginPath();
    ctx.arc(m.x, m.y, m.radius, 0, Math.PI * 2);
    ctx.fill();
  }

  if (scene.pose) drawMachine(ctx, scene, px, palette);

  // Points in screen pixels: a world-sized square would vanish when zoomed out.
  screen();
  ctx.fillStyle = palette.scan;
  const half = POINT_PX / 2;
  const cx = size.width / 2 - cam.cx * cam.scale;
  const cy = size.height / 2 + cam.cy * cam.scale;
  for (const s of scene.scans) {
    for (let i = 0; i < s.x.length; i++) {
      const sx = cx + s.x[i]! * cam.scale;
      const sy = cy - s.y[i]! * cam.scale;
      if (sx < -half || sy < -half || sx > size.width + half || sy > size.height + half) continue;
      ctx.fillRect(sx - half, sy - half, POINT_PX, POINT_PX);
    }
  }

  drawScaleBar(ctx, step, cam, size, palette.label, unit);
}

function drawGrid(
  ctx: CanvasRenderingContext2D,
  b: { min: Vec2; max: Vec2 },
  step: number,
  px: number,
  colour: string,
) {
  const lines = (b.max[0] - b.min[0]) / step + (b.max[1] - b.min[1]) / step;
  if (lines > 400) return;
  ctx.strokeStyle = colour;
  ctx.lineWidth = px;
  ctx.beginPath();
  for (let x = Math.ceil(b.min[0] / step) * step; x <= b.max[0] + 1e-9; x += step) {
    ctx.moveTo(x, b.min[1]);
    ctx.lineTo(x, b.max[1]);
  }
  for (let y = Math.ceil(b.min[1] / step) * step; y <= b.max[1] + 1e-9; y += step) {
    ctx.moveTo(b.min[0], y);
    ctx.lineTo(b.max[0], y);
  }
  ctx.stroke();
}

function drawTrail(
  ctx: CanvasRenderingContext2D,
  trail: readonly Pose2[],
  px: number,
  colour: string,
) {
  if (trail.length < 2) return;
  ctx.globalAlpha = 0.55;
  ctx.strokeStyle = colour;
  ctx.lineWidth = 2 * px;
  ctx.beginPath();
  trail.forEach((p, i) => (i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
  ctx.stroke();
  ctx.globalAlpha = 1;
}

function drawCone(ctx: CanvasRenderingContext2D, polygon: Vec2[], px: number, colour: string) {
  ctx.beginPath();
  polygon.forEach(([x, y], i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
  ctx.closePath();
  ctx.globalAlpha = 0.22;
  ctx.fillStyle = colour;
  ctx.fill();
  ctx.globalAlpha = 0.9;
  ctx.strokeStyle = colour;
  ctx.lineWidth = 1.5 * px;
  ctx.stroke();
  ctx.globalAlpha = 1;
}

function drawMachine(
  ctx: CanvasRenderingContext2D,
  scene: WorldScene,
  px: number,
  palette: WorldPalette,
) {
  const pose = scene.pose!;
  const fp = scene.footprint;
  ctx.beginPath();
  let reach: number;
  if (fp) {
    footprintCorners(pose, fp).forEach(([x, y], i) =>
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y),
    );
    ctx.closePath();
    reach = fp.length / 2;
  } else {
    ctx.arc(pose.x, pose.y, MARKER_M, 0, Math.PI * 2);
    reach = MARKER_M;
  }
  ctx.fillStyle = palette.machine;
  ctx.fill();
  if (scene.bumped) {
    ctx.strokeStyle = palette.bumped;
    ctx.lineWidth = 4 * px;
    ctx.stroke();
  }
  // The heading: an arrow from the middle to the front edge.
  const shaft = Math.max(reach * 0.75, 6 * px);
  const head = Math.min(shaft * 0.45, Math.max(0.12, 8 * px));
  const [tx, ty] = toWorld(pose, shaft, 0);
  const [lx, ly] = toWorld(pose, shaft - head, head * 0.6);
  const [rx, ry] = toWorld(pose, shaft - head, -head * 0.6);
  ctx.strokeStyle = palette.heading;
  ctx.fillStyle = palette.heading;
  ctx.lineWidth = Math.max(2 * px, reach * 0.08);
  ctx.beginPath();
  ctx.moveTo(pose.x, pose.y);
  ctx.lineTo(tx, ty);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(tx, ty);
  ctx.lineTo(lx, ly);
  ctx.lineTo(rx, ry);
  ctx.closePath();
  ctx.fill();
}

/** A bar `step` long in the bottom-left corner, labelled in the units setting's length. */
function drawScaleBar(
  ctx: CanvasRenderingContext2D,
  step: number,
  cam: Camera,
  size: CanvasSize,
  colour: string,
  unit: LengthUnit,
) {
  const len = step * cam.scale;
  const x = 14;
  const y = size.height - 16;
  ctx.strokeStyle = colour;
  ctx.fillStyle = colour;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(x, y - 5);
  ctx.lineTo(x, y);
  ctx.lineTo(x + len, y);
  ctx.lineTo(x + len, y - 5);
  ctx.stroke();
  ctx.font = '500 11px "Atkinson Hyperlegible Mono Variable", ui-monospace, monospace';
  ctx.textAlign = 'left';
  ctx.textBaseline = 'bottom';
  ctx.fillText(formatLength(step, unit), x + 4, y - 4);
}

/** Where the machine's centre is on screen, or null when it is out of view. */
export function machineOnScreen(
  pose: Pose2 | null,
  cam: Camera,
  size: CanvasSize,
): { x: number; y: number } | null {
  if (!pose) return null;
  const [x, y] = worldToScreen(pose.x, pose.y, cam, size);
  if (x < 0 || y < 0 || x > size.width || y > size.height) return null;
  return { x, y };
}
