import type { UploadItem } from './uploadFiles';

/** What was dropped: files (with paths inside dropped folders), and the folders to make. */
export interface Dropped {
  files: UploadItem[];
  /** Folders to create, parents first (`photos`, `photos/2026`). */
  folders: string[];
  /** Files the browser handed over that can't be read: folders, in a browser without entries. */
  unreadable: string[];
}

/** The parts of the File and Directory Entries API a drop gives (webkitGetAsEntry). */
interface Entry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
}
interface FileEntryLike extends Entry {
  file(ok: (f: File) => void, fail?: (e: unknown) => void): void;
}
interface DirEntryLike extends Entry {
  createReader(): { readEntries(ok: (e: Entry[]) => void, fail?: (e: unknown) => void): void };
}

/**
 * Everything a drop holds. Folders come with what is in them when the browser gives folder
 * entries (`webkitGetAsEntry`, in every current browser); otherwise a folder arrives as a file
 * that can't be read, and is listed in `unreadable` so the caller can say so.
 */
export async function readDrop(dt: DataTransfer): Promise<Dropped> {
  const out: Dropped = { files: [], folders: [], unreadable: [] };
  const entries = Array.from(dt.items ?? [])
    .filter((i) => i.kind === 'file')
    .map((i) => (i.webkitGetAsEntry ? (i.webkitGetAsEntry() as Entry | null) : null));
  if (entries.length > 0 && entries.every((e) => e !== null)) {
    for (const e of entries) await walk(e!, '', out);
    return out;
  }
  for (const f of Array.from(dt.files ?? [])) {
    if (await readable(f)) out.files.push({ path: f.name, file: f });
    else out.unreadable.push(f.name);
  }
  return out;
}

async function walk(e: Entry, dir: string, out: Dropped): Promise<void> {
  const path = dir ? `${dir}/${e.name}` : e.name;
  if (e.isFile) {
    const file = await new Promise<File>((ok, fail) => (e as FileEntryLike).file(ok, fail));
    out.files.push({ path, file });
    return;
  }
  if (!e.isDirectory) return;
  out.folders.push(path);
  const reader = (e as DirEntryLike).createReader();
  // readEntries answers in batches until it answers with none.
  for (;;) {
    const batch = await new Promise<Entry[]>((ok, fail) => reader.readEntries(ok, fail));
    if (batch.length === 0) break;
    for (const child of batch) await walk(child, path, out);
  }
}

async function readable(f: File): Promise<boolean> {
  try {
    await f.slice(0, 1).arrayBuffer();
    return true;
  } catch {
    return false;
  }
}
