import { describe, expect, it } from 'vitest';
import { isPackageFile, kindWords, partList, touchWords } from './packages';
import { fileType, fileTypeLabel } from './fileTypes';

describe('packages', () => {
  it('knows package files by their extension', () => {
    expect(isPackageFile('acme-ranger-0.1.0.faberpkg')).toBe(true);
    expect(isPackageFile('ACME.FABERPKG')).toBe(true);
    expect(isPackageFile('acme.zip')).toBe(false);
    expect(fileType({ kind: 'file', name: 'a.faberpkg' })).toBe('package');
    expect(fileTypeLabel({ kind: 'file', name: 'a.faberpkg' })).toBe('Connector package');
  });

  it('says what a connector can touch', () => {
    expect(touchWords({ kind: 'serial', baud: 9600 })).toBe('A serial port you pick, at 9600 baud');
    expect(touchWords({ kind: 'bridge' })).toBe('A FaberLink bridge you pick');
  });

  it('names how it runs, and the parts that use it', () => {
    expect(kindWords('wasm')).toBe('Code, run sandboxed (WebAssembly)');
    expect(kindWords('declarative')).toBe('Settings only (declarative)');
    expect(partList(['front', 'rear'])).toBe('"front", "rear"');
  });
});
