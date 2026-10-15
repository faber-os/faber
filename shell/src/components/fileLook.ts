import { fileType, type FileType } from '../lib/desktop/fileTypes';
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

/** How FileIcon draws an entry: its type, and an emblem on Faber's fixed folders. */
export function fileLook(e: Pick<FileEntry, 'kind' | 'name' | 'special'>): {
  type: FileType;
  emblem?: IconName;
} {
  if (e.special) {
    const icon = SPECIALS.find((s) => s.id === e.special)?.icon;
    return icon ? { type: 'folder', emblem: icon } : { type: 'folder' };
  }
  return { type: fileType(e) };
}

/** A one-colour glyph for an entry, where a full FileIcon is too much (menus, small lists). */
export function entryIcon(e: Pick<FileEntry, 'kind' | 'name' | 'special'>): IconName {
  if (e.special) return SPECIALS.find((s) => s.id === e.special)?.icon ?? 'folder';
  switch (fileType(e)) {
    case 'folder':
      return 'folder';
    case 'recording':
      return 'recording';
    case 'text':
    case 'markdown':
    case 'config':
    case 'code':
      return 'fileText';
    default:
      return 'file';
  }
}
