/** Points are drawn in this many intensity colours, so the canvas changes fillStyle rarely. */
export const INTENSITY_BUCKETS = 24;

export type ViewerTheme = 'light' | 'dark';

export function intensityBucket(intensity: number, buckets: number): number {
  if (!(intensity > 0)) return 0;
  return Math.min(buckets - 1, Math.floor(intensity * buckets));
}

/**
 * A heat ramp, like metal in a forge: weak returns glow dull red, strong returns burn yellow-white.
 * The dark theme can go brighter; the light theme stays dark enough to read on a pale background.
 */
export function intensityRamp(buckets: number, theme: ViewerTheme): string[] {
  const [l0, l1] = theme === 'dark' ? [0.5, 0.94] : [0.42, 0.74];
  return Array.from({ length: buckets }, (_, i) => {
    const t = buckets === 1 ? 1 : i / (buckets - 1);
    const l = l0 + (l1 - l0) * t;
    const c = 0.2 - 0.08 * t * t;
    const h = 28 + 62 * t;
    return `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)})`;
  });
}
