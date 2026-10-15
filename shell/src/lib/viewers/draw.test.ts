import { describe, expect, it } from 'vitest';
import { drawLaserScan, type ScanPalette } from './draw';
import { scanToPoints } from './laserScan';

/** Records fillStyle changes and point rectangles; enough to check batching, not pixels. */
function recorder() {
  const styles: string[] = [];
  let rects = 0;
  const ctx = {
    set fillStyle(v: string) {
      styles.push(v);
    },
    set strokeStyle(_v: string) {},
    set lineWidth(_v: number) {},
    set font(_v: string) {},
    set textAlign(_v: string) {},
    set textBaseline(_v: string) {},
    save() {},
    restore() {},
    setTransform() {},
    clearRect() {},
    fillRect() {
      rects++;
    },
    beginPath() {},
    arc() {},
    moveTo() {},
    lineTo() {},
    closePath() {},
    stroke() {},
    fill() {},
    fillText() {},
    setLineDash() {},
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, styles, rects: () => rects };
}

const palette: ScanPalette = {
  background: 'bg',
  ring: 'ring',
  axis: 'axis',
  label: 'label',
  origin: 'origin',
  points: Array.from({ length: 4 }, (_, i) => `p${i}`),
};

describe('drawLaserScan', () => {
  it('draws every point with a return, setting each colour at most once', () => {
    const n = 200;
    const points = scanToPoints({
      angleMin: 0,
      angleIncrement: (2 * Math.PI) / n,
      scanTime: 0.1,
      ranges: Array.from({ length: n }, (_, i) => (i % 10 === 0 ? Infinity : 2)),
      intensities: Array.from({ length: n }, (_, i) => (i % 4) / 4),
    });
    const r = recorder();
    drawLaserScan(
      r.ctx,
      points,
      { scale: 50, panX: 0, panY: 0 },
      { width: 400, height: 300 },
      1,
      palette,
    );
    const pointStyles = r.styles.filter((s) => s.startsWith('p'));
    expect(new Set(pointStyles).size).toBe(pointStyles.length);
    // 180 point rectangles plus the background and the origin marker are drawn.
    expect(r.rects()).toBeGreaterThanOrEqual(180);
  });
});
