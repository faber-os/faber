/**
 * Notices the user closed with ×: a notice about a lasting situation (a USB device the computer
 * can't talk to, connectors that didn't load) is hidden until something new happens, not
 * forever. A notice is described by a list of items, one per problem; it stays hidden while it
 * shows nothing that wasn't there when it was dismissed, and comes back for a new problem (or one
 * more of the same kind). Remembered in this browser only.
 */
import type { StorageLike } from './geometryStore';

const KEY = 'faber.dismissed.v1';

/** The items each notice showed when it was dismissed, by notice id. */
export type Dismissals = Record<string, string[]>;

/** Whether a notice now showing `current` stays hidden after `dismissed` was closed. */
export function hiddenBy(
  dismissed: readonly string[] | undefined,
  current: readonly string[],
): boolean {
  if (!dismissed || current.length === 0) return false;
  const left = new Map<string, number>();
  for (const d of dismissed) left.set(d, (left.get(d) ?? 0) + 1);
  for (const c of current) {
    const n = left.get(c) ?? 0;
    if (n === 0) return false;
    left.set(c, n - 1);
  }
  return true;
}

/** Storage can be missing, blocked or hold junk: then nothing is dismissed. */
export function loadDismissals(storage: StorageLike | null): Dismissals {
  if (!storage) return {};
  let raw: unknown;
  try {
    raw = JSON.parse(storage.getItem(KEY) ?? '{}');
  } catch {
    return {};
  }
  if (typeof raw !== 'object' || raw === null) return {};
  const out: Dismissals = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (Array.isArray(v) && v.every((x) => typeof x === 'string')) out[k] = v;
  }
  return out;
}

export function saveDismissals(storage: StorageLike | null, dismissals: Dismissals): void {
  if (!storage) return;
  try {
    storage.setItem(KEY, JSON.stringify(dismissals));
  } catch {
    // Not remembering it only means the notice shows again after a reload.
  }
}
