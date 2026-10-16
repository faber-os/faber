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

import { t } from '../i18n';

export type ThemeName = 'light' | 'dark';

interface Oklch {
  l: number;
  c: number;
  h: number;
}

export interface StyleSpec {
  id: string;
  /** In the current language (a getter, so it follows a language change). */
  readonly label: string;
  readonly description: string;
  /** Hue of the greys, and how strongly they are tinted (1 = Aluminium's hint of blue). */
  greyHue: number;
  greyTint: number;
  accent: { light: Oklch; dark: Oklch };
}

type StyleId = 'aluminium' | 'cobalt' | 'workshop' | 'blueprint' | 'graphite' | 'signal';

/** A style with its name and description from the catalogue. */
function named(spec: Omit<StyleSpec, 'id' | 'label' | 'description'> & { id: StyleId }): StyleSpec {
  return {
    ...spec,
    get label() {
      return t(`settings.style.${spec.id}.label`);
    },
    get description() {
      return t(`settings.style.${spec.id}.description`);
    },
  };
}

export const STYLES: readonly StyleSpec[] = [
  named({
    id: 'aluminium',
    greyHue: 250,
    greyTint: 1,
    accent: { light: { l: 0.487, c: 0.083, h: 186 }, dark: { l: 0.74, c: 0.095, h: 184 } },
  }),
  named({
    id: 'cobalt',
    greyHue: 262,
    greyTint: 2,
    accent: { light: { l: 0.5, c: 0.19, h: 265 }, dark: { l: 0.72, c: 0.13, h: 267 } },
  }),
  named({
    id: 'workshop',
    greyHue: 85,
    greyTint: 2.5,
    accent: { light: { l: 0.5, c: 0.115, h: 143 }, dark: { l: 0.76, c: 0.112, h: 142 } },
  }),
  named({
    id: 'blueprint',
    greyHue: 252,
    greyTint: 5,
    accent: { light: { l: 0.4, c: 0.08, h: 257 }, dark: { l: 0.78, c: 0.06, h: 256 } },
  }),
  named({
    id: 'graphite',
    greyHue: 250,
    greyTint: 0,
    accent: { light: { l: 0.37, c: 0.012, h: 248 }, dark: { l: 0.84, c: 0.01, h: 243 } },
  }),
  named({
    id: 'signal',
    greyHue: 100,
    greyTint: 0.6,
    accent: { light: { l: 0.5, c: 0.18, h: 30 }, dark: { l: 0.78, c: 0.128, h: 22 } },
  }),
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
  drive: { light: { l: 0.5, c: 0.13, h: 40 }, dark: { l: 0.78, c: 0.12, h: 45 } },
  world: { light: { l: 0.5, c: 0.11, h: 128 }, dark: { l: 0.8, c: 0.11, h: 128 } },
  flasher: { light: { l: 0.5, c: 0.12, h: 340 }, dark: { l: 0.79, c: 0.11, h: 340 } },
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
 * The status colours: the same hue in every style, so a warning always looks like a warning.
 * `base` is for icons, dots and borders (3:1); `soft` is a translucent background for a notice
 * or a badge; `text` is status-coloured text, on the surfaces and on its own soft background
 * (4.5:1, and further with higher contrast). Text on a soft background that isn't in the status
 * colour uses `--text` or `--text-muted`, which styles.test.ts checks there too.
 */
const STATUS: Record<
  'ok' | 'warn' | 'danger' | 'info',
  Record<ThemeName, { base: Oklch; soft: [Oklch, number]; text: Oklch }>
> = {
  ok: {
    light: {
      base: { l: 0.47, c: 0.11, h: 152 },
      soft: [{ l: 0.62, c: 0.12, h: 152 }, 0.14],
      text: { l: 0.43, c: 0.1, h: 152 },
    },
    dark: {
      base: { l: 0.8, c: 0.12, h: 152 },
      soft: [{ l: 0.7, c: 0.12, h: 152 }, 0.16],
      text: { l: 0.84, c: 0.11, h: 152 },
    },
  },
  warn: {
    light: {
      base: { l: 0.5, c: 0.11, h: 70 },
      soft: [{ l: 0.8, c: 0.13, h: 80 }, 0.24],
      text: { l: 0.45, c: 0.095, h: 68 },
    },
    dark: {
      base: { l: 0.84, c: 0.12, h: 82 },
      soft: [{ l: 0.8, c: 0.13, h: 82 }, 0.16],
      text: { l: 0.87, c: 0.11, h: 84 },
    },
  },
  danger: {
    light: {
      base: { l: 0.5, c: 0.18, h: 27 },
      soft: [{ l: 0.62, c: 0.19, h: 27 }, 0.11],
      text: { l: 0.47, c: 0.17, h: 27 },
    },
    dark: {
      base: { l: 0.76, c: 0.14, h: 25 },
      soft: [{ l: 0.65, c: 0.17, h: 25 }, 0.18],
      text: { l: 0.82, c: 0.11, h: 25 },
    },
  },
  info: {
    light: {
      base: { l: 0.47, c: 0.08, h: 245 },
      soft: [{ l: 0.6, c: 0.09, h: 245 }, 0.13],
      text: { l: 0.43, c: 0.08, h: 245 },
    },
    dark: {
      base: { l: 0.8, c: 0.08, h: 245 },
      soft: [{ l: 0.7, c: 0.09, h: 245 }, 0.17],
      text: { l: 0.84, c: 0.07, h: 245 },
    },
  },
};

/**
 * The code editor's syntax colours (codemirror.ts): the same hues in every style, like the status
 * colours, so code reads the same everywhere. Each is text, checked at 4.5:1 on the editor's
 * background and its active line (styles.test.ts). Comments, operators and plain names use
 * `--text-muted` and `--text`; FaberLink's classes are the type colour in bold.
 */
export const CODE_COLOURS: Record<string, Record<ThemeName, Oklch>> = {
  keyword: { light: { l: 0.47, c: 0.17, h: 305 }, dark: { l: 0.79, c: 0.12, h: 305 } },
  type: { light: { l: 0.47, c: 0.085, h: 190 }, dark: { l: 0.81, c: 0.1, h: 190 } },
  function: { light: { l: 0.47, c: 0.15, h: 260 }, dark: { l: 0.8, c: 0.1, h: 258 } },
  api: { light: { l: 0.5, c: 0.14, h: 48 }, dark: { l: 0.8, c: 0.12, h: 55 } },
  constant: { light: { l: 0.49, c: 0.17, h: 350 }, dark: { l: 0.8, c: 0.12, h: 350 } },
  string: { light: { l: 0.47, c: 0.12, h: 145 }, dark: { l: 0.81, c: 0.13, h: 145 } },
  macro: { light: { l: 0.49, c: 0.15, h: 18 }, dark: { l: 0.79, c: 0.11, h: 18 } },
  property: { light: { l: 0.44, c: 0.07, h: 230 }, dark: { l: 0.83, c: 0.05, h: 230 } },
  'bracket-1': { light: { l: 0.5, c: 0.1, h: 80 }, dark: { l: 0.86, c: 0.13, h: 88 } },
  'bracket-2': { light: { l: 0.49, c: 0.18, h: 330 }, dark: { l: 0.78, c: 0.14, h: 330 } },
  'bracket-3': { light: { l: 0.48, c: 0.13, h: 245 }, dark: { l: 0.8, c: 0.1, h: 245 } },
};

/** `more`: Settings → Accessibility → Higher contrast. */
export type ContrastLevel = 'normal' | 'more';

/**
 * Every colour token of one style in one theme, as CSS custom properties (`--surface` …).
 * Status colours (ok, warn, danger, info) are the same in every style (STATUS), but are made here
 * too, so each style is checked with the status colours laid over its own surfaces.
 *
 * With `more` contrast, text, lines, the focus ring and accent text move further from the
 * backgrounds (styles.test.ts holds them to a stricter bar); the backgrounds stay, so every style
 * still looks like itself.
 */
export function styleTokens(
  style: StyleSpec,
  theme: ThemeName,
  level: ContrastLevel = 'normal',
): Record<string, string> {
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

  for (const [name, colours] of Object.entries(STATUS)) {
    const { base, soft, text } = colours[theme];
    // Higher contrast moves status text further from the backgrounds, like the other text.
    const dl = level === 'more' ? (theme === 'light' ? -0.1 : 0.07) : 0;
    tokens[`--${name}`] = ok(base);
    tokens[`--${name}-soft`] = ok(soft[0], soft[1]);
    tokens[`--${name}-text`] = ok({ ...text, l: Math.min(0.96, text.l + dl) });
  }

  if (level === 'more') {
    const clampL = (l: number) => Math.min(0.96, Math.max(0.2, l));
    const ink = (dl: number) => ok({ l: clampL(A.l + dl), c: A.c, h: A.h });
    Object.assign(
      tokens,
      theme === 'light'
        ? {
            '--text': fg(0.15, 0.006),
            '--text-muted': fg(0.3, 0.01),
            '--text-faint': fg(0.42, 0.01),
            '--line': bg(0.72, 0.006),
            '--line-strong': bg(0.52, 0.008),
            '--accent-ink': ink(-0.13),
            '--focus-ring': ink(-0.07),
          }
        : {
            '--text': fg(0.985, 0.002),
            '--text-muted': fg(0.88, 0.005),
            '--text-faint': fg(0.78, 0.005),
            '--line': bg(0.5, 0.005),
            '--line-strong': bg(0.64, 0.006),
            '--accent-ink': ink(0.14),
            '--focus-ring': ink(0.1),
          },
    );
  }

  // Higher contrast moves code colours further from the background, like status text.
  const codeDl = level === 'more' ? (theme === 'light' ? -0.1 : 0.07) : 0;
  for (const [name, colours] of Object.entries(CODE_COLOURS)) {
    const c = colours[theme];
    tokens[`--code-${name}`] = ok({ ...c, l: Math.min(0.96, c.l + codeDl) });
  }

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
