/**
 * The start menu's logic: searching programs, parts and recent files, the recent-files list,
 * and the power menu.
 */

export interface LaunchProgram {
  key: string;
  kind: string;
  title: string;
  description: string;
  /** "Tool" or "Accessory", shown beside a search result. */
  group: string;
}

export interface LaunchPart {
  name: string;
  /** A laser-scan signal it has, if any: then the result opens its viewer. */
  viewer?: string;
}

export interface RecentFile {
  path: string;
  name: string;
  /** When it was last opened (ms since the epoch). */
  at: number;
}

export interface SearchResults {
  programs: LaunchProgram[];
  parts: LaunchPart[];
  files: RecentFile[];
}

/**
 * Everything whose name contains the query (any case), names starting with it first. Programs
 * also match on their description, so "serial" finds the serial monitor and "text" finds Notes.
 */
export function searchLauncher(
  query: string,
  sources: { programs: LaunchProgram[]; parts: LaunchPart[]; files: RecentFile[] },
): SearchResults {
  const q = query.trim().toLowerCase();
  if (q === '') return { programs: [], parts: [], files: [] };
  const rank = (name: string, extra = '') => {
    const n = name.toLowerCase();
    if (n.startsWith(q)) return 0;
    if (n.includes(q)) return 1;
    if (extra.toLowerCase().includes(q)) return 2;
    return -1;
  };
  function pick<T>(list: T[], name: (x: T) => string, extra?: (x: T) => string): T[] {
    return list
      .map((x, i) => ({ x, i, r: rank(name(x), extra?.(x)) }))
      .filter((m) => m.r >= 0)
      .sort((a, b) => a.r - b.r || a.i - b.i)
      .map((m) => m.x);
  }
  return {
    programs: pick(
      sources.programs,
      (p) => p.title,
      (p) => p.description,
    ),
    parts: pick(sources.parts, (p) => p.name),
    files: pick(sources.files, (f) => f.name),
  };
}

export const RECENT_LIMIT = 8;

/** Opening a file puts it at the top; the list keeps the newest few, each file once. */
export function addRecent(list: readonly RecentFile[], file: RecentFile): RecentFile[] {
  return [file, ...list.filter((f) => f.path !== file.path)].slice(0, RECENT_LIMIT);
}

/** A file that moved keeps its place in the list under its new path. */
export function renameRecent(list: readonly RecentFile[], from: string, to: string): RecentFile[] {
  return list.map((f) =>
    f.path === from ? { ...f, path: to, name: to.slice(to.lastIndexOf('/') + 1) } : f,
  );
}

const RECENT_KEY = 'faber.recent.v1';

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** Recent files live in this browser only; anything unreadable is simply an empty list. */
export function loadRecent(storage: StorageLike | null): RecentFile[] {
  try {
    const raw = JSON.parse(storage?.getItem(RECENT_KEY) ?? '[]') as unknown;
    if (!Array.isArray(raw)) return [];
    return raw
      .filter(
        (f): f is RecentFile =>
          typeof f === 'object' &&
          f !== null &&
          typeof (f as RecentFile).path === 'string' &&
          typeof (f as RecentFile).name === 'string' &&
          typeof (f as RecentFile).at === 'number',
      )
      .slice(0, RECENT_LIMIT);
  } catch {
    return [];
  }
}

export function saveRecent(storage: StorageLike | null, list: readonly RecentFile[]): void {
  try {
    storage?.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Storage full or blocked: the list just isn't remembered.
  }
}

export interface PowerAction {
  id: 'restart' | 'shutdown';
  label: string;
  disabledReason?: string;
}

export const POWER_UNAVAILABLE = 'Available on the Faber machine image (Phase 6)';

/**
 * The power button's menu. The core has no power endpoint yet (it comes with the machine image),
 * so both are shown, disabled, with the reason, rather than hidden.
 */
export function powerActions(canPower: boolean): PowerAction[] {
  const reason = canPower ? undefined : POWER_UNAVAILABLE;
  const a = (id: PowerAction['id'], label: string): PowerAction =>
    reason ? { id, label, disabledReason: reason } : { id, label };
  return [a('restart', 'Restart machine'), a('shutdown', 'Shut down machine')];
}

/** "10:02" today, "Yesterday", or a date. */
export function recentWhen(at: number, now: number): string {
  const d = new Date(at);
  const today = new Date(now);
  const sameDay = (x: Date, y: Date) =>
    x.getFullYear() === y.getFullYear() &&
    x.getMonth() === y.getMonth() &&
    x.getDate() === y.getDate();
  if (sameDay(d, today)) {
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }
  const y = new Date(now);
  y.setDate(y.getDate() - 1);
  if (sameDay(d, y)) return 'Yesterday';
  return `${d.getDate()}-${d.getMonth() + 1}-${d.getFullYear()}`;
}
