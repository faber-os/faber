import {
  isPlural,
  usableMessage,
  type Catalogue,
  type Message,
  type TranslationFile,
} from './check';

export type Params = Readonly<Record<string, string | number>>;

/**
 * The text for `key`: the translation's when it can be used (check.ts, `usableMessage`), else
 * English. Never throws: an unknown key shows as the key itself, so a mistake is visible.
 */
export function translate(
  english: Catalogue,
  language: string,
  translation: TranslationFile | null,
  key: string,
  params: Params = {},
): string {
  const en = english[key];
  if (en === undefined) return key;
  const own = translation ? usableMessage(en, translation[key]) : null;
  return own === null ? fill(en, 'en', params) : fill(own, language, params);
}

function fill(message: Message, language: string, params: Params): string {
  const text = isPlural(message) ? pluralForm(message, language, params['count']) : message;
  return text.replace(/\{(\w+)\}/g, (hole, name: string) =>
    name in params ? String(params[name]) : hole,
  );
}

function pluralForm(
  message: Exclude<Message, string>,
  language: string,
  count: string | number | undefined,
): string {
  const n = typeof count === 'number' ? count : Number(count);
  if (!Number.isFinite(n)) return message.other;
  return message[pluralRules(language).select(n)] ?? message.other;
}

const rules = new Map<string, Intl.PluralRules>();

function pluralRules(language: string): Intl.PluralRules {
  let r = rules.get(language);
  if (!r) {
    try {
      r = new Intl.PluralRules(language);
    } catch {
      r = new Intl.PluralRules('en');
    }
    rules.set(language, r);
  }
  return r;
}
