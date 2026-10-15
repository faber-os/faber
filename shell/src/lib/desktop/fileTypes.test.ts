import { describe, expect, it } from 'vitest';
import { fileType, fileTypeLabel, isTextType } from './fileTypes';

const file = (name: string) => ({ kind: 'file', name });

describe('fileType', () => {
  it('knows folders, text, markdown, settings, code, recordings and images', () => {
    expect(fileType({ kind: 'folder', name: 'robot.md' })).toBe('folder');
    expect(fileType(file('wiring.txt'))).toBe('text');
    expect(fileType(file('drive-test.MD'))).toBe('markdown');
    expect(fileType(file('parts.toml'))).toBe('config');
    expect(fileType(file('blink.ino'))).toBe('code');
    expect(fileType(file('ld19-sample.fbcap'))).toBe('recording');
    expect(fileType(file('photo.jpeg'))).toBe('image');
    expect(fileType(file('firmware.bin'))).toBe('generic');
    expect(fileType(file('.hidden'))).toBe('generic');
  });

  it('says which types Notes can open', () => {
    expect(isTextType('config')).toBe(true);
    expect(isTextType('recording')).toBe(false);
    expect(isTextType('image')).toBe(false);
  });

  it('names the type for the details panel', () => {
    expect(fileTypeLabel(file('parts.toml'))).toBe('TOML settings');
    expect(fileTypeLabel(file('a.fbcap'))).toBe('Recording');
    expect(fileTypeLabel(file('x.bin'))).toBe('BIN file');
    expect(fileTypeLabel(file('README'))).toBe('File');
    expect(fileTypeLabel({ kind: 'folder', name: 'x' })).toBe('Folder');
  });
});
