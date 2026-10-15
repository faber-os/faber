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
  const t = fileType(entry);
  const ext = extension(entry.name).toUpperCase();
  switch (t) {
    case 'folder':
      return 'Folder';
    case 'text':
      return 'Text';
    case 'markdown':
      return 'Markdown text';
    case 'config':
      return `${ext} settings`;
    case 'code':
      return `${ext} source code`;
    case 'recording':
      return 'Recording';
    case 'image':
      return `${ext} image`;
    default:
      return ext ? `${ext} file` : 'File';
  }
}
