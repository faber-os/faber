/**
 * Comparing a translation with the English catalogue (docs/translations.md). One set of rules for
 * three users: `t()` uses `usableMessage` to decide, key by key, whether to show the translation
 * or English; Settings shows `compareTranslation`'s percentage; and `npm run i18n:check` prints
 * its report.
 *
 * This file has no imports at run time, so the Node checker (scripts/i18n-check.ts) can load it
 * directly, without a build.
 */

/** The plural forms of Intl.PluralRules (CLDR). Every plural message has `other`. */
export const PLURAL_FORMS = ['zero', 'one', 'two', 'few', 'many', 'other'] as const;
export type PluralForm = (typeof PLURAL_FORMS)[number];
export type PluralMessage = { readonly [F in PluralForm]?: string } & { readonly other: string };
/** One text in the catalogue: a string, or a plural (`{ one: '{count} part', other: … }`). */
export type Message = string | PluralMessage;
export type Catalogue = Readonly<Record<string, Message>>;
/** A translation file as read from JSON: anything in it may be wrong. */
export type TranslationFile = Readonly<Record<string, unknown>>;

/** Keys starting with `_` are about the file (`_language`), not texts. */
export const LANGUAGE_NAME_KEY = '_language';

export function isPlural(m: unknown): m is PluralMessage {
  return typeof m === 'object' && m !== null && !Array.isArray(m) && 'other' in m;
}

/** The `{name}` placeholders in a message (all forms of a plural), sorted. */
export function placeholdersOf(m: Message): string[] {
  const texts = typeof m === 'string' ? [m] : Object.values(m).filter((v) => v !== undefined);
  const names = new Set<string>();
  for (const text of texts) for (const match of text.matchAll(/\{(\w+)\}/g)) names.add(match[1]!);
  return [...names].sort();
}

/** The translation's message for a key, or null when it can't be used and English shows. */
export function usableMessage(english: Message, value: unknown): Message | null {
  return problemsWith(english, value).length === 0 ? (value as Message) : null;
}

type Problem = 'missing' | 'shape' | 'empty' | 'added-placeholder';

function problemsWith(english: Message, value: unknown): Problem[] {
  if (value === undefined) return ['missing'];
  if (isPlural(english)) {
    if (!isPlural(value)) return ['shape'];
    const forms = Object.entries(value);
    if (forms.some(([form, text]) => !isForm(form) || typeof text !== 'string')) return ['shape'];
    if (forms.some(([, text]) => (text as string).trim() === '')) return ['empty'];
  } else {
    if (typeof value !== 'string') return ['shape'];
    if (value.trim() === '') return ['empty'];
  }
  // A placeholder English doesn't fill would show as "{name}": use English instead.
  const known = new Set(placeholdersOf(english));
  return placeholdersOf(value as Message).some((p) => !known.has(p)) ? ['added-placeholder'] : [];
}

function isForm(form: string): form is PluralForm {
  return (PLURAL_FORMS as readonly string[]).includes(form);
}

export interface PlaceholderMismatch {
  key: string;
  /** In English but not in the translation. Allowed (some languages don't need it), but shown. */
  forgotten: string[];
  /** In the translation but not in English. That key shows in English until fixed. */
  added: string[];
}

export interface Comparison {
  /** In English, not in the translation. */
  missing: string[];
  /** In the translation, not in English (an old key, or a typo). Ignored at run time. */
  extra: string[];
  placeholders: PlaceholderMismatch[];
  empty: string[];
  /** A plural where English has a plain text, or the other way round, or not text at all. */
  wrongShape: string[];
  /** Plural forms the language uses (Intl.PluralRules) but a plural message doesn't have. */
  missingForms: { key: string; forms: PluralForm[] }[];
  /** Keys whose translation is shown. */
  translated: number;
  total: number;
  /** translated ÷ total, rounded down, so 100 means every text is translated. */
  percent: number;
}

export function compareTranslation(
  english: Catalogue,
  translation: TranslationFile,
  language: string,
): Comparison {
  const result: Comparison = {
    missing: [],
    extra: [],
    placeholders: [],
    empty: [],
    wrongShape: [],
    missingForms: [],
    translated: 0,
    total: 0,
    percent: 0,
  };
  const forms = pluralFormsOf(language);
  for (const [key, en] of Object.entries(english)) {
    result.total++;
    const value = translation[key];
    const problems = problemsWith(en, value);
    if (problems.length === 0) result.translated++;
    if (problems.includes('missing')) result.missing.push(key);
    if (problems.includes('shape')) result.wrongShape.push(key);
    if (problems.includes('empty')) result.empty.push(key);
    if (problems.includes('missing') || problems.includes('shape')) continue;
    const want = placeholdersOf(en);
    const have = placeholdersOf(value as Message);
    const forgotten = want.filter((p) => !have.includes(p));
    const added = have.filter((p) => !want.includes(p));
    if (forgotten.length || added.length) result.placeholders.push({ key, forgotten, added });
    if (isPlural(value)) {
      const lacking = forms.filter((f) => !(f in value));
      if (lacking.length) result.missingForms.push({ key, forms: lacking });
    }
  }
  for (const key of Object.keys(translation)) {
    if (!key.startsWith('_') && !(key in english)) result.extra.push(key);
  }
  result.percent = result.total ? Math.floor((100 * result.translated) / result.total) : 100;
  return result;
}

/** The plural forms a language uses, e.g. `one, other` for Danish; `other` if it's unknown. */
export function pluralFormsOf(language: string): PluralForm[] {
  try {
    const used = new Intl.PluralRules(language).resolvedOptions().pluralCategories;
    return PLURAL_FORMS.filter((f) => used.includes(f));
  } catch {
    return ['other'];
  }
}

/** The language's own name from its file, or its code. */
export function languageName(code: string, translation: TranslationFile): string {
  const name = translation[LANGUAGE_NAME_KEY];
  return typeof name === 'string' && name.trim() !== '' ? name.trim() : code;
}

/** Same rule as the core (core/src/names.rs): `da`, `fil`, `pt-BR`. */
export function isLanguageCode(code: string): boolean {
  return /^[a-z]{2,3}(-[A-Z]{2})?$/.test(code);
}

// ---- the checker's report ----------------------------------------------------------------

/** One translation file the checker found. */
export interface FoundTranslation {
  code: string;
  /** Where it came from, as the user should read it. */
  file: string;
  /** The parsed file, or why it couldn't be read. */
  content: { ok: true; value: TranslationFile } | { ok: false; error: string };
}

export interface CheckOptions {
  strict: boolean;
  /** Only this language, e.g. `da`. */
  only?: string;
}

/** What `npm run i18n:check` prints, and its exit code. */
export function checkReport(
  english: Catalogue,
  found: readonly FoundTranslation[],
  options: CheckOptions,
): { lines: string[]; exitCode: number } {
  const total = Object.keys(english).length;
  const lines = [`English: ${total} texts.`];
  const checked = found.filter((f) => !options.only || f.code === options.only);
  if (checked.length === 0) {
    lines.push(
      options.only
        ? `No translation for "${options.only}" was found.`
        : 'No translations found. Faber shows English.',
    );
    return { lines, exitCode: options.strict && options.only ? 1 : 0 };
  }
  let clean = true;
  for (const f of checked) {
    lines.push('');
    if (!f.content.ok) {
      clean = false;
      lines.push(`${f.code}: ${f.file}`, `  Can't be read: ${f.content.error}`);
      continue;
    }
    const c = compareTranslation(english, f.content.value, f.code);
    const name = languageName(f.code, f.content.value);
    lines.push(
      `${f.code} (${name}): ${f.file}`,
      `  ${c.percent} % translated (${c.translated} of ${c.total})`,
    );
    if (name === f.code) lines.push(`  No "${LANGUAGE_NAME_KEY}": add the language's own name.`);
    const section = (title: string, items: string[]) => {
      if (items.length === 0) return;
      clean = false;
      lines.push(`  ${title} (${items.length}):`, ...items.map((i) => `    ${i}`));
    };
    section(
      'Missing, shown in English',
      c.missing.map((k) => `${k}  ${JSON.stringify(english[k])}`),
    );
    section(
      'Placeholders that differ from English',
      c.placeholders.map((p) => {
        const parts = [];
        if (p.added.length) parts.push(`unknown ${braces(p.added)}, shown in English`);
        if (p.forgotten.length) parts.push(`leaves out ${braces(p.forgotten)}`);
        return `${p.key}: ${parts.join('; ')}`;
      }),
    );
    section('Empty, shown in English', c.empty);
    section(
      'Not the same kind of text as English (plural or not), shown in English',
      c.wrongShape.map((k) => `${k}  English: ${JSON.stringify(english[k])}`),
    );
    section(
      'Plural forms missing (the "other" form is used)',
      c.missingForms.map((m) => `${m.key}: ${m.forms.join(', ')}`),
    );
    section('Extra, not in English (ignored)', c.extra);
  }
  return { lines, exitCode: options.strict && !clean ? 1 : 0 };
}

function braces(names: string[]): string {
  return names.map((n) => `{${n}}`).join(' ');
}

/** A starting point for a new translation: every English text, ready to be replaced. */
export function translationTemplate(english: Catalogue, languageName = ''): string {
  return JSON.stringify({ [LANGUAGE_NAME_KEY]: languageName, ...english }, null, 2) + '\n';
}
