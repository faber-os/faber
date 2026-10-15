/**
 * Small viewers for one-number signals: a distance sensor's bar and recent plot, and a drive
 * base's velocity gauges. Pure maths; the windows draw what these return.
 */

export interface Sample {
  /** Core time (µs). */
  tUs: number;
  value: number;
  /** False for a reading that isn't a real measurement (out of range, no target). */
  ok: boolean;
}

/** The recent past of one signal: keeps `windowUs` of samples, oldest first. */
export class History {
  private items: Sample[] = [];

  constructor(readonly windowUs: number) {}

  push(s: Sample): void {
    // A restarted part starts its clock again: begin a fresh plot rather than draw backwards.
    const last = this.items.at(-1);
    if (last && s.tUs < last.tUs) this.items = [];
    this.items.push(s);
    const oldest = s.tUs - this.windowUs;
    let drop = 0;
    while (drop < this.items.length && this.items[drop]!.tUs < oldest) drop++;
    if (drop > 0) this.items.splice(0, drop);
  }

  get samples(): readonly Sample[] {
    return this.items;
  }

  get latest(): Sample | null {
    return this.items.at(-1) ?? null;
  }
}

/** Where a value sits between 0 and `max`, as 0..1. */
export function barFraction(value: number, max: number): number {
  if (!(max > 0) || !Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value / max));
}

/** A value against a symmetric limit, as −1..1 (a gauge with zero in the middle). */
export function gaugeFraction(value: number, max: number): number {
  if (!(max > 0) || !Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(-1, value / max));
}

/**
 * The plot of `samples` as SVG polyline runs: x is time (the newest at the right edge), y is the
 * value (0 at the bottom, `max` at the top). A sample that isn't a real measurement breaks the
 * line, so a gap shows as a gap and not as a made-up value.
 */
export function plotRuns(
  samples: readonly Sample[],
  nowUs: number,
  windowUs: number,
  max: number,
  width: number,
  height: number,
): string[] {
  const runs: string[] = [];
  let run: string[] = [];
  for (const s of samples) {
    if (!s.ok) {
      if (run.length) runs.push(run.join(' '));
      run = [];
      continue;
    }
    const x = width * (1 - (nowUs - s.tUs) / windowUs);
    const y = height * (1 - barFraction(s.value, max));
    run.push(`${round(x)},${round(y)}`);
  }
  if (run.length) runs.push(run.join(' '));
  return runs;
}

function round(v: number): number {
  return Math.round(v * 10) / 10;
}
