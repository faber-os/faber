import { fileType, type FileType } from '../lib/desktop/fileTypes';
import type { FileEntry, SpecialFolder } from '../lib/state/types';
import type { IconName } from './Icon.svelte';

/**
 * One of the home folder's fixed folders. Its label is its real name on disk (the core's), so it
 * is data, not catalogue text.
 */
function special(id: SpecialFolder, name: string, icon: IconName) {
  return { id, path: `home:/${name}`, label: name, icon };
}

/** The home folder's fixed folders, in sidebar order, with what each is for. */
export const SPECIALS: readonly {
  id: SpecialFolder;
  path: string;
  label: string;
  icon: IconName;
}[] = [
  special('desktop', 'Desktop', 'desktop'),
  special('documents', 'Documents', 'fileText'),
  special('recordings', 'Recordings', 'recording'),
  special('system', 'System', 'nut'),
  special('trash', 'Trash', 'trash'),
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
    case 'package':
      return 'package';
    case 'text':
    case 'markdown':
    case 'config':
    case 'code':
      return 'fileText';
    default:
      return 'file';
  }
}
