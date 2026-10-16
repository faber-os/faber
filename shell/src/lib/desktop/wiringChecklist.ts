/**
 * The wiring checklist's ticks (docs/wiring.md §3, step 4), kept per guided board in this
 * browser so they survive closing the window while wiring. A tick belongs to a line's exact
 * text: when a wire moves its line changes, so it shows unticked again. Storage can be missing,
 * blocked or hold junk: then nothing is ticked.
 */
import type { StorageLike } from './geometryStore';

const key = (plan: string) => `faber.wiring.v1.${plan}`;

/** The id of one checklist line: its part and its text. */
export function lineId(part: string, line: string): string {
  return `${part}\u0000${line}`;
}

export function loadTicks(storage: StorageLike | null, plan: string): Set<string> {
  if (!storage) return new Set();
  try {
    const raw: unknown = JSON.parse(storage.getItem(key(plan)) ?? '[]');
    return new Set(Array.isArray(raw) ? raw.filter((x) => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

/** Keeps only ticks of lines that still exist, so old wires don't pile up. */
export function saveTicks(
  storage: StorageLike | null,
  plan: string,
  ticks: Set<string>,
  lines: string[],
): void {
  if (!storage) return;
  try {
    storage.setItem(key(plan), JSON.stringify(lines.filter((l) => ticks.has(l))));
  } catch {
    // Not remembering the ticks only means ticking them again after a reload.
  }
}
