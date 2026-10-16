/// <reference types="node" />
// Node's types: this test reads a stylesheet from disk (Vitest empties CSS imports, even `?raw`).
/**
 * Guards how colour tokens are used, so text is only drawn in a colour that styles.test.ts checks
 * on the background it sits on:
 *
 * - `--accent-text` is text *on the accent*. Anywhere else (on a surface, or on a soft warning) it
 *   is near-white in a light theme and unreadable.
 * - status-coloured text uses `--ok-text`, `--warn-text`, `--danger-text` or `--info-text`, which
 *   are checked on every surface and on their soft backgrounds; `--ok` … are for icons, dots and
 *   borders (3:1).
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const components = import.meta.glob('/src/**/*.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

// Vitest empties CSS imports, so the global stylesheet is read from disk (tests run in shell/).
const sources: Record<string, string> = {
  ...components,
  '/src/styles/base.css': readFileSync('src/styles/base.css', 'utf8'),
};

interface Rule {
  file: string;
  line: number;
  selector: string;
  body: string;
}

/** Every innermost CSS rule (`selector { declarations }`), with the line it starts on. */
function rules(): Rule[] {
  const out: Rule[] = [];
  for (const [file, text] of Object.entries(sources)) {
    const css = file.endsWith('.svelte') ? styleOf(text) : { text, offset: 0 };
    for (const m of css.text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const line = text.slice(0, css.offset + m.index).split('\n').length;
      out.push({ file, line, selector: m[1]!.trim(), body: m[2]! });
    }
  }
  return out;
}

function styleOf(component: string): { text: string; offset: number } {
  const m = /<style[^>]*>([\s\S]*?)<\/style>/.exec(component);
  if (!m) return { text: '', offset: 0 };
  return { text: m[1]!, offset: m.index + m[0].indexOf('>') + 1 };
}

/** The value of the `color` property itself (not `border-color`, `accent-color` …). */
function textColour(body: string): string | undefined {
  return /(?:^|[;\s])color:\s*([^;]+);/.exec(body)?.[1]?.trim();
}

describe('colour tokens', () => {
  const all = rules();

  it('finds the rules it checks', () => {
    expect(all.length).toBeGreaterThan(200);
  });

  it('uses --accent-text only on the accent', () => {
    const bad = all.filter((r) => {
      const c = textColour(r.body);
      return (
        c?.includes('--accent-text') &&
        !/background(?:-color)?:\s*var\(--accent(?:-hover)?\)/.test(r.body)
      );
    });
    expect(bad.map((r) => `${r.file}:${r.line} ${r.selector}`)).toEqual([]);
  });

  it('writes status-coloured text with the --*-text tokens', () => {
    const bad = all.filter((r) =>
      /^var\(--(ok|warn|danger|info)\)$/.test(textColour(r.body) ?? ''),
    );
    expect(bad.map((r) => `${r.file}:${r.line} ${r.selector}: ${textColour(r.body)}`)).toEqual([]);
  });
});
