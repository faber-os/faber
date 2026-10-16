import { childPath, uniqueName } from '../lib/desktop/filePaths';
import { afterConflict, type ConflictAnswer } from '../lib/desktop/uploads';
import type { CoreState } from '../lib/state/core.svelte';
import type { ErrorBody } from '../lib/state/types';

/** A file to upload, at a path inside the target folder (`notes.txt`, or `photos/a.jpg`). */
export interface UploadItem {
  path: string;
  file: Blob;
}

export interface UploadResult {
  renamed: { from: string; to: string }[];
  replaced: string[];
  skipped: string[];
  /** Where the last file went, to select it. */
  lastPath: string | null;
  /** Why it stopped, if it did. */
  error: ErrorBody | null;
}

/**
 * Uploads into `folder` (Files, and files dropped on the desktop). Nothing is overwritten by
 * accident: each file is created with `mode=create`, and a name that is taken asks `ask`
 * (Replace, Keep both or Skip, maybe for all the rest). Folders in `folders` (paths inside
 * `folder`, parents first) are made first; one that is already there is used as it is.
 */
export async function uploadFiles(
  core: CoreState,
  folder: string,
  items: readonly UploadItem[],
  namesInFolder: readonly string[],
  ask: (name: string, remaining: number) => Promise<ConflictAnswer>,
  folders: readonly string[] = [],
): Promise<UploadResult> {
  const out: UploadResult = { renamed: [], replaced: [], skipped: [], lastPath: null, error: null };
  for (const f of folders) {
    const r = await core.createFolder(childPath(folder, f));
    if (!r.ok && r.error.code !== 'already-exists') {
      out.error = r.error;
      return out;
    }
  }
  const names = [...namesInFolder];
  let rule: ConflictAnswer | null = null;
  const taken = (r: { ok: boolean; error?: ErrorBody }) =>
    !r.ok && r.error?.code === 'already-exists';
  for (const [i, item] of items.entries()) {
    const slash = item.path.lastIndexOf('/');
    const dir = slash < 0 ? folder : childPath(folder, item.path.slice(0, slash));
    const original = item.path.slice(slash + 1);
    // Names are only known for the folder itself; inside a dropped folder, ask the core.
    const known = slash < 0 ? names : [];
    let name = original;
    let r = await core.writeFile(childPath(dir, name), item.file, 'create');
    if (taken(r)) {
      const answer: ConflictAnswer = rule ?? (await ask(item.path, items.length - i - 1));
      if (answer.forAll) rule = answer;
      if (!known.includes(name)) known.push(name);
      const next = afterConflict(answer.choice, name, known);
      if (!next) {
        out.skipped.push(item.path);
        continue;
      }
      name = next.name;
      r = await core.writeFile(childPath(dir, name), item.file, next.mode);
      // Keep both: another upload may take the free name meanwhile; try the next one.
      for (let tries = 0; next.mode === 'create' && taken(r) && tries < 20; tries++) {
        known.push(name);
        name = uniqueName(known, original);
        r = await core.writeFile(childPath(dir, name), item.file, 'create');
      }
      if (r.ok && next.mode === 'replace') out.replaced.push(name);
      else if (r.ok) out.renamed.push({ from: original, to: name });
    }
    if (!r.ok) {
      out.error = r.error;
      break;
    }
    known.push(name);
    out.lastPath = childPath(dir, name);
  }
  return out;
}
