import { t } from '../i18n';

/** The built-in wallpapers (Settings → Appearance). Each is drawn in CSS, so the kiosk needs no network. */
export interface Wallpaper {
  id: string;
  /** In the current language (a getter, so it follows a language change). */
  readonly label: string;
  readonly description: string;
}

type WallpaperId = 'aluminium' | 'anodised' | 'cutting-mat' | 'graphite';

function wallpaper(id: WallpaperId): Wallpaper {
  return {
    id,
    get label() {
      return t(`settings.wallpaper.${id}.label`);
    },
    get description() {
      return t(`settings.wallpaper.${id}.description`);
    },
  };
}

export const WALLPAPERS: readonly Wallpaper[] = [
  wallpaper('aluminium'),
  wallpaper('anodised'),
  wallpaper('cutting-mat'),
  wallpaper('graphite'),
];

export const DEFAULT_WALLPAPER = 'aluminium';

/** A wallpaper this shell knows; an unknown id (from a newer or older Faber) falls back. */
export function resolveWallpaper(id: string | undefined): string {
  return WALLPAPERS.some((w) => w.id === id) ? id! : DEFAULT_WALLPAPER;
}
