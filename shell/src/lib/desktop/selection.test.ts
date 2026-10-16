import { describe, expect, it } from 'vitest';
import { clickSelect } from './selection';

const ORDER = ['a', 'b', 'c', 'd', 'e'];
const none = { ctrl: false, shift: false };

describe('clickSelect', () => {
  it('selects just the clicked icon on a plain click', () => {
    expect(clickSelect({ keys: ['a', 'b'], anchor: 'a' }, 'c', none, ORDER)).toEqual({
      keys: ['c'],
      anchor: 'c',
    });
  });

  it('adds or takes away one icon with Ctrl', () => {
    const ctrl = { ctrl: true, shift: false };
    expect(clickSelect({ keys: ['a'], anchor: 'a' }, 'c', ctrl, ORDER)).toEqual({
      keys: ['a', 'c'],
      anchor: 'c',
    });
    expect(clickSelect({ keys: ['a', 'c'], anchor: 'c' }, 'a', ctrl, ORDER)).toEqual({
      keys: ['c'],
      anchor: 'a',
    });
  });

  it('adds everything from the last clicked icon to this one with Shift', () => {
    const shift = { ctrl: false, shift: true };
    expect(clickSelect({ keys: ['b'], anchor: 'b' }, 'd', shift, ORDER)).toEqual({
      keys: ['b', 'c', 'd'],
      anchor: 'b',
    });
    expect(clickSelect({ keys: ['e'], anchor: 'e' }, 'c', shift, ORDER)).toEqual({
      keys: ['e', 'c', 'd'],
      anchor: 'e',
    });
    // Without an anchor, Shift adds just the one.
    expect(clickSelect({ keys: [], anchor: null }, 'c', shift, ORDER).keys).toEqual(['c']);
  });
});
