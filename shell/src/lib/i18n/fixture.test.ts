/**
 * The test-only translation in src/test/fixtures/xx.json ("Testish") against the real catalogue:
 * some texts translated, some missing, and some broken on purpose.
 */
import { afterEach, describe, expect, it } from 'vitest';
import xx from '../../test/fixtures/xx.json';
import { locale, t } from './locale.svelte';
import { checkReport, compareTranslation } from './check';
import { en } from './en';

afterEach(() => locale.use('en', null));

describe('a partial translation', () => {
  it('shows what it translates, with its own placeholders and plurals', () => {
    locale.use('xx', xx);
    expect(t('settings.appearance.title')).toBe('Lookz');
    expect(t('settings.time.zone.now', { time: '14:30', date: '1 May' })).toBe('At 14:30 on 1 May');
    expect(t('parts.summary.count', { count: 3 })).toBe('3 partz');
  });

  it('falls back to English for missing, empty, broken and wrongly shaped texts', () => {
    locale.use('xx', xx);
    expect(t('settings.units.intro')).toBe(
      'How Faber shows measurements, e.g. the rings in the laser-scan viewer.',
    );
    expect(t('settings.loading')).toBe('Loading settings…');
    expect(t('settings.storage.full.message', { percent: 91 })).toBe('The disk is 91 % full.');
    expect(t('settings.units.title')).toBe('Units');
  });

  it('may leave a placeholder out', () => {
    locale.use('xx', xx);
    expect(t('settings.storage.backup.saved', { name: 'b.zip', size: '2 MB' })).toBe('Kept b.zip');
  });

  it('switches back to English', () => {
    locale.use('xx', xx);
    locale.use('en', null);
    expect(t('settings.appearance.title')).toBe('Appearance');
    expect(locale.language).toBe('en');
  });

  it('is reported by the checker', () => {
    const c = compareTranslation(en, xx, 'xx');
    expect(c.translated).toBe(4);
    expect(c.empty).toEqual(['settings.loading']);
    expect(c.wrongShape).toEqual(['settings.units.title']);
    expect(c.extra).toEqual(['settings.no.such.key']);
    expect(c.placeholders).toEqual([
      { key: 'settings.storage.full.message', forgotten: ['percent'], added: ['procent'] },
      { key: 'settings.storage.backup.saved', forgotten: ['size'], added: [] },
    ]);
    const r = checkReport(en, [{ code: 'xx', file: 'xx.json', content: { ok: true, value: xx } }], {
      strict: true,
    });
    expect(r.lines).toContain('xx (Testish): xx.json');
    expect(r.lines).toContain(
      '    settings.units.intro  "How Faber shows measurements, e.g. the rings in the laser-scan viewer."',
    );
    expect(r.exitCode).toBe(1);
  });
});
