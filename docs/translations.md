# Translations

Faber's desktop (the shell) can be shown in other languages (D44). It ships in English only. A
translation is one plain JSON file per language, and it works as soon as the file is there. You
don't need to know any code to write one.

## How a translation works

Every text the shell shows has a **key**, grouped by window, e.g. `parts.empty.title`. The English
texts live in `shell/src/lib/i18n/en.ts`. A translation is a file with the same keys and your
language's texts:

```json
{
  "_language": "Dansk",
  "settings.appearance.title": "Udseende",
  "parts.summary.count": { "one": "én del", "other": "{count} dele" },
  "settings.storage.backup.saved": "Gemte {name} ({size})."
}
```

- **The file name is the language code:** `da.json` for Danish, `sv.json` for Swedish,
  `pt-BR.json` for Brazilian Portuguese (2–3 lowercase letters, optionally a dash and 2 capitals).
- **`_language`** is the language's own name, as people who speak it write it. Settings shows it.
- **Anything you leave out shows in English**, key by key. A half-done translation is fine.
- **`{name}` is a placeholder**: Faber fills it in (a file name, a number…). Keep every
  placeholder from the English text, spelled exactly the same. You may move it anywhere in the
  sentence. A text with a placeholder English doesn't have is shown in English instead.
- **Plurals** are objects with one text per plural form. Which forms your language has comes from
  the Unicode plural rules: Danish and English use `one` and `other`; Polish uses `one`, `few`,
  `many` and `other`. `other` is required. The checker tells you which forms your language needs.
- **`<b>…</b>` and `<code>…</code>`** mark a bold word or a code word inside a sentence. Keep them
  around the matching words. No other markup works: the file is never read as HTML.
- **Keyboard shortcuts** inside a text (`Ctrl+S`) stay as they are.

## Making one

1. Get every English text as a starting file:

   ```bash
   cd shell
   npm run --silent i18n:check -- --template > da.json
   ```

2. Fill in `_language`, and replace the English texts you translate. Delete the lines you don't
   translate yet, or leave them in English.
3. Put the file in one of the two places below.
4. Run the checker, then pick the language in **Settings → Appearance → Language**. The picker
   only shows once there is more than one language; each entry says how complete it is, e.g.
   "Dansk (84 %)".

## Where to put it

| Where | When it works | For |
|---|---|---|
| `<data>/config/translations/da.json` on the machine (next to `parts.toml` and `settings.toml`) | at once, no restart: Settings reads the folder each time it opens | trying a translation, or a machine in one language |
| `shell/src/lib/i18n/locales/da.json` in the source | after rebuilding the shell | shipping a translation with Faber |

A file on the machine wins over a built-in one with the same code, so a fix can be tried without
a rebuild. A broken file (e.g. a missing comma) isn't offered in the picker; Settings shows the
file and the line instead. Translations will become store packages later (D44).

## Checking it

```bash
cd shell
npm run i18n:check                       # every built-in translation
npm run i18n:check -- da                 # only Danish
npm run i18n:check -- --dir ../data      # also the machine's translations (a data folder, or any folder of .json files)
npm run i18n:check -- --strict           # exit code 1 if anything is missing or wrong (for CI)
```

For each translation it prints how complete it is, then:

- **missing** keys, with the English text to translate;
- **placeholders** that differ from English (left out, or unknown and so shown in English);
- **empty** texts;
- texts that aren't the same **kind** as English (a plural where English has a plain text, or the
  other way round);
- **plural forms** your language needs that a plural is missing;
- **extra** keys that English doesn't have (an old key, or a typo). They are ignored.

Without `--strict` it always exits with 0: an incomplete translation is allowed. The checker, the
picker's percentage and the shell itself use the same rules (`shell/src/lib/i18n/check.ts`), so
what the checker calls translated is exactly what the shell shows.

## What isn't translated (yet)

- **Messages from the core.** Errors and hints the core sends (e.g. "COM3 is used by the part
  "lidar".") are shown as the core wrote them, in English. Only texts the shell makes itself are
  translated.
- **Data:** file and folder names (including Desktop, Documents, Recordings, Trash and System,
  which are real folders), part names, port ids and labels, connector names, units like `cm`.
- **"Copy diagnostics"** in Settings → About: it is a bug report for maintainers, so it stays in
  English.
- **Dates and times** keep their current format (day month year, 24- or 12-hour clock from
  Settings → Date and time). They don't follow the language yet.
- **Open windows keep their titles** when the language changes; new windows use the new language.

## For developers

- Never write visible text straight into a component or module. Add it to `en.ts` and show it
  with `t('key')` or `t('key', { name })` from `src/lib/i18n`. The key and its placeholders are
  type-checked: a missing key or parameter is a compile error.
- `src/lib/i18n/noRawText.test.ts` fails on visible text that doesn't come from `t()`, with the file
  and line. Its small allowlist (the name Faber, units, symbols) says why each entry is there.
- Write whole sentences with placeholders, never sentences glued together from pieces: other
  languages put words in a different order.
- `t()` is reactive: it follows a language change in templates and `$derived`. Don't call it when a
  module loads; use a function or a getter (see `windows/registry.ts`).
- Changing an English text means every translation of it is out of date. Prefer a new key when
  the meaning changes.
