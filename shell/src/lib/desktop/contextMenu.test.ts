import { describe, expect, it } from 'vitest';
import {
  isMenuTrigger,
  item,
  menuKey,
  menuPosition,
  moveFocus,
  normaliseMenu,
  separator,
  typeahead,
  type MenuEntry,
} from './contextMenu';

const entries: MenuEntry[] = [
  item('open', 'Open'),
  item('notes', 'Open with Notes'),
  separator(),
  item('rename', 'Rename', { shortcut: 'F2' }),
  item('trash', 'Move to Trash', { disabledReason: 'Desktop is a fixed folder.', danger: true }),
  separator(),
  item('props', 'Properties'),
];

describe('menu model', () => {
  it('builds items and separators', () => {
    expect(entries[3]).toEqual({ kind: 'item', id: 'rename', label: 'Rename', shortcut: 'F2' });
    expect(entries[4]).toMatchObject({ disabledReason: 'Desktop is a fixed folder.' });
    expect(entries[2]).toEqual({ kind: 'separator' });
  });

  it('drops separators at the ends and doubled separators', () => {
    const tidy = normaliseMenu([
      separator(),
      item('a', 'A'),
      separator(),
      separator(),
      item('b', 'B'),
      separator(),
    ]);
    expect(tidy.map((e) => (e.kind === 'item' ? e.id : '-'))).toEqual(['a', '-', 'b']);
  });

  it('can leave items out with false, so menus read like a list', () => {
    const tidy = normaliseMenu([item('a', 'A'), false, separator(), false, item('b', 'B')]);
    expect(tidy).toHaveLength(3);
  });
});

describe('keyboard', () => {
  it('moves between items, skipping separators but not disabled items (they say why)', () => {
    expect(moveFocus(entries, 1, 'next')).toBe(3);
    expect(moveFocus(entries, 3, 'next')).toBe(4);
    expect(moveFocus(entries, 4, 'next')).toBe(6);
    expect(moveFocus(entries, 6, 'next')).toBe(0);
    expect(moveFocus(entries, 0, 'prev')).toBe(6);
    expect(moveFocus(entries, -1, 'next')).toBe(0);
    expect(moveFocus(entries, 3, 'first')).toBe(0);
    expect(moveFocus(entries, 3, 'last')).toBe(6);
  });

  it('is -1 for a menu with no items', () => {
    expect(moveFocus([separator()], -1, 'next')).toBe(-1);
  });

  it('jumps to the next item starting with a typed letter', () => {
    expect(typeahead(entries, 0, 'o')).toBe(1);
    expect(typeahead(entries, 1, 'O')).toBe(0);
    expect(typeahead(entries, 0, 'p')).toBe(6);
    expect(typeahead(entries, 0, 'z')).toBe(0);
  });

  it('maps keys to menu actions', () => {
    expect(menuKey('ArrowDown')).toBe('next');
    expect(menuKey('ArrowUp')).toBe('prev');
    expect(menuKey('Home')).toBe('first');
    expect(menuKey('End')).toBe('last');
    expect(menuKey('Enter')).toBe('activate');
    expect(menuKey(' ')).toBe('activate');
    expect(menuKey('Escape')).toBe('close');
    expect(menuKey('Tab')).toBe('close');
    expect(menuKey('x')).toBeNull();
  });

  it('opens a menu with Shift+F10 or the ContextMenu key', () => {
    expect(isMenuTrigger({ key: 'F10', shiftKey: true })).toBe(true);
    expect(isMenuTrigger({ key: 'F10', shiftKey: false })).toBe(false);
    expect(isMenuTrigger({ key: 'ContextMenu', shiftKey: false })).toBe(true);
    expect(isMenuTrigger({ key: 'Enter', shiftKey: true })).toBe(false);
  });
});

describe('menuPosition', () => {
  const viewport = { width: 1024, height: 600 };
  const size = { w: 220, h: 300 };

  it('opens to the right of and below the pointer when it fits', () => {
    expect(menuPosition({ x: 100, y: 50 }, size, viewport)).toEqual({ x: 100, y: 50 });
  });

  it('flips left and up near the right and bottom edges', () => {
    expect(menuPosition({ x: 1000, y: 580 }, size, viewport)).toEqual({ x: 780, y: 280 });
  });

  it('stays on screen when it fits neither way', () => {
    const p = menuPosition({ x: 10, y: 200 }, { w: 220, h: 700 }, viewport);
    expect(p).toEqual({ x: 10, y: 4 });
  });

  it('keeps a margin from the edges', () => {
    expect(menuPosition({ x: 0, y: 0 }, size, viewport)).toEqual({ x: 4, y: 4 });
  });
});
