import { describe, expect, it } from 'vitest';
import { hiddenBy, loadDismissals, saveDismissals } from './dismissals';
import type { StorageLike } from './geometryStore';

function memory(): StorageLike & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
  };
}

describe('hiddenBy', () => {
  it('hides a notice while it shows nothing that was not dismissed', () => {
    expect(hiddenBy(['a', 'a'], ['a', 'a'])).toBe(true);
    // One of the problems went away: still nothing new.
    expect(hiddenBy(['a', 'a'], ['a'])).toBe(true);
    expect(hiddenBy(['a', 'b'], ['b'])).toBe(true);
  });

  it('brings it back for a new problem, or one more of the same', () => {
    expect(hiddenBy(['a'], ['a', 'b'])).toBe(false);
    expect(hiddenBy(['a', 'a'], ['a', 'a', 'a'])).toBe(false);
    expect(hiddenBy(undefined, ['a'])).toBe(false);
  });

  it('has nothing to hide when there is nothing to show', () => {
    expect(hiddenBy(['a'], [])).toBe(false);
  });
});

describe('remembering dismissals', () => {
  it('keeps them per notice in this browser', () => {
    const s = memory();
    saveDismissals(s, { portProblems: ['usb-no-answer', 'usb-no-answer'] });
    expect(loadDismissals(s)).toEqual({ portProblems: ['usb-no-answer', 'usb-no-answer'] });
  });

  it('starts empty without storage, with storage that throws, or with junk in it', () => {
    expect(loadDismissals(null)).toEqual({});
    const throwing: StorageLike = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(loadDismissals(throwing)).toEqual({});
    expect(() => saveDismissals(throwing, { a: ['x'] })).not.toThrow();
    const s = memory();
    s.data.set('faber.dismissed.v1', '{"a": "x", "b": [1, "y"], "c": ["z"]}');
    expect(loadDismissals(s)).toEqual({ c: ['z'] });
    s.data.set('faber.dismissed.v1', 'not json');
    expect(loadDismissals(s)).toEqual({});
  });
});
