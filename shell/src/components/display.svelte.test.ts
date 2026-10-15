import { describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { DisplayPrefs } from './display.svelte';

function media(matches: boolean) {
  let listener: ((e: { matches: boolean }) => void) | null = null;
  return {
    list: {
      matches,
      addEventListener: (_: string, l: (e: { matches: boolean }) => void) => (listener = l),
    } as unknown as MediaQueryList,
    change: (m: boolean) => listener?.({ matches: m }),
  };
}

describe('DisplayPrefs', () => {
  it('reduces motion when the screen asks and the setting follows it, or when set to', () => {
    const m = media(true);
    const d = new DisplayPrefs(m.list);
    expect(d.reduceMotion).toBe(true);
    m.change(false);
    flushSync();
    expect(d.reduceMotion).toBe(false);
    d.set({ reduceMotion: 'on', higherContrast: 'system', textSize: '100' });
    flushSync();
    expect(d.reduceMotion).toBe(true);
    d.set({ reduceMotion: 'off', higherContrast: 'system', textSize: '100' });
    m.change(true);
    flushSync();
    expect(d.reduceMotion).toBe(false);
  });

  it('scales text to the chosen size', () => {
    const d = new DisplayPrefs(null);
    expect(d.fontSize).toBe('100%');
    d.set({ reduceMotion: 'system', higherContrast: 'system', textSize: '130' });
    flushSync();
    expect(d.fontSize).toBe('130%');
  });
});
