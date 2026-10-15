export type ThemePreference = 'system' | 'light' | 'dark';
export type Theme = 'light' | 'dark';

export function resolveTheme(pref: ThemePreference, systemDark: boolean): Theme {
  if (pref === 'system') return systemDark ? 'dark' : 'light';
  return pref;
}

export function parseThemePreference(stored: string | null): ThemePreference {
  // Light is the default (core/src/settings.rs); only an explicit choice changes it.
  return stored === 'system' || stored === 'dark' ? stored : 'light';
}
