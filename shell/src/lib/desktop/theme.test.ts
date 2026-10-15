import { describe, expect, it } from 'vitest';
import { nextThemePreference, parseThemePreference, resolveTheme } from './theme';

describe('theme', () => {
  it('follows the system unless the user picked one', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });

  it('cycles system → light → dark → system', () => {
    expect(nextThemePreference('system')).toBe('light');
    expect(nextThemePreference('light')).toBe('dark');
    expect(nextThemePreference('dark')).toBe('system');
  });

  it('reads a stored preference and falls back to system', () => {
    expect(parseThemePreference('dark')).toBe('dark');
    expect(parseThemePreference(null)).toBe('system');
    expect(parseThemePreference('purple')).toBe('system');
  });
});
