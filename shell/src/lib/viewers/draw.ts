import { intensityBucket } from './colour';
import type { ScanPoints } from './laserScan';
import { formatDistance, ringRadii, ringStep } from './rings';
import { worldToScreen, type CanvasSize, type View } from './view';

export interface ScanPalette {
  background: string;
  ring: string;
  axis: string;
  label: string;
  origin: string;
  /** One colour per intensity bucket, weakest first (see intensityRamp). */
  points: string[];
}

const RING_MIN_PX = 64;
const POINT_PX = 2.5;

/**
 * Draw one scan in the top view. `size` is in CSS pixels; the canvas backing store is
 * `size × dpr`. Draws only what it is given: the caller passes the newest reading each frame.
 */
export function drawLaserScan(
  ctx: CanvasRenderingContext2D,
  points: ScanPoints | null,
  view: View,
  size: CanvasSize,
  dpr: number,
  palette: ScanPalette,
): void {
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = palette.background;
  ctx.fillRect(0, 0, size.width, size.height);

  drawRings(ctx, view, size, palette);
  if (points) drawPoints(ctx, points, view, size, palette.points);
  drawOrigin(ctx, view, size, palette.origin);
}

function drawRings(
  ctx: CanvasRenderingContext2D,
  view: View,
  size: CanvasSize,
  palette: ScanPalette,
) {
  const [ox, oy] = worldToScreen(0, 0, view, size);
  const step = ringStep(view.scale, RING_MIN_PX);

  ctx.strokeStyle = palette.axis;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(ox, 0);
  ctx.lineTo(ox, size.height);
  ctx.moveTo(0, oy);
  ctx.lineTo(size.width, oy);
  ctx.stroke();

  ctx.strokeStyle = palette.ring;
  ctx.fillStyle = palette.label;
  ctx.font = '500 11px "Red Hat Mono Variable", ui-monospace, monospace';
  ctx.textAlign = 'left';
  ctx.textBaseline = 'bottom';
  for (const r of ringRadii(view, size, step)) {
    const px = r * view.scale;
    ctx.beginPath();
    ctx.arc(ox, oy, px, 0, Math.PI * 2);
    ctx.stroke();
    // Label each ring where it crosses the forward axis, just right of it.
    const ly = oy - px;
    if (ly > 12 && ly < size.height) ctx.fillText(formatDistance(r), ox + 4, ly - 2);
  }
}

function drawPoints(
  ctx: CanvasRenderingContext2D,
  p: ScanPoints,
  view: View,
  size: CanvasSize,
  colours: string[],
) {
  const buckets = colours.length;
  // Counting sort by colour, so each colour is set once per frame.
  const bucketOf = new Uint8Array(p.count);
  const counts = new Uint32Array(buckets + 1);
  for (let i = 0; i < p.count; i++) {
    const b = intensityBucket(p.intensity[i]!, buckets);
    bucketOf[i] = b;
    counts[b + 1]!++;
  }
  for (let b = 1; b <= buckets; b++) counts[b]! += counts[b - 1]!;
  const order = new Uint32Array(p.count);
  const fill = counts.slice(0, buckets);
  for (let i = 0; i < p.count; i++) order[fill[bucketOf[i]!]!++] = i;

  const half = POINT_PX / 2;
  const cx = size.width / 2 + view.panX;
  const cy = size.height / 2 + view.panY;
  for (let b = 0; b < buckets; b++) {
    const start = counts[b]!;
    const end = counts[b + 1]!;
    if (start === end) continue;
    ctx.fillStyle = colours[b]!;
    for (let k = start; k < end; k++) {
      const i = order[k]!;
      const sx = cx - p.y[i]! * view.scale;
      const sy = cy - p.x[i]! * view.scale;
      if (sx < -half || sy < -half || sx > size.width + half || sy > size.height + half) continue;
      ctx.fillRect(sx - half, sy - half, POINT_PX, POINT_PX);
    }
  }
}

/** A small arrow at the part's origin, pointing forward (up). */
function drawOrigin(ctx: CanvasRenderingContext2D, view: View, size: CanvasSize, colour: string) {
  const [ox, oy] = worldToScreen(0, 0, view, size);
  ctx.fillStyle = colour;
  ctx.beginPath();
  ctx.moveTo(ox, oy - 9);
  ctx.lineTo(ox + 6, oy + 6);
  ctx.lineTo(ox, oy + 3);
  ctx.lineTo(ox - 6, oy + 6);
  ctx.closePath();
  ctx.fill();
}
