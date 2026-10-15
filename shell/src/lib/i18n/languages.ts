import { en } from './en';
import { compareTranslation, isLanguageCode, languageName, type TranslationFile } from './check';

/** A translation installed on the machine (`GET /api/v0/translations`), with its file once read. */
export interface MachineTranslation {
  code: string;
  name: string;
  /** Why the file can't be used (broken JSON …), from the core. */
  problem?: string;
  translation?: TranslationFile;
}

/** One entry of Settings → Appearance → Language. */
export interface LanguageChoice {
  code: string;
  name: string;
  /** How much is translated, 0–100; null for English, which is the catalogue itself. */
  percent: number | null;
  translation: TranslationFile | null;
}

/**
 * Translations built into the shell: `src/lib/i18n/locales/<code>.json`. Adding a file there and
 * rebuilding is all it takes (docs/translations.md).
 */
export const BUILT_IN: ReadonlyMap<string, TranslationFile> = builtInTranslations(
  import.meta.glob('./locales/*.json', { eager: true, import: 'default' }),
);

/** `{ './locales/da.json': {...} }` → `da` → its file. Badly named files are left out. */
export function builtInTranslations(
  modules: Record<string, unknown>,
): Map<string, TranslationFile> {
  const found = new Map<string, TranslationFile>();
  for (const [path, file] of Object.entries(modules)) {
    const code = /([^/]+)\.json$/.exec(path)?.[1];
    if (!code || code === 'en' || !isLanguageCode(code)) continue;
    if (typeof file === 'object' && file !== null && !Array.isArray(file))
      found.set(code, file as TranslationFile);
  }
  return found;
}

/**
 * Every language the shell can show: English first, then the translations by name. One on the
 * machine replaces a built-in one with the same code, so a fix can be tried without a rebuild.
 * Files that can't be read aren't choices (Settings lists them as problems).
 */
export function languageChoices(
  builtIn: ReadonlyMap<string, TranslationFile>,
  machine: readonly MachineTranslation[],
  englishName: string,
): LanguageChoice[] {
  const files = new Map(builtIn);
  for (const m of machine) {
    if (m.code !== 'en' && !m.problem && m.translation) files.set(m.code, m.translation);
  }
  const translated = [...files].map(([code, translation]) => ({
    code,
    name: languageName(code, translation),
    percent: compareTranslation(en, translation, code).percent,
    translation,
  }));
  translated.sort((a, b) => a.name.localeCompare(b.name));
  return [{ code: 'en', name: englishName, percent: null, translation: null }, ...translated];
}
