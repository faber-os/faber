/**
 * The top view: x forward is up on screen, y left is left (REP-103, docs/capabilities.md).
 * `scale` is pixels per metre; `panX`/`panY` move the origin away from the canvas centre in pixels.
 */
export interface View {
  scale: number;
  panX: number;
  panY: number;
}

export interface CanvasSize {
  width: number;
  height: number;
}

/** 2 px/m shows about ±150 m on the 7" screen; 4000 px/m shows millimetres. */
export const MIN_SCALE = 2;
export const MAX_SCALE = 4000;

export function worldToScreen(x: number, y: number, v: View, size: CanvasSize): [number, number] {
  return [size.width / 2 + v.panX - y * v.scale, size.height / 2 + v.panY - x * v.scale];
}

export function screenToWorld(
  sx: number,
  sy: number,
  v: View,
  size: CanvasSize,
): { x: number; y: number } {
  return {
    x: (size.height / 2 + v.panY - sy) / v.scale,
    y: (size.width / 2 + v.panX - sx) / v.scale,
  };
}

/** Zoom by `factor`, keeping the world point under (sx, sy) where it is. */
export function zoomAt(v: View, factor: number, sx: number, sy: number, size: CanvasSize): View {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, v.scale * factor));
  const k = scale / v.scale;
  // The origin's distance from the anchor scales with the zoom.
  const ox = size.width / 2 + v.panX;
  const oy = size.height / 2 + v.panY;
  const nox = sx + (ox - sx) * k;
  const noy = sy + (oy - sy) * k;
  return { scale, panX: nox - size.width / 2, panY: noy - size.height / 2 };
}

export function zoomCentre(v: View, factor: number, size: CanvasSize): View {
  return zoomAt(v, factor, size.width / 2, size.height / 2, size);
}

export function panBy(v: View, dx: number, dy: number): View {
  return { ...v, panX: v.panX + dx, panY: v.panY + dy };
}

/** Show `rangeM` metres around the origin in the smaller canvas side, with a small margin. */
export function fitView(rangeM: number, size: CanvasSize): View {
  const half = Math.min(size.width, size.height) / 2;
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, (half * 0.9) / rangeM));
  return { scale, panX: 0, panY: 0 };
}
