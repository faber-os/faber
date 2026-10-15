import type { CanvasSize, View } from './view';

const MAX_RINGS = 100;

/** The smallest 1-2-5 × 10ⁿ step (m) whose rings are at least `minPx` pixels apart. */
export function ringStep(scale: number, minPx: number): number {
  const raw = minPx / scale;
  const exp = Math.floor(Math.log10(raw));
  for (let e = exp; e <= exp + 1; e++) {
    for (const m of [1, 2, 5]) {
      const step = roundNice(m * 10 ** e);
      if (step * scale >= minPx - 1e-9) return step;
    }
  }
  return roundNice(10 ** (exp + 1));
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

export function formatDistance(m: number): string {
  if (m < 1) return `${trim(m * 100)} cm`;
  return `${trim(m)} m`;
}

function trim(v: number): string {
  return String(Number(v.toFixed(3)));
}

/** Remove floating-point noise such as 0.30000000000000004. */
function roundNice(v: number): number {
  return Number(v.toPrecision(12));
}
