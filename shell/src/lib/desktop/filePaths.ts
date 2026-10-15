/**
 * File paths as the core writes them: `<root>:/<a>/<b>` (docs/api.md, "Files"). These only take
 * paths apart and put them together; every rule about what is allowed is the core's.
 */

export function splitPath(path: string): { root: string; segments: string[] } {
  const i = path.indexOf(':/');
  if (i === -1) return { root: '', segments: path.split('/').filter(Boolean) };
  return {
    root: path.slice(0, i),
    segments: path
      .slice(i + 2)
      .split('/')
      .filter(Boolean),
  };
}

export function rootPath(path: string): string {
  return `${splitPath(path).root}:/`;
}

export function joinPath(root: string, segments: readonly string[]): string {
  return `${root}:/${segments.join('/')}`;
}

export function childPath(folder: string, name: string): string {
  const { root, segments } = splitPath(folder);
  return joinPath(root, [...segments, name]);
}

export function parentPath(path: string): string | null {
  const { root, segments } = splitPath(path);
  if (segments.length === 0) return null;
  return joinPath(root, segments.slice(0, -1));
}

export function baseName(path: string): string {
  return splitPath(path).segments.at(-1) ?? '';
}

/** Lowercase, without the dot; '' when there is none (a leading dot is not an extension). */
export function extension(name: string): string {
  const i = name.lastIndexOf('.');
  return i <= 0 ? '' : name.slice(i + 1).toLowerCase();
}

export function stem(name: string): string {
  const i = name.lastIndexOf('.');
  return i <= 0 ? name : name.slice(0, i);
}

export function breadcrumbs(path: string, rootLabel: string): { label: string; path: string }[] {
  const { root, segments } = splitPath(path);
  return [
    { label: rootLabel, path: joinPath(root, []) },
    ...segments.map((s, i) => ({ label: s, path: joinPath(root, segments.slice(0, i + 1)) })),
  ];
}

export const TRASH = 'home:/Trash';
export const RECORDINGS = 'home:/Recordings';
export const DESKTOP = 'home:/Desktop';

/** Something inside the Trash (not the Trash folder itself). */
export function isInTrash(path: string): boolean {
  return path.startsWith(TRASH + '/');
}

/** Every `.fbcap` in Recordings is a replay port named after the file (D35). */
export function replayPortFor(path: string): string | null {
  if (parentPath(path) !== RECORDINGS) return null;
  const name = baseName(path);
  return extension(name) === 'fbcap' ? `replay:${stem(name)}` : null;
}

/** `wanted`, or `wanted 2`, `wanted 3` … (before the extension) if that name is taken. */
export function uniqueName(existing: readonly string[], wanted: string): string {
  const taken = new Set(existing.map((n) => n.toLowerCase()));
  if (!taken.has(wanted.toLowerCase())) return wanted;
  const ext = extension(wanted);
  const base = ext ? stem(wanted) : wanted;
  for (let n = 2; ; n++) {
    const name = ext ? `${base} ${n}.${ext}` : `${base} ${n}`;
    if (!taken.has(name.toLowerCase())) return name;
  }
}

/**
 * Whether a `files-changed` message makes the listing of `folder` out of date. The core sends a
 * root (`home:/`) when a lot changed at once: then everything under it is.
 */
export function isAffected(folder: string, changed: readonly string[]): boolean {
  const root = rootPath(folder);
  return changed.some((c) => c === folder || c === root);
}
