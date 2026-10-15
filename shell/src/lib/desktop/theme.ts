export type ThemePreference = 'system' | 'light' | 'dark';
export type Theme = 'light' | 'dark';

export function resolveTheme(pref: ThemePreference, systemDark: boolean): Theme {
  if (pref === 'system') return systemDark ? 'dark' : 'light';
  return pref;
}

export function nextThemePreference(pref: ThemePreference): ThemePreference {
  return pref === 'system' ? 'light' : pref === 'light' ? 'dark' : 'system';
}

export function parseThemePreference(stored: string | null): ThemePreference {
  return stored === 'light' || stored === 'dark' ? stored : 'system';
}
