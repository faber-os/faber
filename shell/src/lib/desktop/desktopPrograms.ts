/**
 * Which programs have an icon on the desktop. Every program does, unless the user removed it
 * (Settings.desktop.removed), so a program added in a later version of Faber shows up by itself.
 * Where the icons sit is Settings.desktop.icons (iconGrid.ts).
 */
import type { SavedPosition } from './iconGrid';

export interface DesktopLayoutLike {
  icons: SavedPosition[];
  removed: string[];
}

/** The programs to show, in the order given. */
export function onDesktop(programs: readonly string[], layout: DesktopLayoutLike): string[] {
  const removed = new Set(layout.removed);
  return programs.filter((p) => !removed.has(p));
}

/** Removed for good: it stays off the desktop until it is added back. */
export function removeFromDesktop<L extends DesktopLayoutLike>(layout: L, key: string): L {
  return {
    ...layout,
    icons: layout.icons.filter((p) => p.key !== key),
    removed: layout.removed.includes(key) ? layout.removed : [...layout.removed, key],
  };
}

/** Back on the desktop, in the next free cell. */
export function addToDesktop<L extends DesktopLayoutLike>(layout: L, key: string): L {
  return { ...layout, removed: layout.removed.filter((k) => k !== key) };
}
