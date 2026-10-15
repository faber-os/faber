/**
 * npm run i18n:check [lang] [--strict] [--dir <folder>] [--template]
 *
 * Compares every translation with the English catalogue (docs/translations.md) and prints what
 * each one is missing: missing and extra keys, placeholders that differ, empty texts, and how
 * complete it is.
 *
 *   lang          only this language, e.g. `da`
 *   --strict      exit with 1 when anything is missing or wrong (for CI); otherwise always 0
 *   --dir <path>  also check the translations in a data folder (`<data>/config/translations`)
 *                 or in any folder of `<code>.json` files
 *   --template    print every English text as a JSON file to start a new translation from
 *
 * Runs on Node's built-in TypeScript support, with no build: it imports the catalogue and the
 * comparison rules (src/lib/i18n/check.ts) that the shell itself uses.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { en } from '../src/lib/i18n/en.ts';
import {
  checkReport,
  isLanguageCode,
  translationTemplate,
  type FoundTranslation,
} from '../src/lib/i18n/check.ts';

const shell = resolve(import.meta.dirname, '..');
const args = process.argv.slice(2);

if (args.includes('--template')) {
  process.stdout.write(translationTemplate(en));
  process.exit(0);
}

const strict = args.includes('--strict');
const dirAt = args.indexOf('--dir');
const dir = dirAt >= 0 ? args[dirAt + 1] : undefined;
if (dirAt >= 0 && !dir) fail('--dir needs a folder, e.g. --dir ../data');
const only = args.find((a, i) => !a.startsWith('--') && i !== dirAt + 1);
if (only !== undefined && !isLanguageCode(only)) fail(`"${only}" isn't a language code, e.g. da.`);

const folders = [join(shell, 'src/lib/i18n/locales')];
if (dir) {
  const data = join(resolve(dir), 'config', 'translations');
  folders.push(existsSync(data) ? data : resolve(dir));
}

const found: FoundTranslation[] = folders.flatMap(read);
const { lines, exitCode } = checkReport(en, found, { strict, only });
console.log(lines.join('\n'));
process.exit(exitCode);

function read(folder: string): FoundTranslation[] {
  if (!existsSync(folder)) fail(`There is no folder ${folder}.`);
  return readdirSync(folder)
    .filter((f) => f.endsWith('.json') && isLanguageCode(f.slice(0, -5)) && f !== 'en.json')
    .sort()
    .map((f) => {
      const path = join(folder, f);
      const file = relative(process.cwd(), path) || path;
      const text = readFileSync(path, 'utf8');
      try {
        const value: unknown = JSON.parse(text);
        if (typeof value !== 'object' || value === null || Array.isArray(value))
          return {
            code: f.slice(0, -5),
            file,
            content: { ok: false, error: 'not one JSON object' },
          };
        return { code: f.slice(0, -5), file, content: { ok: true, value: value as never } };
      } catch (e) {
        return { code: f.slice(0, -5), file, content: { ok: false, error: jsonError(e, text) } };
      }
    });
}

/** "line 3: Expected ',' or '}' …", from the position JSON.parse reports. */
function jsonError(e: unknown, text: string): string {
  const message = e instanceof Error ? e.message : String(e);
  const at = /position (\d+)/.exec(message);
  if (!at) return message;
  const line = text.slice(0, Number(at[1])).split('\n').length;
  return `line ${line}: ${message.replace(/ in JSON at position \d+.*$/, '')}`;
}

function fail(message: string): never {
  console.error(`i18n:check: ${message}`);
  process.exit(2);
}
