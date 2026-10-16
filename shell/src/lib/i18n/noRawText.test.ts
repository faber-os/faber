/**
 * Guards the catalogue (D44): every text the shell shows comes from `t()`. This test reads every
 * Svelte component and TypeScript module in src/ and fails on visible text written straight into
 * the code, with the file and line, so new text can't slip past the catalogue and translations
 * quietly fall behind.
 *
 * What counts as visible text:
 * - text between tags in a component, and static `title`, `aria-label`, `placeholder`, `alt` and
 *   text-like props (`label`, `description`, `hint`, `message` …);
 * - in scripts, a string under one of those property names, or any string that reads like a
 *   sentence (two or more words starting with a capital, or ending in . ! ? … or :).
 *
 * Not checked: `t('key')` calls, programmer errors (`new Error(...)`), console output, imports.
 * If a hit really isn't text for people (a symbol, a unit, a brand), add it to ALLOWED below
 * with a reason.
 */
import { describe, expect, it } from 'vitest';
import { parse } from 'svelte/compiler';

/** Text that is the same in every language. Each entry says why. */
const ALLOWED = new Set([
  'Faber', // the product name (CLAUDE.md: written only as Faber, faber or FABER)
  'faber', // the default machine name, shown as an example of a name
  'cm', // a unit (D43: units are data, converted by viewers/units.ts)
  'm',
  'ft',
  'in',
  'mm',
  'Hz',
  'ms',
  'µs',
  'OK', // the same in the languages Faber is likely to get, and used as a status word
  'Aa', // the type sample on a style swatch (aria-hidden): shows the font, not a word
  'No diagnostics', // a CodeMirror phrase's name, which codemirror.ts translates with t()
]);

/** Props and attributes whose value people read. */
const VISIBLE_PROPS = new Set([
  'title',
  'aria-label',
  'aria-description',
  'aria-roledescription',
  'aria-valuetext',
  'placeholder',
  'alt',
  'label',
  'ariaLabel',
  'description',
  'hint',
  'message',
  'text',
  'heading',
  'tooltip',
  'confirmLabel',
  'cancelLabel',
  'summary',
  'caption',
  'detail',
  'emptyText',
  'subtitle',
]);

const SOURCES = {
  ...import.meta.glob('/src/**/*.svelte', { query: '?raw', import: 'default', eager: true }),
  ...import.meta.glob(['/src/**/*.ts', '!/src/**/*.test.ts', '!/src/**/*.d.ts'], {
    query: '?raw',
    import: 'default',
    eager: true,
  }),
} as Record<string, string>;

/**
 * Files that never hold shell text: tests and their fakes, generated types, the catalogue. And
 * lib/state/diagnostics.ts: "Copy diagnostics" is a report for a bug tracker, kept in English on
 * purpose so maintainers can read every report (docs/translations.md). And the Arduino editor's
 * API catalogue (components/editor/arduinoApi.ts, faberlinkApi.ts): documentation of code, in
 * English like the code it describes (docs/arduino-editor.md).
 */
const SKIPPED =
  /^\/src\/(test\/|lib\/api\/generated\/|lib\/i18n\/|lib\/state\/diagnostics\.ts$|components\/editor\/(arduinoApi|faberlinkApi)\.ts$)/;

/** A catalogue key such as `settings.nav.units`, passed around to be translated later. */
const KEY = /^[a-z][\w-]*(\.[\w-]+)+$/;

const LETTER = /\p{L}/u;

function sentenceLike(s: string): boolean {
  const text = s.trim();
  const words = text.match(/\p{L}[\p{L}'’-]*/gu) ?? [];
  if (words.length < 2) return false;
  if (/^[^\p{L}]*\p{Lu}\p{Ll}/u.test(text) && /\s/.test(text)) return true;
  return /[.!?…:]$/.test(text) && /\s/.test(text);
}

function visible(s: string): boolean {
  const text = s.trim();
  return LETTER.test(text) && !ALLOWED.has(text) && !KEY.test(text);
}

interface Hit {
  file: string;
  line: number;
  text: string;
}

type Node = { type?: string; start?: number; [key: string]: unknown };

function findRawText(file: string, source: string): Hit[] {
  const isSvelte = file.endsWith('.svelte');
  const prefix = isSvelte ? '' : '<script lang="ts">\n';
  const code = isSvelte ? source : `${prefix}${source}\n</script>`;
  const ast = parse(code, { modern: true }) as unknown as Node;
  const hits: Hit[] = [];
  const lineOf = (offset: number) => code.slice(0, offset).split('\n').length - (isSvelte ? 0 : 1);
  const hit = (node: Node, text: string) =>
    hits.push({ file, line: lineOf(node.start ?? 0), text: text.trim() });

  const walk = (node: unknown, parent: Node | null, parentKey: string): void => {
    if (Array.isArray(node)) {
      for (const n of node) walk(n, parent, parentKey);
      return;
    }
    if (typeof node !== 'object' || node === null) return;
    const n = node as Node;
    switch (n.type) {
      case 'Text':
        // Text between tags. Attribute values are judged with their attribute below.
        if (parentKey === 'nodes' && visible(String(n.data))) hit(n, String(n.data));
        return;
      case 'Attribute': {
        const name = String(n.name);
        if (Array.isArray(n.value)) {
          for (const part of n.value as Node[]) {
            if (part.type === 'Text' && VISIBLE_PROPS.has(name) && visible(String(part.data)))
              hit(part, String(part.data));
            else walk(part, n, 'value');
          }
        } else walk(n.value, n, 'value');
        return;
      }
      case 'StyleSheet':
      case 'ImportDeclaration':
      case 'ExportAllDeclaration':
      case 'TSLiteralType':
      case 'TSTypeAnnotation':
      case 'TSTypeAliasDeclaration':
      case 'TSInterfaceDeclaration':
        return;
      case 'CallExpression':
      case 'NewExpression':
        if (ignoredCall(n)) return;
        break;
      case 'Literal':
        if (typeof n.value === 'string') checkString(n, String(n.value), parent);
        return;
      case 'TemplateLiteral': {
        const quasis = (n.quasis as { value: { cooked: string } }[]).map((q) => q.value.cooked);
        checkString(n, quasis.join('{}'), parent);
        break;
      }
    }
    for (const [key, value] of Object.entries(n)) {
      if (key === 'type' || key === 'start' || key === 'end' || key === 'loc') continue;
      if (typeof value === 'object') walk(value, n, key);
    }
  };

  const checkString = (n: Node, text: string, parent: Node | null) => {
    if (!visible(text)) return;
    const prop = parent?.type === 'Property' && parent['value'] === n ? propName(parent) : null;
    if ((prop !== null && VISIBLE_PROPS.has(prop)) || sentenceLike(text)) hit(n, text);
  };

  walk(ast, null, '');
  return hits;
}

function propName(p: Node): string | null {
  const key = p['key'] as Node;
  if (key.type === 'Identifier') return String(key['name']);
  if (key.type === 'Literal') return String(key['value']);
  return null;
}

/** Calls whose strings are not for people: t() keys, programmer errors, logging. */
function ignoredCall(n: Node): boolean {
  const callee = n['callee'] as Node;
  if (callee.type === 'Identifier')
    return ['t', 'Error', 'TypeError', 'RangeError'].includes(String(callee['name']));
  if (callee.type === 'MemberExpression') {
    const object = callee['object'] as Node;
    return object.type === 'Identifier' && object['name'] === 'console';
  }
  return false;
}

describe('every visible text comes from the catalogue', () => {
  const files = Object.keys(SOURCES).filter((f) => !SKIPPED.test(f));

  it('finds the source files', () => {
    expect(files.length).toBeGreaterThan(50);
  });

  it('has no raw text in components or modules', () => {
    const hits = files.flatMap((f) => findRawText(f, SOURCES[f]!));
    const report = hits.map((h) => `${h.file.slice(1)}:${h.line}: ${JSON.stringify(h.text)}`);
    // One line per text, so the failure lists every one: move each into en.ts and use t().
    expect(report.join('\n'), 'Move these texts into src/lib/i18n/en.ts and use t()').toBe('');
  });

  it('catches raw text the way it is meant to', () => {
    const svelte = `<script lang="ts">
      const items = [{ label: 'Move to Trash' }];
      const ok = t('parts.title');
      const id = 'tool:parts';
      throw new Error('Unknown window kind');
    </script>
    <p title="Close this">Hello there {name}</p><span>×</span><span>cm</span>
    <Button label="Save" class="primary wide" />`;
    expect(
      findRawText('/x.svelte', svelte)
        .map((h) => h.text)
        .sort(),
    ).toEqual(['Close this', 'Hello there', 'Move to Trash', 'Save']);
    const ts = "export const POWER = 'Available on the machine image';\nconst k = 'Escape';";
    expect(findRawText('/x.ts', ts)).toEqual([
      { file: '/x.ts', line: 1, text: 'Available on the machine image' },
    ]);
  });
});
