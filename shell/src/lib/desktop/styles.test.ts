import { describe, expect, it } from 'vitest';
import { contrast } from './contrast';
import {
  APP_COLOURS,
  DEFAULT_STYLE,
  PLATE,
  STYLES,
  resolveStyle,
  styleSwatch,
  styleTokens,
  type ThemeName,
} from './styles';

const THEMES: ThemeName[] = ['light', 'dark'];

describe('contrast', () => {
  it('is 21 for black on white and 1 for a colour on itself', () => {
    expect(contrast('oklch(0 0 0)', 'oklch(1 0 0)')).toBeCloseTo(21, 0);
    expect(contrast('oklch(0.5 0.1 200)', 'oklch(0.5 0.1 200)')).toBeCloseTo(1, 5);
  });

  it('lays a translucent colour over what is behind it', () => {
    // Fully transparent black on white is just white.
    expect(contrast('oklch(0 0 0 / 0)', 'oklch(1 0 0)')).toBeCloseTo(1, 5);
    expect(contrast('oklch(0 0 0)', ['oklch(1 0 0)', 'oklch(0 0 0 / 0.5)'])).toBeLessThan(6);
  });
});

describe('styles', () => {
  it('has the six built-in styles, Aluminium first and default', () => {
    expect(STYLES.map((s) => s.id)).toEqual([
      'aluminium',
      'cobalt',
      'workshop',
      'blueprint',
      'graphite',
      'signal',
    ]);
    expect(DEFAULT_STYLE).toBe('aluminium');
  });

  it('falls back to Aluminium for an unknown style', () => {
    expect(resolveStyle('nope').id).toBe('aluminium');
    expect(resolveStyle(undefined).id).toBe('aluminium');
    expect(resolveStyle('cobalt').id).toBe('cobalt');
  });

  it('gives every style the same complete set of tokens in both themes', () => {
    const keys = Object.keys(styleTokens(STYLES[0]!, 'light')).sort();
    expect(keys.length).toBeGreaterThan(30);
    for (const s of STYLES) {
      for (const t of THEMES) {
        const tokens = styleTokens(s, t);
        expect(Object.keys(tokens).sort(), `${s.id} ${t}`).toEqual(keys);
        for (const v of Object.values(tokens)) expect(v).toMatch(/^oklch\(/);
      }
    }
  });

  it('keeps Aluminium as tokens.css has it', () => {
    const light = styleTokens(resolveStyle('aluminium'), 'light');
    expect(light['--accent']).toBe('oklch(0.487 0.083 186)');
    expect(light['--surface']).toBe('oklch(0.993 0.001 250)');
    const dark = styleTokens(resolveStyle('aluminium'), 'dark');
    expect(dark['--accent']).toBe('oklch(0.74 0.095 184)');
    expect(dark['--text']).toBe('oklch(0.94 0.003 250)');
  });

  describe.each(STYLES.flatMap((s) => THEMES.map((t) => [s.id, t] as const)))(
    '%s, %s, is readable',
    (id, theme) => {
      const k = styleTokens(resolveStyle(id), theme);
      const t = (name: string) => k[name]!;

      it('text on every background (at least 7:1)', () => {
        for (const bg of ['--surface', '--surface-2', '--surface-3', '--chrome', '--menu']) {
          expect(contrast(t('--text'), t(bg)), bg).toBeGreaterThanOrEqual(7);
        }
        expect(contrast(t('--text'), t('--taskbar'))).toBeGreaterThanOrEqual(7);
      });

      it('muted text on every background (at least 4.5:1)', () => {
        for (const bg of ['--surface', '--surface-2', '--surface-3', '--chrome', '--taskbar']) {
          expect(contrast(t('--text-muted'), t(bg)), bg).toBeGreaterThanOrEqual(4.5);
        }
        expect(contrast(t('--text-muted'), t('--chrome-tabs'))).toBeGreaterThanOrEqual(4.5);
      });

      it('text on a selected row and a hovered menu item (at least 4.5:1)', () => {
        expect(contrast(t('--text'), [t('--surface'), t('--selected')])).toBeGreaterThanOrEqual(
          4.5,
        );
        expect(contrast(t('--text'), [t('--menu'), t('--accent-soft')])).toBeGreaterThanOrEqual(
          4.5,
        );
      });

      it('text on the accent, and the accent as text (at least 4.5:1)', () => {
        expect(contrast(t('--accent-text'), t('--accent'))).toBeGreaterThanOrEqual(4.5);
        expect(contrast(t('--accent-text'), t('--accent-hover'))).toBeGreaterThanOrEqual(4.5);
        expect(contrast(t('--accent-ink'), t('--surface'))).toBeGreaterThanOrEqual(4.5);
        expect(
          contrast(t('--accent-ink'), [t('--surface'), t('--selected')]),
        ).toBeGreaterThanOrEqual(4.5);
      });

      it('the accent and the focus ring stand out from the surface (at least 3:1)', () => {
        expect(contrast(t('--accent'), t('--surface'))).toBeGreaterThanOrEqual(3);
        expect(contrast(t('--focus-ring'), t('--surface'))).toBeGreaterThanOrEqual(3);
      });
    },
  );

  it('draws every program icon at 3:1 or more on its plate', () => {
    for (const theme of THEMES) {
      for (const [name, c] of Object.entries(APP_COLOURS)) {
        const colour = `oklch(${c[theme].l} ${c[theme].c} ${c[theme].h})`;
        expect(contrast(colour, PLATE[theme]), `${name} ${theme}`).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('shows a light and a dark swatch for each style card', () => {
    const s = styleSwatch(resolveStyle('cobalt'));
    expect(s.accentLight).not.toBe(s.accentDark);
    expect(contrast(s.light, s.dark)).toBeGreaterThan(4);
  });
});

describe('higher contrast', () => {
  const PAIRS: [string, string][] = [
    ['--text', '--surface'],
    ['--text', '--chrome'],
    ['--text-muted', '--surface'],
    ['--text-muted', '--surface-2'],
    ['--text-muted', '--chrome-tabs'],
    ['--text-faint', '--surface'],
    ['--accent-ink', '--surface'],
    ['--focus-ring', '--surface'],
    ['--line-strong', '--surface'],
  ];

  describe.each(STYLES.flatMap((s) => THEMES.map((t) => [s.id, t] as const)))(
    '%s, %s, with higher contrast',
    (id, theme) => {
      const normal = styleTokens(resolveStyle(id), theme);
      const boosted = styleTokens(resolveStyle(id), theme, 'more');
      const b = (name: string) => boosted[name]!;

      it('has the same tokens as the normal version', () => {
        expect(Object.keys(boosted).sort()).toEqual(Object.keys(normal).sort());
      });

      it('is never weaker than the normal version', () => {
        for (const [fg, bg] of PAIRS) {
          expect(contrast(b(fg), b(bg)), `${fg} on ${bg}`).toBeGreaterThanOrEqual(
            contrast(normal[fg]!, normal[bg]!) - 1e-9,
          );
        }
      });

      it('meets the stricter bar: muted text 7:1, faint text and focus 4.5:1, lines 3:1', () => {
        for (const bg of ['--surface', '--surface-2', '--surface-3', '--chrome', '--taskbar']) {
          expect(contrast(b('--text'), b(bg)), bg).toBeGreaterThanOrEqual(12);
          expect(contrast(b('--text-muted'), b(bg)), bg).toBeGreaterThanOrEqual(7);
          expect(contrast(b('--text-faint'), b(bg)), bg).toBeGreaterThanOrEqual(4.5);
        }
        expect(contrast(b('--focus-ring'), b('--surface'))).toBeGreaterThanOrEqual(4.5);
        expect(contrast(b('--accent-ink'), b('--surface'))).toBeGreaterThanOrEqual(7);
        expect(contrast(b('--line-strong'), b('--surface'))).toBeGreaterThanOrEqual(3);
        expect(contrast(b('--accent-text'), b('--accent'))).toBeGreaterThanOrEqual(4.5);
      });
    },
  );
});
