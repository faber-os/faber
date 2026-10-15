import { describe, expect, it } from 'vitest';
import {
  checkReport,
  compareTranslation,
  isLanguageCode,
  languageName,
  placeholdersOf,
  pluralFormsOf,
  translationTemplate,
  type Catalogue,
} from './check';

const EN = {
  a: 'Parts',
  b: 'Removed "{name}" from {place}.',
  c: { one: '{count} part', other: '{count} parts' },
  d: 'Close',
} satisfies Catalogue;

describe('compareTranslation', () => {
  it('counts what is translated', () => {
    const c = compareTranslation(EN, { _language: 'Dansk', a: 'Dele', d: 'Luk' }, 'da');
    expect(c).toMatchObject({ translated: 2, total: 4, percent: 50, missing: ['b', 'c'] });
    expect(c.extra).toEqual([]);
  });

  it('finds extra keys, empty texts, wrong shapes and placeholders', () => {
    const c = compareTranslation(
      EN,
      { a: '', b: 'Fjernede {navn}.', c: 'dele', d: 'Luk', old: 'Gammel' },
      'da',
    );
    expect(c.empty).toEqual(['a']);
    expect(c.wrongShape).toEqual(['c']);
    expect(c.extra).toEqual(['old']);
    expect(c.placeholders).toEqual([{ key: 'b', forgotten: ['name', 'place'], added: ['navn'] }]);
    expect(c.translated).toBe(1);
    expect(c.percent).toBe(25);
  });

  it('lists plural forms the language needs', () => {
    const c = compareTranslation(EN, { c: { other: '{count} dele' } }, 'da');
    expect(c.missingForms).toEqual([{ key: 'c', forms: ['one'] }]);
    expect(c.translated).toBe(1);
  });

  it('rounds down, so 100 % means everything', () => {
    const many: Record<string, string> = {};
    for (let i = 0; i < 300; i++) many[`k${i}`] = 'x';
    const tr: Record<string, string> = { ...many };
    delete tr['k0'];
    expect(compareTranslation(many, tr, 'da').percent).toBe(99);
  });
});

describe('helpers', () => {
  it('finds placeholders in texts and plurals', () => {
    expect(placeholdersOf('From {b} to {a}, {a}.')).toEqual(['a', 'b']);
    expect(placeholdersOf({ one: 'one', other: '{count} things' })).toEqual(['count']);
  });

  it('knows the plural forms of each language', () => {
    expect(pluralFormsOf('da')).toEqual(['one', 'other']);
    expect(pluralFormsOf('ja')).toEqual(['other']);
  });

  it('names a language from its file', () => {
    expect(languageName('da', { _language: ' Dansk ' })).toBe('Dansk');
    expect(languageName('da', { _language: 3 })).toBe('da');
  });

  it('accepts the same language codes as the core', () => {
    for (const ok of ['da', 'fil', 'pt-BR']) expect(isLanguageCode(ok)).toBe(true);
    for (const bad of ['', 'DA', 'da-br', '../da', 'da.json'])
      expect(isLanguageCode(bad)).toBe(false);
  });

  it('makes a template with every English text', () => {
    const t = JSON.parse(translationTemplate(EN)) as Record<string, unknown>;
    expect(t['_language']).toBe('');
    expect(t['c']).toEqual(EN.c);
  });
});

describe('checkReport', () => {
  const ok = (value: Record<string, unknown>) => ({ ok: true as const, value });

  it('says so when there are no translations', () => {
    const r = checkReport(EN, [], { strict: true });
    expect(r.lines).toEqual(['English: 4 texts.', 'No translations found. Faber shows English.']);
    expect(r.exitCode).toBe(0);
  });

  it('lists missing keys with their English text', () => {
    const r = checkReport(
      EN,
      [{ code: 'da', file: 'locales/da.json', content: ok({ _language: 'Dansk', a: 'Dele' }) }],
      { strict: false },
    );
    expect(r.lines).toContain('da (Dansk): locales/da.json');
    expect(r.lines).toContain('  25 % translated (1 of 4)');
    expect(r.lines).toContain('  Missing, shown in English (3):');
    expect(r.lines).toContain('    b  "Removed \\"{name}\\" from {place}."');
    expect(r.exitCode).toBe(0);
  });

  it('fails only with --strict', () => {
    const found = [{ code: 'da', file: 'da.json', content: ok({ a: 'Dele' }) }];
    expect(checkReport(EN, found, { strict: true }).exitCode).toBe(1);
    const full = {
      _language: 'Dansk',
      a: 'Dele',
      b: '{name} {place}',
      c: { one: '1', other: '{count}' },
      d: 'Luk',
    };
    const done = [{ code: 'da', file: 'da.json', content: ok(full) }];
    expect(checkReport(EN, done, { strict: true }).exitCode).toBe(0);
  });

  it('reports a file that cannot be read', () => {
    const broken = [
      { code: 'da', file: 'da.json', content: { ok: false as const, error: 'line 3: bad' } },
    ];
    const r = checkReport(EN, broken, { strict: true });
    expect(r.lines).toContain("  Can't be read: line 3: bad");
    expect(r.exitCode).toBe(1);
  });

  it('checks one language when asked', () => {
    const found = [
      { code: 'da', file: 'da.json', content: ok({}) },
      { code: 'sv', file: 'sv.json', content: ok({}) },
    ];
    const r = checkReport(EN, found, { strict: false, only: 'sv' });
    expect(r.lines.some((l) => l.startsWith('da'))).toBe(false);
    expect(r.lines.some((l) => l.startsWith('sv'))).toBe(true);
  });
});
