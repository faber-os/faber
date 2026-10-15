import type { Rect } from './windows';

/** The two Storage methods used, so tests (and a missing localStorage) are easy. */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const KEY = 'faber.windows.v1';

/**
 * Remembered window geometry, by window key. Storage can be missing or throw (private windows,
 * blocked site data): then the desktop simply starts with default positions.
 */
export function loadGeometry(storage: StorageLike | null): Record<string, Rect> {
  if (!storage) return {};
  let raw: unknown;
  try {
    raw = JSON.parse(storage.getItem(KEY) ?? '{}');
  } catch {
    return {};
  }
  if (typeof raw !== 'object' || raw === null) return {};
  const out: Record<string, Rect> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (isRect(v)) out[k] = { x: v.x, y: v.y, w: v.w, h: v.h };
  }
  return out;
}

export function saveGeometry(storage: StorageLike | null, geometry: Record<string, Rect>): void {
  if (!storage) return;
  try {
    storage.setItem(KEY, JSON.stringify(geometry));
  } catch {
    // Not being able to remember positions is harmless; the desktop works the same.
  }
}

function isRect(v: unknown): v is Rect {
  if (typeof v !== 'object' || v === null) return false;
  const r = v as Record<string, unknown>;
  return ['x', 'y', 'w', 'h'].every((k) => typeof r[k] === 'number' && Number.isFinite(r[k]));
}
