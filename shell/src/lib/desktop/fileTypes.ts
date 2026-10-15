import { t } from '../i18n';
import { extension } from './filePaths';

/**
 * Which picture a file or folder gets (components/FileIcon.svelte draws them). Decided by the
 * name only: the shell never reads a file to choose its icon.
 */
export type FileType =
  'folder' | 'text' | 'markdown' | 'config' | 'code' | 'recording' | 'image' | 'generic';

const BY_EXTENSION: Record<string, FileType> = {
  txt: 'text',
  log: 'text',
  csv: 'text',
  md: 'markdown',
  markdown: 'markdown',
  toml: 'config',
  json: 'config',
  yaml: 'config',
  yml: 'config',
  ini: 'config',
  cfg: 'config',
  conf: 'config',
  ino: 'code',
  c: 'code',
  h: 'code',
  cpp: 'code',
  hpp: 'code',
  rs: 'code',
  py: 'code',
  js: 'code',
  ts: 'code',
  fbcap: 'recording',
  png: 'image',
  jpg: 'image',
  jpeg: 'image',
  gif: 'image',
  webp: 'image',
  svg: 'image',
  bmp: 'image',
};

export function fileType(entry: { kind: string; name: string }): FileType {
  if (entry.kind === 'folder') return 'folder';
  return BY_EXTENSION[extension(entry.name)] ?? 'generic';
}

/** Types Notes can open as text. */
export function isTextType(t: FileType): boolean {
  return t === 'text' || t === 'markdown' || t === 'config' || t === 'code';
}

/** A short name for the type, for the details panel ("Markdown text", "Recording"). */
export function fileTypeLabel(entry: { kind: string; name: string }): string {
  const type = fileType(entry);
  const ext = extension(entry.name).toUpperCase();
  switch (type) {
    case 'folder':
      return t('files.type.folder');
    case 'text':
      return t('files.type.text');
    case 'markdown':
      return t('files.type.markdown');
    case 'config':
      return t('files.type.config', { ext });
    case 'code':
      return t('files.type.code', { ext });
    case 'recording':
      return t('files.type.recording');
    case 'image':
      return t('files.type.image', { ext });
    default:
      return ext ? t('files.type.other', { ext }) : t('files.type.file');
  }
}
