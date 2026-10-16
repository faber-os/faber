import { describe, expect, it } from 'vitest';
import { lineId, loadTicks, saveTicks } from './wiringChecklist';

function memory(): Storage & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  } as unknown as Storage & { data: Map<string, string> };
}

describe('the wiring checklist’s ticks', () => {
  it('are kept per board, and only for lines that still exist', () => {
    const s = memory();
    const a = lineId('front', 'Trig → D7');
    const b = lineId('front', 'Echo → D6');
    saveTicks(s, 'uno', new Set([a, b, 'gone']), [a, b]);
    expect(loadTicks(s, 'uno')).toEqual(new Set([a, b]));
    expect(loadTicks(s, 'mega')).toEqual(new Set());
    // Echo moved to D8: its line is new, so it isn't ticked.
    expect(loadTicks(s, 'uno').has(lineId('front', 'Echo → D8'))).toBe(false);
  });

  it('survive storage that is missing, broken or throws', () => {
    expect(loadTicks(null, 'uno')).toEqual(new Set());
    const junk = memory();
    junk.setItem('faber.wiring.v1.uno', '{not json');
    expect(loadTicks(junk, 'uno')).toEqual(new Set());
    const throwing = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('full');
      },
    };
    expect(loadTicks(throwing, 'uno')).toEqual(new Set());
    expect(() => saveTicks(throwing, 'uno', new Set(['x']), ['x'])).not.toThrow();
  });
});
