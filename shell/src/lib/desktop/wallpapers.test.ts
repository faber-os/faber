import { describe, expect, it } from 'vitest';
import { DEFAULT_WALLPAPER, WALLPAPERS, resolveWallpaper } from './wallpapers';

describe('wallpapers', () => {
  it('has aluminium as the default and a few more', () => {
    expect(DEFAULT_WALLPAPER).toBe('aluminium');
    expect(WALLPAPERS.length).toBeGreaterThanOrEqual(3);
  });

  it('falls back to the default for an id it does not know', () => {
    expect(resolveWallpaper('graphite')).toBe('graphite');
    expect(resolveWallpaper('sunset')).toBe('aluminium');
    expect(resolveWallpaper(undefined)).toBe('aluminium');
  });
});
