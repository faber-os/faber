import { getContext, setContext } from 'svelte';
import type { StorageLike } from '../lib/desktop/geometryStore';
import { parseThemePreference, resolveTheme, type ThemePreference } from '../lib/desktop/theme';

const STORAGE_KEY = 'faber.theme';

/**
 * Light or dark. The choice lives in the core's settings (shared by every screen); this keeps a
 * copy in the browser only so index.html can pick the right colours before the first paint.
 */
export class ThemeController {
  preference = $state<ThemePreference>('system');
  systemDark = $state(false);
  resolved = $derived(resolveTheme(this.preference, this.systemDark));

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
    try {
      this.storage?.setItem(STORAGE_KEY, pref);
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
