/** The built-in wallpapers (Settings → Appearance). Each is drawn in CSS, so the kiosk needs no network. */
export interface Wallpaper {
  id: string;
  label: string;
  description: string;
}

export const WALLPAPERS: readonly Wallpaper[] = [
  { id: 'aluminium', label: 'Aluminium', description: 'Brushed metal, the default' },
  { id: 'anodised', label: 'Anodised', description: 'Deep teal, like an anodised part' },
  { id: 'cutting-mat', label: 'Cutting mat', description: 'A self-healing mat with its grid' },
  { id: 'graphite', label: 'Graphite', description: 'Plain dark grey, nothing to look at' },
];

export const DEFAULT_WALLPAPER = 'aluminium';

/** A wallpaper this shell knows; an unknown id (from a newer or older Faber) falls back. */
export function resolveWallpaper(id: string | undefined): string {
  return WALLPAPERS.some((w) => w.id === id) ? id! : DEFAULT_WALLPAPER;
}
