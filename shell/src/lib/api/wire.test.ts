import { describe, expect, it } from 'vitest';
import { decode, encode } from '@msgpack/msgpack';
import { decodeServerMessage, encodeClientMessage, WireError, wireFormatFromSearch } from './wire';
import type { ServerMessage } from './generated/ServerMessage';

const scanJson = {
  type: 'signal',
  id: 3,
  address: 'lidar/laser-scan',
  reading: {
    capability: 'laser-scan@1',
    reading: {
      tUs: 1000,
      angleMin: 0,
      angleIncrement: 0.1,
      scanTime: 0.1,
      ranges: [1.5, null, 2],
      intensities: [0.5, 0, 1],
    },
  },
};

describe('encodeClientMessage', () => {
  it('encodes msgpack as binary', () => {
    const out = encodeClientMessage(
      { type: 'subscribe', id: 1, topic: { kind: 'parts' } },
      'msgpack',
    );
    expect(out).toBeInstanceOf(Uint8Array);
    expect(decode(out as Uint8Array)).toEqual({
      type: 'subscribe',
      id: 1,
      topic: { kind: 'parts' },
    });
  });

  it('keeps bytes as msgpack bin', () => {
    const out = encodeClientMessage(
      { type: 'serial-send', id: 2, data: new Uint8Array([1, 2, 255]) },
      'msgpack',
    );
    const back = decode(out as Uint8Array) as { data: Uint8Array };
    expect(back.data).toBeInstanceOf(Uint8Array);
    expect([...back.data]).toEqual([1, 2, 255]);
  });

  it('encodes JSON as text with bytes as an array of numbers', () => {
    const out = encodeClientMessage(
      { type: 'serial-send', id: 2, data: new Uint8Array([10, 13]) },
      'json',
    );
    expect(typeof out).toBe('string');
    expect(JSON.parse(out as string)).toEqual({ type: 'serial-send', id: 2, data: [10, 13] });
  });
});

describe('decodeServerMessage', () => {
  it('reads the settings, files, packages and recording messages', () => {
    const settings = {
      appearance: { theme: 'dark', wallpaper: 'graphite' },
      desktop: { icons: [{ key: 'tool:parts', x: 16, y: 16 }] },
      developer: { fullMachineFiles: false },
    };
    for (const m of [
      { type: 'settings', id: 1, settings },
      { type: 'files-changed', id: 2, folders: ['home:/Desktop'] },
      { type: 'packages', id: 4, packages: [] },
      { type: 'serial-recording', id: 3, path: 'home:/Recordings/a.fbcap' },
      { type: 'serial-recording-stopped', id: 3, path: 'home:/Recordings/a.fbcap', bytes: 9 },
    ]) {
      expect(decodeServerMessage(JSON.stringify(m), 'json')).toEqual(m);
    }
  });

  it('turns JSON null ranges back into Infinity', () => {
    const msg = decodeServerMessage(JSON.stringify(scanJson), 'json');
    expect(msg.type).toBe('signal');
    if (msg.type !== 'signal') return;
    expect(msg.reading.capability === 'laser-scan@1' ? msg.reading.reading.ranges : null).toEqual([
      1.5,
      Infinity,
      2,
    ]);
  });

  it('keeps real Infinity from msgpack', () => {
    const raw = structuredClone(scanJson);
    (raw.reading.reading.ranges as (number | null)[])[1] = Infinity;
    const bytes = encode(raw);
    const msg = decodeServerMessage(
      bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
      'msgpack',
    );
    if (msg.type !== 'signal') throw new Error('expected signal');
    expect(msg.reading.capability === 'laser-scan@1' ? msg.reading.reading.ranges : null).toEqual([
      1.5,
      Infinity,
      2,
    ]);
  });

  it('turns JSON byte arrays into Uint8Array', () => {
    const msg = decodeServerMessage(
      JSON.stringify({ type: 'serial-data', id: 7, tUs: 5, data: [72, 105] }),
      'json',
    );
    if (msg.type !== 'serial-data') throw new Error('expected serial-data');
    expect(msg.data).toBeInstanceOf(Uint8Array);
    expect([...msg.data]).toEqual([72, 105]);
  });

  it('decodes msgpack bin into Uint8Array', () => {
    const bytes = encode({ type: 'serial-data', id: 7, tUs: 5, data: new Uint8Array([1, 2]) });
    const msg = decodeServerMessage(bytes, 'msgpack') as Extract<
      ServerMessage,
      { type: 'serial-data' }
    >;
    expect([...msg.data]).toEqual([1, 2]);
  });

  it('accepts a binary frame in JSON mode as UTF-8 text', () => {
    const text = new TextEncoder().encode(JSON.stringify({ type: 'parts', id: 1, parts: [] }));
    expect(decodeServerMessage(text, 'json')).toEqual({ type: 'parts', id: 1, parts: [] });
  });

  it('drops optional fields sent as null or nil, in both formats', () => {
    const port = {
      id: 'serial:COM1',
      kind: 'serial',
      label: 'COM1',
      claimedBy: null,
      suggestions: [],
    };
    const json = decodeServerMessage(
      JSON.stringify({ type: 'ports', id: 1, ports: [port] }),
      'json',
    );
    const packed = decodeServerMessage(encode({ type: 'ports', id: 1, ports: [port] }), 'msgpack');
    for (const msg of [json, packed]) {
      if (msg.type !== 'ports') throw new Error('expected ports');
      expect('claimedBy' in msg.ports[0]!).toBe(false);
    }
    const err = decodeServerMessage(
      encode({ type: 'error', id: null, error: { code: 'x', message: 'm', hint: 'h' } }),
      'msgpack',
    );
    expect('id' in err).toBe(false);
  });

  it('rejects garbage with a WireError that says what to do', () => {
    expect(() => decodeServerMessage('{not json', 'json')).toThrow(WireError);
    expect(() => decodeServerMessage(JSON.stringify({ hello: 1 }), 'json')).toThrow(WireError);
    try {
      decodeServerMessage(JSON.stringify(42), 'json');
    } catch (e) {
      expect((e as WireError).body.hint).not.toBe('');
    }
  });
});

describe('wireFormatFromSearch', () => {
  it('uses msgpack unless the page asks for JSON', () => {
    expect(wireFormatFromSearch('')).toBe('msgpack');
    expect(wireFormatFromSearch('?wire=json')).toBe('json');
    expect(wireFormatFromSearch('?wire=xml&x=1')).toBe('msgpack');
  });
});
