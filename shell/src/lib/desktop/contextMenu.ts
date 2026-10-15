/**
 * Right-click menus as plain data: what is in a menu, which item has focus, and where it opens.
 * One Svelte component (components/ContextMenu.svelte) draws whatever these describe.
 *
 * Disabled items stay in the menu and stay focusable, with the reason they are disabled: hiding
 * them would hide what Faber can do, and a reason says what to change (docs/principles.md).
 */

export interface MenuItem {
  kind: 'item';
  id: string;
  label: string;
  /** Shown on the right, e.g. "F2". The handler that owns the menu also handles the key. */
  shortcut?: string;
  /** Set when the item can't be used right now, in words that say why. */
  disabledReason?: string;
  /** Destroys something (Delete for good). Drawn in the danger colour. */
  danger?: boolean;
}

export interface MenuSeparator {
  kind: 'separator';
}

export type MenuEntry = MenuItem | MenuSeparator;

export interface Point {
  x: number;
  y: number;
}

export type FocusMove = 'next' | 'prev' | 'first' | 'last';
export type MenuAction = FocusMove | 'activate' | 'close';

export function item(
  id: string,
  label: string,
  opts: Omit<MenuItem, 'kind' | 'id' | 'label'> = {},
): MenuItem {
  const out: MenuItem = { kind: 'item', id, label };
  if (opts.shortcut !== undefined) out.shortcut = opts.shortcut;
  if (opts.disabledReason !== undefined) out.disabledReason = opts.disabledReason;
  if (opts.danger) out.danger = true;
  return out;
}

export function separator(): MenuSeparator {
  return { kind: 'separator' };
}

/** Drop `false` (items left out), separators at either end and separators next to each other. */
export function normaliseMenu(entries: readonly (MenuEntry | false)[]): MenuEntry[] {
  const out: MenuEntry[] = [];
  for (const e of entries) {
    if (e === false) continue;
    if (e.kind === 'separator' && (out.length === 0 || out.at(-1)!.kind === 'separator')) continue;
    out.push(e);
  }
  while (out.at(-1)?.kind === 'separator') out.pop();
  return out;
}

/** The index that gets focus after `move` from `current` (-1: nothing focused yet). */
export function moveFocus(entries: readonly MenuEntry[], current: number, move: FocusMove): number {
  const items = entries.flatMap((e, i) => (e.kind === 'item' ? [i] : []));
  if (items.length === 0) return -1;
  if (move === 'first') return items[0]!;
  if (move === 'last') return items.at(-1)!;
  const at = items.indexOf(current);
  if (at === -1) return move === 'next' ? items[0]! : items.at(-1)!;
  const step = move === 'next' ? 1 : -1;
  return items[(at + step + items.length) % items.length]!;
}

/** The next item after `current` whose label starts with `char`, or `current` if there is none. */
export function typeahead(entries: readonly MenuEntry[], current: number, char: string): number {
  const c = char.toLowerCase();
  for (let n = 1; n <= entries.length; n++) {
    const i = (current + n) % entries.length;
    const e = entries[i];
    if (e?.kind === 'item' && e.label.toLowerCase().startsWith(c)) return i;
  }
  return current;
}

export function menuKey(key: string): MenuAction | null {
  switch (key) {
    case 'ArrowDown':
      return 'next';
    case 'ArrowUp':
      return 'prev';
    case 'Home':
      return 'first';
    case 'End':
      return 'last';
    case 'Enter':
    case ' ':
      return 'activate';
    case 'Escape':
    case 'Tab':
      return 'close';
    default:
      return null;
  }
}

/** Shift+F10 and the ContextMenu key open a menu from the keyboard, as on Windows. */
export function isMenuTrigger(e: { key: string; shiftKey: boolean }): boolean {
  return e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey);
}

const MARGIN = 4;

/**
 * Where a menu of `size` opens for a click at `anchor`: right and down when it fits, flipped left
 * or up when it doesn't, and always fully on screen with a small margin.
 */
export function menuPosition(
  anchor: Point,
  size: { w: number; h: number },
  viewport: { width: number; height: number },
): Point {
  let x = anchor.x;
  let y = anchor.y;
  if (x + size.w > viewport.width - MARGIN) x = anchor.x - size.w;
  if (y + size.h > viewport.height - MARGIN) y = anchor.y - size.h;
  x = Math.min(Math.max(x, MARGIN), Math.max(MARGIN, viewport.width - MARGIN - size.w));
  y = Math.min(Math.max(y, MARGIN), Math.max(MARGIN, viewport.height - MARGIN - size.h));
  return { x: Math.round(x), y: Math.round(y) };
}
