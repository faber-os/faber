import { getContext, setContext } from 'svelte';
import type { StorageLike } from '../lib/desktop/geometryStore';
import { parseThemePreference, resolveTheme, type ThemePreference } from '../lib/desktop/theme';
import { resolveStyle, styleTokens } from '../lib/desktop/styles';

const STORAGE_KEY = 'faber.theme';
/** The last style's colours, both themes, for index.html to paint with before the shell runs. */
export const TOKENS_KEY = 'faber.tokens';

/**
 * Light or dark, and the style (colours). The choices live in the core's settings (shared by
 * every screen); this keeps a copy in the browser only so index.html can pick the right colours
 * before the first paint.
 */
export class ThemeController {
  preference = $state<ThemePreference>('system');
  systemDark = $state(false);
  /** '' until the settings have said; the colours are Aluminium's meanwhile. */
  style = $state('');
  resolved = $derived(resolveTheme(this.preference, this.systemDark));
  /** Every colour token of the current style and theme, as CSS custom properties. */
  tokens = $derived(styleTokens(resolveStyle(this.style), this.resolved));

  private readonly storage: StorageLike | null;

  constructor(storage: StorageLike | null, media: MediaQueryList | null) {
    this.storage = storage;
    try {
      this.preference = parseThemePreference(storage?.getItem(STORAGE_KEY) ?? null);
    } catch {
      this.preference = 'system';
    }
    if (media) {
      this.systemDark = media.matches;
      media.addEventListener('change', (e) => (this.systemDark = e.matches));
    }
  }

  /** Follow the settings (called whenever they arrive or change). */
  setPreference(pref: ThemePreference): void {
    if (pref === this.preference) return;
    this.preference = pref;
    this.remember(STORAGE_KEY, pref);
  }

  setStyle(id: string): void {
    const style = resolveStyle(id);
    if (style.id === this.style) return;
    this.style = style.id;
    this.remember(
      TOKENS_KEY,
      JSON.stringify({ light: styleTokens(style, 'light'), dark: styleTokens(style, 'dark') }),
    );
  }

  private remember(key: string, value: string): void {
    try {
      this.storage?.setItem(key, value);
    } catch {
      // Only the first paint of the next visit may use the old colours.
    }
  }
}

const KEY = Symbol('faber-theme');

export function provideTheme(t: ThemeController): void {
  setContext(KEY, t);
}

export function useTheme(): ThemeController {
  const t = getContext<ThemeController | undefined>(KEY);
  if (!t) throw new Error('useTheme() needs provideTheme() in a parent component.');
  return t;
}

export const THEME_CONTEXT_KEY = KEY;
