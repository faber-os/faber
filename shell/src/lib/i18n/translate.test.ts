import { describe, expect, it } from 'vitest';
import { translate } from './translate';
import type { Catalogue } from './check';

const EN = {
  'parts.title': 'Parts',
  'parts.count': { one: '{count} part', other: '{count} parts' },
  'parts.removed': 'Removed "{name}".',
} satisfies Catalogue;

describe('translate', () => {
  it('shows English without a translation, with placeholders filled', () => {
    expect(translate(EN, 'en', null, 'parts.title')).toBe('Parts');
    expect(translate(EN, 'en', null, 'parts.removed', { name: 'lidar' })).toBe('Removed "lidar".');
  });

  it('picks the plural form with Intl.PluralRules', () => {
    expect(translate(EN, 'en', null, 'parts.count', { count: 1 })).toBe('1 part');
    expect(translate(EN, 'en', null, 'parts.count', { count: 0 })).toBe('0 parts');
    expect(translate(EN, 'en', null, 'parts.count', { count: 2 })).toBe('2 parts');
  });

  it('uses the translation, and its own plural rules', () => {
    const da = { 'parts.count': { one: 'én del', other: '{count} dele' } };
    expect(translate(EN, 'da', da, 'parts.count', { count: 1 })).toBe('én del');
    expect(translate(EN, 'da', da, 'parts.count', { count: 3 })).toBe('3 dele');
    // Polish has "few" and "many"; a form the file leaves out falls back to its "other".
    const pl = {
      'parts.count': { one: '{count} część', few: '{count} części', other: '{count} cz.' },
    };
    expect(translate(EN, 'pl', pl, 'parts.count', { count: 3 })).toBe('3 części');
    expect(translate(EN, 'pl', pl, 'parts.count', { count: 5 })).toBe('5 cz.');
  });

  it('falls back to English key by key', () => {
    const da = { 'parts.title': 'Dele' };
    expect(translate(EN, 'da', da, 'parts.title')).toBe('Dele');
    expect(translate(EN, 'da', da, 'parts.removed', { name: 'x' })).toBe('Removed "x".');
    // English plurals keep English rules even while Danish is chosen.
    expect(translate(EN, 'da', da, 'parts.count', { count: 1 })).toBe('1 part');
  });

  it('never shows a broken translation', () => {
    const broken = {
      'parts.title': '',
      'parts.count': 'dele',
      'parts.removed': 'Fjernede {navn}.',
    };
    expect(translate(EN, 'da', broken, 'parts.title')).toBe('Parts');
    expect(translate(EN, 'da', broken, 'parts.count', { count: 2 })).toBe('2 parts');
    expect(translate(EN, 'da', broken, 'parts.removed', { name: 'x' })).toBe('Removed "x".');
  });

  it('allows a translation to leave a placeholder out', () => {
    const da = { 'parts.removed': 'Fjernet.' };
    expect(translate(EN, 'da', da, 'parts.removed', { name: 'x' })).toBe('Fjernet.');
  });

  it('shows an unknown key as itself instead of crashing', () => {
    expect(translate(EN, 'en', null, 'no.such.key')).toBe('no.such.key');
  });

  it('copes with an unknown language code', () => {
    const xx = { 'parts.count': { other: '{count} xx' } };
    expect(translate(EN, 'qq', xx, 'parts.count', { count: 1 })).toBe('1 xx');
  });
});
