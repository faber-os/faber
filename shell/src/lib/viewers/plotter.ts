/**
 * The Signal plotter's pure half: which numbers a reading has, their scrolling plot, and the CSV
 * a person can save. Any capability works: every number in a reading (and every true/false, as
 * 1/0) is a field that can be plotted.
 */

import type { Sample } from './signals';

/** One plottable number in a reading, by its path: `celsius`, `twist.vx`, `level`. */
export interface Field {
  path: string;
  value: number;
}

/**
 * Every number and true/false in `reading` (the capability's own fields, as the API sends them),
 * with nested objects as dotted paths. The time (`tUs`) and lists (a scan's ranges) are left out.
 */
export function numericFields(reading: unknown, prefix = ''): Field[] {
  if (reading === null || typeof reading !== 'object' || Array.isArray(reading)) return [];
  const out: Field[] = [];
  for (const [key, value] of Object.entries(reading as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (path === 'tUs') continue;
    if (typeof value === 'number' && Number.isFinite(value)) out.push({ path, value });
    else if (typeof value === 'boolean') out.push({ path, value: value ? 1 : 0 });
    else if (value !== null && typeof value === 'object' && !Array.isArray(value))
      out.push(...numericFields(value, path));
  }
  return out;
}

/** A plotted series: one field of one signal. */
export interface SeriesKey {
  address: string;
  field: string;
}

export function seriesId(k: SeriesKey): string {
  return `${k.address} ${k.field}`;
}

/** The lowest and highest value in view, widened a little so a flat line isn't at the edge. */
export function valueRange(series: readonly (readonly Sample[])[]): { min: number; max: number } {
  let min = Infinity;
  let max = -Infinity;
  for (const s of series)
    for (const p of s) {
      if (p.value < min) min = p.value;
      if (p.value > max) max = p.value;
    }
  if (!Number.isFinite(min)) return { min: 0, max: 1 };
  if (min === max) return { min: min - 1, max: max + 1 };
  const pad = (max - min) * 0.05;
  return { min: min - pad, max: max + pad };
}

/** The samples as one SVG polyline: time across (newest at the right), value up. */
export function polyline(
  samples: readonly Sample[],
  nowUs: number,
  windowUs: number,
  range: { min: number; max: number },
  width: number,
  height: number,
): string {
  const span = range.max - range.min || 1;
  return samples
    .map((s) => {
      const x = width * (1 - (nowUs - s.tUs) / windowUs);
      const y = height * (1 - (s.value - range.min) / span);
      return `${round(x)},${round(y)}`;
    })
    .join(' ');
}

function round(v: number): number {
  return Math.round(v * 10) / 10;
}

/** One row of the CSV: when (core time, µs), which series, and its value. */
export interface CsvRow {
  tUs: number;
  address: string;
  field: string;
  value: number;
}

/**
 * The CSV a person saves: one line per sample, oldest first, time in seconds on the core's
 * clock. Long form (`time_s,signal,field,value`), so any number of series fit one file and a
 * spreadsheet can pivot it.
 */
export function toCsv(rows: readonly CsvRow[]): string {
  const sorted = [...rows].sort((a, b) => a.tUs - b.tUs);
  const lines = ['time_s,signal,field,value'];
  for (const r of sorted)
    lines.push(
      [(r.tUs / 1e6).toFixed(6), csvText(r.address), csvText(r.field), String(r.value)].join(','),
    );
  return lines.join('\n') + '\n';
}

function csvText(s: string): string {
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}
