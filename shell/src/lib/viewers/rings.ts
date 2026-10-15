import { niceLengthStep, type LengthUnit } from './units';
import type { CanvasSize, View } from './view';

const MAX_RINGS = 100;

/**
 * The smallest round step (m) whose rings are at least `minPx` pixels apart: 1-2-5 × 10ⁿ metres,
 * or whole inches and feet.
 */
export function ringStep(scale: number, minPx: number, unit: LengthUnit = 'metric'): number {
  return niceLengthStep(minPx / scale, unit);
}

/** Ring radii (m) from `step` out to the canvas corner farthest from the origin. */
export function ringRadii(v: View, size: CanvasSize, step: number): number[] {
  const ox = size.width / 2 + v.panX;
  const oy = size.height / 2 + v.panY;
  const far = Math.max(
    Math.hypot(ox, oy),
    Math.hypot(size.width - ox, oy),
    Math.hypot(ox, size.height - oy),
    Math.hypot(size.width - ox, size.height - oy),
  );
  const count = Math.min(MAX_RINGS, Math.ceil(far / v.scale / step));
  return Array.from({ length: count }, (_, i) => roundNice((i + 1) * step));
}

/** Remove floating-point noise such as 0.30000000000000004. */
function roundNice(v: number): number {
  return Number(v.toPrecision(12));
}
