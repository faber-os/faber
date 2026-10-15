/**
 * The built-in styles (Settings → Appearance → Style): the colours of the whole shell. Each style
 * has a light and a dark version; the theme (System, Light, Dark) picks which.
 *
 * A style is a few numbers: the tint of its greys and the colour of its accent. `styleTokens`
 * turns them into every colour token the shell uses, so no style can miss one, and
 * styles.test.ts checks that every style is readable (WCAG contrast) in both versions.
 *
 * tokens.css holds the Aluminium values too, so the page has colours before this runs.
 */

export type ThemeName = 'light' | 'dark';

interface Oklch {
  l: number;
  c: number;
  h: number;
}

export interface StyleSpec {
  id: string;
  label: string;
  description: string;
  /** Hue of the greys, and how strongly they are tinted (1 = Aluminium's hint of blue). */
  greyHue: number;
  greyTint: number;
  accent: { light: Oklch; dark: Oklch };
}

export const STYLES: readonly StyleSpec[] = [
  {
    id: 'aluminium',
    label: 'Aluminium',
    description: 'Brushed metal with an anodised teal, the default',
    greyHue: 250,
    greyTint: 1,
    accent: { light: { l: 0.487, c: 0.083, h: 186 }, dark: { l: 0.74, c: 0.095, h: 184 } },
  },
  {
    id: 'cobalt',
    label: 'Cobalt',
    description: 'Cool greys and a strong blue',
    greyHue: 262,
    greyTint: 2,
    accent: { light: { l: 0.5, c: 0.19, h: 265 }, dark: { l: 0.72, c: 0.13, h: 267 } },
  },
  {
    id: 'workshop',
    label: 'Workshop',
    description: 'Warm greys and a green like a cutting mat',
    greyHue: 85,
    greyTint: 2.5,
    accent: { light: { l: 0.5, c: 0.115, h: 143 }, dark: { l: 0.76, c: 0.112, h: 142 } },
  },
  {
    id: 'blueprint',
    label: 'Blueprint',
    description: 'Blue paper and navy ink',
    greyHue: 252,
    greyTint: 5,
    accent: { light: { l: 0.4, c: 0.08, h: 257 }, dark: { l: 0.78, c: 0.06, h: 256 } },
  },
  {
    id: 'graphite',
    label: 'Graphite',
    description: 'Pure greys, no colour at all',
    greyHue: 250,
    greyTint: 0,
    accent: { light: { l: 0.37, c: 0.012, h: 248 }, dark: { l: 0.84, c: 0.01, h: 243 } },
  },
  {
    id: 'signal',
    label: 'Signal',
    description: 'Neutral greys and a signal red',
    greyHue: 100,
    greyTint: 0.6,
    accent: { light: { l: 0.5, c: 0.18, h: 30 }, dark: { l: 0.78, c: 0.128, h: 22 } },
  },
];

export const DEFAULT_STYLE = 'aluminium';

/** A style this shell knows; an unknown id (from a newer or older Faber) falls back. */
export function resolveStyle(id: string | undefined): StyleSpec {
  return STYLES.find((s) => s.id === id) ?? STYLES[0]!;
}

/**
 * The colours of each program's icon (desktop plates, start menu, taskbar), the same in every
 * style so a program is always found by its colour. Light: on a white plate; dark: on a dark one.
 */
export const APP_COLOURS: Record<string, { light: Oklch; dark: Oklch }> = {
  parts: { light: { l: 0.487, c: 0.083, h: 186 }, dark: { l: 0.78, c: 0.1, h: 184 } },
  files: { light: { l: 0.58, c: 0.13, h: 68 }, dark: { l: 0.82, c: 0.13, h: 80 } },
  notes: { light: { l: 0.5, c: 0.02, h: 248 }, dark: { l: 0.8, c: 0.02, h: 248 } },
  serial: { light: { l: 0.47, c: 0.09, h: 248 }, dark: { l: 0.78, c: 0.09, h: 248 } },
  settings: { light: { l: 0.37, c: 0.015, h: 248 }, dark: { l: 0.86, c: 0.01, h: 248 } },
  logs: { light: { l: 0.5, c: 0.1, h: 296 }, dark: { l: 0.78, c: 0.09, h: 296 } },
  viewer: { light: { l: 0.48, c: 0.1, h: 160 }, dark: { l: 0.8, c: 0.1, h: 160 } },
  about: { light: { l: 0.45, c: 0.02, h: 250 }, dark: { l: 0.82, c: 0.02, h: 250 } },
};

/** The plate behind a program icon. */
export const PLATE: Record<ThemeName, string> = {
  light: 'oklch(0.995 0.001 250)',
  dark: 'oklch(0.31 0.005 250)',
};

function ok({ l, c, h }: Oklch, alpha?: number): string {
  const n = (x: number) => String(Math.round(x * 1000) / 1000);
  return alpha === undefined
    ? `oklch(${n(l)} ${n(c)} ${n(h)})`
    : `oklch(${n(l)} ${n(c)} ${n(h)} / ${n(alpha)})`;
}

/**
 * Every colour token of one style in one theme, as CSS custom properties (`--surface` …).
 * Status colours (ok, warn, danger, info) are not part of a style: they mean the same everywhere.
 */
export function styleTokens(style: StyleSpec, theme: ThemeName): Record<string, string> {
  const h = style.greyHue;
  // Backgrounds take the full tint; text only a little of it, so it stays crisp.
  const bg = (l: number, c: number, a?: number) => ok({ l, c: c * style.greyTint, h }, a);
  const fg = (l: number, c: number, a?: number) =>
    ok({ l, c: c * Math.min(style.greyTint, 1.5), h }, a);
  const A = style.accent[theme];
  const at = (dl: number, dc = 0, a?: number) => ok({ l: A.l + dl, c: A.c + dc, h: A.h }, a);

  const tokens: Record<string, string> =
    theme === 'light'
      ? {
          '--surface': bg(0.993, 0.001),
          '--surface-2': bg(0.974, 0.002),
          '--surface-3': bg(0.948, 0.003),
          '--surface-sunken': bg(0.966, 0.002),
          '--chrome': bg(0.957, 0.003),
          '--chrome-tabs': bg(0.925, 0.004),
          '--taskbar': bg(0.944, 0.003),
          '--menu': bg(0.993, 0.001),
          '--line': bg(0.888, 0.004),
          '--line-strong': bg(0.8, 0.006),
          '--text': fg(0.235, 0.008),
          '--text-muted': fg(0.46, 0.01),
          '--text-faint': fg(0.6, 0.008),
          '--hover': fg(0.3, 0.01, 0.055),
          '--press': fg(0.3, 0.01, 0.1),
          '--scrim': fg(0.2, 0.01, 0.28),
          '--neutral': fg(0.46, 0.008),
          '--neutral-soft': fg(0.5, 0.008, 0.11),
          '--scan-bg': bg(0.982, 0.002),
          '--scan-ring': fg(0.3, 0.01, 0.12),
          '--scan-axis': fg(0.3, 0.01, 0.2),
          '--scan-label': fg(0.46, 0.01),

          '--accent': at(0),
          '--accent-hover': at(-0.057, -0.005),
          '--accent-text': ok({ l: 0.99, c: 0.004, h: A.h }),
          '--accent-soft': at(0.073, 0.007, 0.11),
          '--accent-ink': at(-0.047, -0.005),
          '--selected': at(0.073, 0.007, 0.17),
          '--focus-ring': at(0.073, 0.012),
          '--scan-origin': at(0),
        }
      : {
          '--surface': bg(0.238, 0.003),
          '--surface-2': bg(0.215, 0.003),
          '--surface-3': bg(0.29, 0.004),
          '--surface-sunken': bg(0.198, 0.003),
          '--chrome': bg(0.262, 0.004),
          '--chrome-tabs': bg(0.205, 0.004),
          '--taskbar': bg(0.205, 0.004),
          '--menu': bg(0.262, 0.004),
          '--line': bg(0.335, 0.004),
          '--line-strong': bg(0.42, 0.006),
          '--text': fg(0.94, 0.003),
          '--text-muted': fg(0.76, 0.006),
          '--text-faint': fg(0.63, 0.006),
          '--hover': fg(0.95, 0.005, 0.07),
          '--press': fg(0.95, 0.005, 0.12),
          '--scrim': 'oklch(0 0 0 / 0.45)',
          '--neutral': fg(0.76, 0.006),
          '--neutral-soft': fg(0.8, 0.006, 0.11),
          '--scan-bg': bg(0.19, 0.003),
          '--scan-ring': fg(0.95, 0.005, 0.1),
          '--scan-axis': fg(0.95, 0.005, 0.16),
          '--scan-label': fg(0.72, 0.006),

          '--accent': at(0),
          '--accent-hover': at(0.05, -0.005),
          '--accent-text': ok({ l: 0.2, c: Math.min(0.03, A.c), h: A.h }),
          '--accent-soft': at(0, 0, 0.14),
          '--accent-ink': at(0.06, -0.01),
          '--selected': at(0, 0, 0.22),
          '--focus-ring': at(0.02, 0.005),
          '--scan-origin': at(0),
        };

  tokens['--plate'] = PLATE[theme];
  for (const [name, colours] of Object.entries(APP_COLOURS)) {
    tokens[`--app-${name}`] = ok(colours[theme]);
  }
  return tokens;
}

/** The swatch colours a style card shows: its desktop and its accent, light and dark. */
export function styleSwatch(style: StyleSpec): {
  light: string;
  dark: string;
  accentLight: string;
  accentDark: string;
} {
  const light = styleTokens(style, 'light');
  const dark = styleTokens(style, 'dark');
  return {
    light: light['--surface-3']!,
    dark: dark['--surface']!,
    accentLight: light['--accent']!,
    accentDark: dark['--accent']!,
  };
}
