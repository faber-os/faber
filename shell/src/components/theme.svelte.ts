import { getContext, setContext } from 'svelte';
import type { StorageLike } from '../lib/desktop/geometryStore';
import {
  nextThemePreference,
  parseThemePreference,
  resolveTheme,
  type ThemePreference,
} from '../lib/desktop/theme';

const STORAGE_KEY = 'faber.theme';

/** Light/dark: follows the system until the user picks one with the taskbar toggle. */
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

  cycle(): void {
    this.preference = nextThemePreference(this.preference);
    try {
      this.storage?.setItem(STORAGE_KEY, this.preference);
    } catch {
      // Not remembered, but the theme still changes for this visit.
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
