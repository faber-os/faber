import { describe, expect, it } from 'vitest';
import { loadGeometry, saveGeometry, type StorageLike } from './geometryStore';

function memory(): StorageLike & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
  };
}

describe('geometry store', () => {
  it('round-trips window geometry', () => {
    const s = memory();
    saveGeometry(s, { parts: { x: 1, y: 2, w: 300, h: 400 } });
    expect(loadGeometry(s)).toEqual({ parts: { x: 1, y: 2, w: 300, h: 400 } });
  });

  it('works without storage', () => {
    expect(loadGeometry(null)).toEqual({});
    expect(() => saveGeometry(null, {})).not.toThrow();
  });

  it('survives storage that throws (private mode, blocked site data)', () => {
    const broken: StorageLike = {
      getItem: () => {
        throw new Error('SecurityError');
      },
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    };
    expect(loadGeometry(broken)).toEqual({});
    expect(() => saveGeometry(broken, { a: { x: 0, y: 0, w: 1, h: 1 } })).not.toThrow();
  });

  it('ignores stored data that is not valid geometry', () => {
    const s = memory();
    s.data.set('faber.windows.v1', '{"parts":{"x":"a"},"ok":{"x":1,"y":2,"w":3,"h":4},"n":null}');
    expect(loadGeometry(s)).toEqual({ ok: { x: 1, y: 2, w: 3, h: 4 } });
    s.data.set('faber.windows.v1', 'not json');
    expect(loadGeometry(s)).toEqual({});
  });
});
