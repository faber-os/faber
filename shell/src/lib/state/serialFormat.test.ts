import { describe, expect, it } from 'vitest';
import { ByteLog, bytesToText, encodeLine, hexLines } from './serialFormat';

describe('hexLines', () => {
  it('shows offset, hex bytes and printable ASCII', () => {
    const bytes = new TextEncoder().encode('Hello, Faber!\n\x00\xff');
    const lines = hexLines(bytes, 8);
    expect(lines).toEqual([
      '00000000  48 65 6c 6c 6f 2c 20 46  Hello, F',
      '00000008  61 62 65 72 21 0a 00 c3  aber!...',
      '00000010  bf                       .',
    ]);
  });

  it('can start counting at an offset', () => {
    expect(hexLines(new Uint8Array([1]), 16, 0x20)[0]).toMatch(/^00000020  01 /);
  });

  it('is empty for no bytes', () => {
    expect(hexLines(new Uint8Array(), 16)).toEqual([]);
  });
});

describe('bytesToText', () => {
  it('decodes UTF-8 and normalises line endings', () => {
    expect(bytesToText(new TextEncoder().encode('a\r\nb\rc\nø'))).toBe('a\nb\nc\nø');
  });
});

describe('encodeLine', () => {
  it('adds the chosen line ending', () => {
    expect([...encodeLine('hi', 'none')]).toEqual([104, 105]);
    expect([...encodeLine('hi', 'lf')]).toEqual([104, 105, 10]);
    expect([...encodeLine('hi', 'cr')]).toEqual([104, 105, 13]);
    expect([...encodeLine('hi', 'crlf')]).toEqual([104, 105, 13, 10]);
  });
});

describe('ByteLog', () => {
  it('appends chunks and keeps only the newest bytes', () => {
    const log = new ByteLog(5);
    log.append(new Uint8Array([1, 2, 3]));
    log.append(new Uint8Array([4, 5, 6, 7]));
    expect([...log.bytes()]).toEqual([3, 4, 5, 6, 7]);
    expect(log.total).toBe(7);
    // The offset of the first byte kept, for the hex view.
    expect(log.firstOffset).toBe(2);
  });

  it('handles one chunk bigger than the limit', () => {
    const log = new ByteLog(2);
    log.append(new Uint8Array([1, 2, 3, 4]));
    expect([...log.bytes()]).toEqual([3, 4]);
  });

  it('clears', () => {
    const log = new ByteLog(10);
    log.append(new Uint8Array([1]));
    log.clear();
    expect(log.bytes().length).toBe(0);
    expect(log.total).toBe(0);
  });
});
