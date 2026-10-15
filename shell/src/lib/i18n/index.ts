/**
 * Translations (D44, docs/translations.md). Every text the shell shows comes from `t()`, which
 * reads the English catalogue (en.ts) and the translation in use, key by key.
 */
export { t, locale, type MessageKey } from './locale.svelte';
export { richParts, type RichPart } from './rich';
