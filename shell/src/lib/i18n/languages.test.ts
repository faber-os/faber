import { describe, expect, it } from 'vitest';
import { BUILT_IN, builtInTranslations, languageChoices } from './languages';
import { en } from './en';
import xx from '../../test/fixtures/xx.json';

describe('built-in translations', () => {
  it('ships English only', () => {
    expect([...BUILT_IN.keys()]).toEqual([]);
  });

  it('finds a <code>.json next to the catalogue', () => {
    const found = builtInTranslations({
      './locales/da.json': { _language: 'Dansk' },
      './locales/pt-BR.json': {},
      './locales/en.json': { a: 'b' },
      './locales/Notes.json': {},
      './locales/sv.json': ['not', 'an', 'object'],
    });
    expect([...found.keys()]).toEqual(['da', 'pt-BR']);
  });
});

describe('languageChoices', () => {
  it('is English alone without translations, so no picker shows', () => {
    expect(languageChoices(new Map(), [], 'English')).toEqual([
      { code: 'en', name: 'English', percent: null, translation: null },
    ]);
  });

  it('lists translations by name, with how complete they are', () => {
    const builtIn = new Map([['xx', xx]]);
    const choices = languageChoices(builtIn, [], 'English');
    expect(choices.map((c) => c.code)).toEqual(['en', 'xx']);
    expect(choices[1]!.name).toBe('Testish');
    const total = Object.keys(en).length;
    expect(choices[1]!.percent).toBe(Math.floor((100 * 4) / total));
  });

  it('lets a file on the machine replace a built-in one, and skips broken files', () => {
    const builtIn = new Map([['da', { _language: 'Dansk (built in)' }]]);
    const choices = languageChoices(
      builtIn,
      [
        { code: 'da', name: 'Dansk', translation: { _language: 'Dansk' } },
        { code: 'sv', name: 'sv', problem: 'sv.json has a problem on line 2: …' },
        { code: 'en', name: 'English?', translation: {} },
      ],
      'English',
    );
    expect(choices.map((c) => c.name)).toEqual(['English', 'Dansk']);
  });
});
