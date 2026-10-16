/**
 * Selecting desktop icons with the mouse, as on Windows: a click selects one icon, Ctrl+click
 * adds or takes away one, Shift+click adds every icon from the last clicked one (the anchor) to
 * this one, in keyboard order.
 */

export interface Selection {
  keys: string[];
  /** Where a Shift+click range starts: the last icon clicked without Shift. */
  anchor: string | null;
}

export function clickSelect(
  sel: Selection,
  key: string,
  mods: { ctrl: boolean; shift: boolean },
  order: readonly string[],
): Selection {
  if (mods.shift && sel.anchor !== null) {
    const a = order.indexOf(sel.anchor);
    const b = order.indexOf(key);
    if (a >= 0 && b >= 0) {
      const range = order.slice(Math.min(a, b), Math.max(a, b) + 1);
      return {
        keys: [...sel.keys, ...range.filter((k) => !sel.keys.includes(k))],
        anchor: sel.anchor,
      };
    }
  }
  if (mods.shift) {
    return { keys: sel.keys.includes(key) ? sel.keys : [...sel.keys, key], anchor: key };
  }
  if (mods.ctrl) {
    const keys = sel.keys.includes(key) ? sel.keys.filter((k) => k !== key) : [...sel.keys, key];
    return { keys, anchor: key };
  }
  return { keys: [key], anchor: key };
}
