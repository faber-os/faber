import { en } from './en';
import { translate, type Params } from './translate';
import type { TranslationFile } from './check';

type English = typeof en;
/** Every text in the catalogue. A key that isn't in en.ts is a compile error. */
export type MessageKey = keyof English & string;

/** The `{name}` placeholders in a text, as a union of names. */
type Holes<S> = S extends `${string}{${infer P}}${infer R}` ? P | Holes<R> : never;
type HolesOf<M> = M extends string
  ? Holes<M>
  : M extends { readonly other: string }
    ? Holes<M[keyof M] & string>
    : never;
/** What a text needs filled in. `count` is a number: it picks the plural form. */
type ParamsFor<K extends MessageKey> = {
  readonly [P in HolesOf<English[K]>]: P extends 'count' ? number : string | number;
};
/** `t('a')` for a text without placeholders, `t('b', { name })` for one with them. */
export type TArgs<K extends MessageKey> = [HolesOf<English[K]>] extends [never]
  ? []
  : [params: ParamsFor<K>];

/** The language the shell shows. Reactive: every `t()` in a template updates when it changes. */
class Locale {
  /** `en`, or the code of the translation in use. */
  language = $state('en');
  translation = $state.raw<TranslationFile | null>(null);

  /** Shows `translation` (missing texts stay English), or English when it's null. */
  use(language: string, translation: TranslationFile | null): void {
    this.translation = translation;
    this.language = translation ? language : 'en';
  }
}

export const locale = new Locale();

/** The text for `key` in the current language, with its placeholders filled in. */
export function t<K extends MessageKey>(key: K, ...args: TArgs<K>): string {
  return translate(en, locale.language, locale.translation, key, args[0] as Params | undefined);
}
