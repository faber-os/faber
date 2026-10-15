/**
 * How big Files draws a folder: a details list, small icons or large icons. Ctrl and the mouse
 * wheel, Ctrl +/−/0, a trackpad or touch pinch, and the view buttons all move along these steps.
 */

export const ZOOM_STEPS = ['details', 'small', 'large'] as const;
export type FileView = (typeof ZOOM_STEPS)[number];

export const DEFAULT_VIEW: FileView = 'details';

export const VIEW_LABELS: Record<FileView, string> = {
  details: 'Details',
  small: 'Small icons',
  large: 'Large icons',
};

/** One step bigger (+1) or smaller (−1), stopping at the ends. */
export function zoomView(view: FileView, dir: 1 | -1): FileView {
  const i = ZOOM_STEPS.indexOf(view);
  return ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, i + dir))]!;
}

/** Wheel distance (px) that makes one step; a notched mouse wheel sends about 100 per notch. */
export const WHEEL_STEP = 80;

/**
 * Ctrl+wheel. Trackpads send many small deltas (a pinch arrives as Ctrl+wheel too), so they are
 * added up until they make a step. Wheel up (negative deltaY) zooms in, as in a browser.
 */
export function wheelZoom(
  view: FileView,
  carried: number,
  deltaY: number,
): { view: FileView; carried: number } {
  const total = carried + deltaY;
  if (Math.abs(total) < WHEEL_STEP) return { view, carried: total };
  return { view: zoomView(view, total < 0 ? 1 : -1), carried: 0 };
}

/** Ctrl + / Ctrl − / Ctrl 0 (also with the number pad), or null for any other key. */
export function keyZoom(view: FileView, key: string): FileView | null {
  if (key === '+' || key === '=' || key === 'Add') return zoomView(view, 1);
  if (key === '-' || key === '_' || key === 'Subtract') return zoomView(view, -1);
  if (key === '0') return DEFAULT_VIEW;
  return null;
}

/** A two-finger pinch: spreading by a quarter zooms in one step, pinching by a fifth zooms out. */
export function pinchZoom(view: FileView, startDistance: number, distance: number): FileView {
  if (startDistance <= 0) return view;
  const ratio = distance / startDistance;
  if (ratio >= 1.25) return zoomView(view, 1);
  if (ratio <= 0.8) return zoomView(view, -1);
  return view;
}
