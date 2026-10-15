import { describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { TOKENS_KEY, ThemeController } from './theme.svelte';

function memory() {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  };
}

describe('ThemeController', () => {
  it("gives the style's tokens for the theme in use", () => {
    const t = new ThemeController(null, null);
    t.setPreference('light');
    flushSync();
    expect(t.tokens['--accent']).toBe('oklch(0.487 0.083 186)');
    t.setStyle('cobalt');
    t.setPreference('dark');
    flushSync();
    expect(t.tokens['--accent']).toBe('oklch(0.72 0.13 267)');
  });

  it('remembers both versions of the style for the next first paint', () => {
    const storage = memory();
    const t = new ThemeController(storage, null);
    t.setStyle('workshop');
    const saved = JSON.parse(storage.data.get(TOKENS_KEY)!) as Record<
      string,
      Record<string, string>
    >;
    expect(Object.keys(saved)).toEqual(['light', 'dark']);
    expect(saved.light!['--accent']).toMatch(/^oklch\(/);
  });

  it('raises the contrast when set to, or when the screen asks and the setting follows it', () => {
    const plain = new ThemeController(null, null);
    plain.setPreference('light');
    const asks = { matches: true, addEventListener: () => {} } as unknown as MediaQueryList;
    const t = new ThemeController(null, null, asks);
    t.setPreference('light');
    flushSync();
    expect(t.boosted).toBe(true);
    expect(t.tokens['--text']).not.toBe(plain.tokens['--text']);
    t.setContrast('off');
    flushSync();
    expect(t.tokens['--text']).toBe(plain.tokens['--text']);
    t.setContrast('on');
    plain.setContrast('on');
    flushSync();
    expect(plain.boosted).toBe(true);
    expect(t.tokens).toEqual(plain.tokens);
  });

  it('remembers the boosted colours for the next first paint', () => {
    const storage = memory();
    const t = new ThemeController(storage, null);
    t.setStyle('workshop');
    const before = storage.data.get(TOKENS_KEY);
    t.setContrast('on');
    expect(storage.data.get(TOKENS_KEY)).not.toBe(before);
  });

  it('falls back to Aluminium for a style it does not know', () => {
    const t = new ThemeController(null, null);
    t.setStyle('neon');
    expect(t.style).toBe('aluminium');
  });
});
