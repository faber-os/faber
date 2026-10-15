/** Points are drawn in this many intensity colours, so the canvas changes fillStyle rarely. */
export const INTENSITY_BUCKETS = 24;

export type ViewerTheme = 'light' | 'dark';

export function intensityBucket(intensity: number, buckets: number): number {
  if (!(intensity > 0)) return 0;
  return Math.min(buckets - 1, Math.floor(intensity * buckets));
}

/**
 * Weak returns are faint slate, strong ones deep anodised teal: contrast with the background grows
 * with intensity, so the strong returns stand out on either theme (darker on light, brighter on dark).
 */
export function intensityRamp(buckets: number, theme: ViewerTheme): string[] {
  return Array.from({ length: buckets }, (_, i) => {
    const t = buckets === 1 ? 1 : i / (buckets - 1);
    const l = theme === 'dark' ? 0.46 + 0.44 * t : 0.78 - 0.36 * t;
    const c = 0.03 + 0.08 * t;
    const h = 255 - 72 * t;
    return `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)})`;
  });
}
