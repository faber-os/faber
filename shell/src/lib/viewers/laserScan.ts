/**
 * The shape of a `laser-scan@1` reading that the viewer needs (docs/capabilities.md). It matches
 * the generated LaserScanReading structurally, so viewers stay free of the API layer.
 */
export interface ScanInput {
  angleMin: number;
  angleIncrement: number;
  angles?: number[];
  scanTime: number;
  ranges: number[];
  intensities?: number[];
}

/** The points with a return, in the part's frame: x forward, y left (m). */
export interface ScanPoints {
  x: Float32Array;
  y: Float32Array;
  intensity: Float32Array;
  count: number;
  hasIntensity: boolean;
}

export interface ScanStats {
  /** Points with a return. */
  points: number;
  /** Points without a return (`+inf`, or a value that is not a positive number). */
  noReturn: number;
  /** Revolutions per second (1 / scanTime), or null when the scan time is unknown. */
  rateHz: number | null;
}

function hasReturn(r: number): boolean {
  return Number.isFinite(r) && r > 0;
}

export function scanToPoints(scan: ScanInput): ScanPoints {
  const n = scan.ranges.length;
  const x = new Float32Array(n);
  const y = new Float32Array(n);
  const intensity = new Float32Array(n);
  const intens = scan.intensities;
  const hasIntensity = intens !== undefined && intens.length === n;
  let count = 0;
  for (let i = 0; i < n; i++) {
    const r = scan.ranges[i]!;
    if (!hasReturn(r)) continue;
    const a = scan.angles ? scan.angles[i]! : scan.angleMin + i * scan.angleIncrement;
    x[count] = r * Math.cos(a);
    y[count] = r * Math.sin(a);
    intensity[count] = hasIntensity ? intens[i]! : 1;
    count++;
  }
  return { x, y, intensity, count, hasIntensity };
}

export function scanStats(scan: ScanInput): ScanStats {
  let points = 0;
  for (const r of scan.ranges) if (hasReturn(r)) points++;
  return {
    points,
    noReturn: scan.ranges.length - points,
    rateHz: scan.scanTime > 0 ? 1 / scan.scanTime : null,
  };
}
