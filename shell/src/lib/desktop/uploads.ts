import { t } from '../i18n';
import { uniqueName } from './filePaths';

/**
 * Uploading never overwrites by accident: each file is created with `mode=create`, and when one
 * with that name is already there the user picks Replace, Keep both or Skip (docs/api.md).
 */
export type ConflictChoice = 'replace' | 'keep-both' | 'skip';

export interface ConflictAnswer {
  choice: ConflictChoice;
  /** Do the same for every later conflict in this upload. */
  forAll: boolean;
}

/** What to write after the answer: the same name over the old file, a free name, or nothing. */
export function afterConflict(
  choice: ConflictChoice,
  name: string,
  namesInFolder: readonly string[],
): { name: string; mode: 'replace' | 'create' } | null {
  if (choice === 'skip') return null;
  if (choice === 'replace') return { name, mode: 'replace' };
  return { name: uniqueName(namesInFolder, name), mode: 'create' };
}

/** One line saying what an upload did, or null when every file went in under its own name. */
export function uploadSummary(r: {
  renamed: readonly { from: string; to: string }[];
  replaced: readonly string[];
  skipped: readonly string[];
}): string | null {
  const parts: string[] = [];
  if (r.renamed.length) {
    const names = r.renamed.map((x) => t('files.upload.renamedPair', x)).join(', ');
    parts.push(t('files.upload.keptBoth', { names }));
  }
  if (r.replaced.length) parts.push(t('files.upload.replaced', { names: r.replaced.join(', ') }));
  if (r.skipped.length) parts.push(t('files.upload.skipped', { names: r.skipped.join(', ') }));
  return parts.length ? parts.join(' ') : null;
}
