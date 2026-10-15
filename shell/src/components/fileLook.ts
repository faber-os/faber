import { extension } from '../lib/desktop/filePaths';
import type { FileEntry, SpecialFolder } from '../lib/state/types';
import type { IconName } from './Icon.svelte';

/** The home folder's fixed folders, in sidebar order, with what each is for. */
export const SPECIALS: readonly {
  id: SpecialFolder;
  path: string;
  label: string;
  icon: IconName;
}[] = [
  { id: 'desktop', path: 'home:/Desktop', label: 'Desktop', icon: 'desktop' },
  { id: 'documents', path: 'home:/Documents', label: 'Documents', icon: 'fileText' },
  { id: 'recordings', path: 'home:/Recordings', label: 'Recordings', icon: 'recording' },
  { id: 'system', path: 'home:/System', label: 'System', icon: 'nut' },
  { id: 'trash', path: 'home:/Trash', label: 'Trash', icon: 'trash' },
];

const TEXT = new Set([
  'txt',
  'md',
  'toml',
  'json',
  'csv',
  'log',
  'ini',
  'yaml',
  'yml',
  'ino',
  'c',
  'h',
  'cpp',
  'rs',
  'py',
  'js',
  'ts',
]);

export function entryIcon(e: Pick<FileEntry, 'kind' | 'name' | 'special'>): IconName {
  if (e.special) return SPECIALS.find((s) => s.id === e.special)?.icon ?? 'folder';
  if (e.kind === 'folder') return 'folder';
  const ext = extension(e.name);
  if (ext === 'fbcap') return 'recording';
  return TEXT.has(ext) ? 'fileText' : 'file';
}
