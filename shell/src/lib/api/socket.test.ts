import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { decode, encode } from '@msgpack/msgpack';
import { FakeWebSocket } from '../../test/fakeWebSocket';
import { CoreSocket, backoffDelay, wsUrl } from './socket';
import type { ServerMessage } from './generated/ServerMessage';

function sentMsgpack(ws: FakeWebSocket): unknown[] {
  return ws.sent.map((d) => decode(d as Uint8Array));
}

function bin(msg: unknown): ArrayBuffer {
  const b = encode(msg);
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
}

function make(format: 'msgpack' | 'json' = 'msgpack') {
  return new CoreSocket({
    url: 'ws://core/api/v0/ws?format=' + format,
    format,
    WebSocketImpl: FakeWebSocket,
  });
}

beforeEach(() => {
  FakeWebSocket.reset();
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe('wsUrl', () => {
  it('builds the WebSocket address from the page address', () => {
    expect(wsUrl({ protocol: 'http:', host: 'faber.local' }, 'msgpack')).toBe(
      'ws://faber.local/api/v0/ws?format=msgpack',
    );
    expect(wsUrl({ protocol: 'https:', host: 'x:8443' }, 'json')).toBe(
      'wss://x:8443/api/v0/ws?format=json',
    );
  });
});

describe('backoffDelay', () => {
  it('doubles from 500 ms and caps at 10 s', () => {
    expect([0, 1, 2, 3, 4, 5, 6].map((n) => backoffDelay(n))).toEqual([
      500, 1000, 2000, 4000, 8000, 10000, 10000,
    ]);
  });
});

describe('CoreSocket', () => {
  it('reports connecting, then connected', () => {
    const s = make();
    const seen: string[] = [];
    s.onStatus((st) => seen.push(st));
    s.connect();
    expect(FakeWebSocket.latest().binaryType).toBe('arraybuffer');
    FakeWebSocket.latest().open();
    expect(seen).toEqual(['connecting', 'connected']);
    expect(s.status).toBe('connected');
  });

  it('queues subscriptions until the socket opens, then sends them', () => {
    const s = make();
    s.connect();
    s.subscribe({ kind: 'parts' }, { onMessage: () => {} });
    const ws = FakeWebSocket.latest();
    expect(ws.sent).toHaveLength(0);
    ws.open();
    expect(sentMsgpack(ws)).toEqual([{ type: 'subscribe', id: 1, topic: { kind: 'parts' } }]);
  });

  it('routes messages to the subscription with the same id', () => {
    const s = make();
    s.connect();
    const ws = FakeWebSocket.latest();
    ws.open();
    const parts: ServerMessage[] = [];
    const ports: ServerMessage[] = [];
    s.subscribe({ kind: 'parts' }, { onMessage: (m) => parts.push(m) });
    s.subscribe({ kind: 'ports' }, { onMessage: (m) => ports.push(m) });
    ws.receive(bin({ type: 'ports', id: 2, ports: [] }));
    ws.receive(bin({ type: 'parts', id: 1, parts: [] }));
    expect(parts).toHaveLength(1);
    expect(ports).toHaveLength(1);
    expect(ports[0]?.type).toBe('ports');
  });

  it('sends an error with an id to that subscription only', () => {
    const s = make();
    s.connect();
    const ws = FakeWebSocket.latest();
    ws.open();
    const onError = vi.fn();
    const global = vi.fn();
    s.onError(global);
    s.subscribe({ kind: 'signal', address: 'nope/laser-scan' }, { onMessage: () => {}, onError });
    const error = {
      code: 'unknown-signal',
      message: 'No signal nope/laser-scan.',
      hint: 'Check the name.',
    };
    ws.receive(bin({ type: 'error', id: 1, error }));
    expect(onError).toHaveBeenCalledWith(error);
    expect(global).not.toHaveBeenCalled();
  });

  it('sends errors without an id, and unreadable frames, to the global listeners', () => {
    const s = make();
    s.connect();
    const ws = FakeWebSocket.latest();
    ws.open();
    const global = vi.fn();
    s.onError(global);
    ws.receive(bin({ type: 'error', error: { code: 'x', message: 'm', hint: 'h' } }));
    ws.receive(new Uint8Array([0xc1]).buffer);
    expect(global).toHaveBeenCalledTimes(2);
    expect(global.mock.calls[1]?.[0].code).toBe('bad-message');
  });

  it('unsubscribes and stops delivering', () => {
    const s = make();
    s.connect();
    const ws = FakeWebSocket.latest();
    ws.open();
    const got = vi.fn();
    const off = s.subscribe({ kind: 'logs' }, { onMessage: got });
    off();
    ws.receive(
      bin({
        type: 'log',
        id: 1,
        entry: { tUs: 1, level: 'info', source: 'core', message: 'hi' },
      }),
    );
    expect(got).not.toHaveBeenCalled();
    expect(sentMsgpack(ws).at(-1)).toEqual({ type: 'unsubscribe', id: 1 });
  });

  it('reconnects with backoff and re-sends active subscriptions', () => {
    const s = make();
    const seen: string[] = [];
    s.onStatus((st) => seen.push(st));
    s.connect();
    FakeWebSocket.latest().open();
    s.subscribe({ kind: 'parts' }, { onMessage: () => {} });
    const off = s.subscribe({ kind: 'ports' }, { onMessage: () => {} });
    off();

    FakeWebSocket.latest().drop();
    expect(s.status).toBe('reconnecting');
    expect(FakeWebSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(499);
    expect(FakeWebSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeWebSocket.instances).toHaveLength(2);

    // The second attempt fails too: the next wait is longer.
    FakeWebSocket.latest().drop();
    vi.advanceTimersByTime(999);
    expect(FakeWebSocket.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeWebSocket.instances).toHaveLength(3);

    const ws = FakeWebSocket.latest();
    ws.open();
    expect(s.status).toBe('connected');
    expect(sentMsgpack(ws)).toEqual([{ type: 'subscribe', id: 1, topic: { kind: 'parts' } }]);
    expect(seen).toEqual(['connecting', 'connected', 'reconnecting', 'connected']);

    // After a good connection the backoff starts from the beginning again.
    ws.drop();
    vi.advanceTimersByTime(500);
    expect(FakeWebSocket.instances).toHaveLength(4);
  });

  it('does not reconnect after close()', () => {
    const s = make();
    s.connect();
    FakeWebSocket.latest().open();
    s.close();
    expect(s.status).toBe('closed');
    vi.advanceTimersByTime(60_000);
    expect(FakeWebSocket.instances).toHaveLength(1);
  });

  it('speaks JSON in JSON mode, including Infinity normalisation', () => {
    const s = make('json');
    s.connect();
    const ws = FakeWebSocket.latest();
    ws.open();
    const got: ServerMessage[] = [];
    s.subscribe(
      { kind: 'signal', address: 'demo/laser-scan', maxHz: 15 },
      { onMessage: (m) => got.push(m) },
    );
    expect(JSON.parse(ws.sent[0] as string)).toEqual({
      type: 'subscribe',
      id: 1,
      topic: { kind: 'signal', address: 'demo/laser-scan', maxHz: 15 },
    });
    ws.receive(
      JSON.stringify({
        type: 'signal',
        id: 1,
        address: 'demo/laser-scan',
        reading: {
          capability: 'laser-scan@1',
          reading: { tUs: 1, angleMin: 0, angleIncrement: 1, scanTime: 0.1, ranges: [null, 1] },
        },
      }),
    );
    const m = got[0];
    if (m?.type !== 'signal') throw new Error('expected a signal');
    expect(m.reading.reading.ranges).toEqual([Infinity, 1]);
  });

  describe('serial sessions', () => {
    it('opens, receives data, sends bytes and closes', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onData = vi.fn();
      const onClosed = vi.fn();
      const onOpened = vi.fn();
      const session = s.openSerial('serial:COM3', 115200, {
        onOpened,
        onData,
        onClosed,
        onError: () => {},
      });
      expect(sentMsgpack(ws)[0]).toEqual({
        type: 'serial-open',
        id: 1,
        port: 'serial:COM3',
        baud: 115200,
      });
      expect(onOpened).not.toHaveBeenCalled();

      ws.receive(bin({ type: 'serial-opened', id: 1, port: 'serial:COM3' }));
      expect(onOpened).toHaveBeenCalledWith('serial:COM3');

      ws.receive(bin({ type: 'serial-data', id: 1, tUs: 9, data: new Uint8Array([104, 105]) }));
      expect(onData).toHaveBeenCalledWith(new Uint8Array([104, 105]), 9);

      session.send(new Uint8Array([65]));
      const sent = sentMsgpack(ws)[1] as { type: string; data: Uint8Array };
      expect(sent.type).toBe('serial-send');
      expect([...sent.data]).toEqual([65]);

      session.close();
      expect(sentMsgpack(ws)[2]).toEqual({ type: 'serial-close', id: 1 });
      expect(onClosed).toHaveBeenCalledTimes(1);
    });

    it('keeps an opened session alive when one request fails', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onError = vi.fn();
      const onClosed = vi.fn();
      const session = s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError,
      });
      ws.receive(bin({ type: 'serial-opened', id: 1, port: 'serial:COM3' }));
      const error = { code: 'serial-write-failed', message: 'Could not send.', hint: 'Check it.' };
      ws.receive(bin({ type: 'error', id: 1, error }));
      expect(onError).toHaveBeenCalledWith(error);
      expect(onClosed).not.toHaveBeenCalled();
      session.send(new Uint8Array([1]));
      expect(sentMsgpack(ws).at(-1)).toMatchObject({ type: 'serial-send', id: 1 });
    });

    it('reports a port-claimed error and ends the session', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onError = vi.fn();
      const onClosed = vi.fn();
      s.openSerial('usb:10c4:ea60', 230400, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError,
      });
      const error = {
        code: 'port-claimed',
        message: 'usb:10c4:ea60 is used by the part "lidar".',
        hint: 'Remove or stop "lidar" first, or pick another port.',
      };
      ws.receive(bin({ type: 'error', id: 1, error }));
      expect(onError).toHaveBeenCalledWith(error);
      expect(onClosed).toHaveBeenCalledTimes(1);
    });

    it('reports the core closing the port', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onClosed = vi.fn();
      s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError: () => {},
      });
      ws.receive(bin({ type: 'serial-closed', id: 1, reason: 'The adapter was unplugged.' }));
      expect(onClosed).toHaveBeenCalledWith('The adapter was unplugged.');
    });

    it('ends sessions when the connection drops, saying why', () => {
      const s = make();
      s.connect();
      FakeWebSocket.latest().open();
      const onClosed = vi.fn();
      s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError: () => {},
      });
      FakeWebSocket.latest().drop();
      expect(onClosed).toHaveBeenCalledWith(expect.stringContaining('connection to the core'));
      vi.advanceTimersByTime(500);
      FakeWebSocket.latest().open();
      expect(FakeWebSocket.latest().sent).toHaveLength(0);
    });

    it('refuses to open while not connected', () => {
      const s = make();
      s.connect();
      const onError = vi.fn();
      const onClosed = vi.fn();
      s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError,
      });
      expect(onError).toHaveBeenCalledWith(expect.objectContaining({ code: 'not-connected' }));
      expect(onClosed).toHaveBeenCalled();
    });

    it('records an open session and reports where to and how much', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onRecording = vi.fn();
      const onRecordingStopped = vi.fn();
      const session = s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed: () => {},
        onError: () => {},
        onRecording,
        onRecordingStopped,
      });
      ws.receive(bin({ type: 'serial-opened', id: 1, port: 'serial:COM3' }));
      session.startRecording('bench_test');
      expect(sentMsgpack(ws).at(-1)).toEqual({
        type: 'serial-record-start',
        id: 1,
        name: 'bench_test',
      });
      ws.receive(
        bin({ type: 'serial-recording', id: 1, path: 'home:/Recordings/bench_test.fbcap' }),
      );
      expect(onRecording).toHaveBeenCalledWith('home:/Recordings/bench_test.fbcap');
      session.stopRecording();
      expect(sentMsgpack(ws).at(-1)).toEqual({ type: 'serial-record-stop', id: 1 });
      ws.receive(
        bin({
          type: 'serial-recording-stopped',
          id: 1,
          path: 'home:/Recordings/bench_test.fbcap',
          bytes: 4096,
        }),
      );
      expect(onRecordingStopped).toHaveBeenCalledWith('home:/Recordings/bench_test.fbcap', 4096);
    });

    it('still reports the recording a close ended, until the core says the session is over', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onClosed = vi.fn();
      const onData = vi.fn();
      const onRecordingStopped = vi.fn();
      const session = s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData,
        onClosed,
        onError: () => {},
        onRecording: () => {},
        onRecordingStopped,
      });
      ws.receive(bin({ type: 'serial-opened', id: 1, port: 'serial:COM3' }));
      session.startRecording('bench_test');
      ws.receive(
        bin({ type: 'serial-recording', id: 1, path: 'home:/Recordings/bench_test.fbcap' }),
      );

      session.close();
      expect(sentMsgpack(ws).at(-1)).toEqual({ type: 'serial-close', id: 1 });
      expect(onClosed).toHaveBeenCalledTimes(1);

      // Data still in flight is not shown any more, and nothing more can be sent.
      ws.receive(bin({ type: 'serial-data', id: 1, tUs: 9, data: new Uint8Array([1]) }));
      expect(onData).not.toHaveBeenCalled();
      session.send(new Uint8Array([2]));
      session.close();
      expect(sentMsgpack(ws).at(-1)).toEqual({ type: 'serial-close', id: 1 });

      ws.receive(
        bin({
          type: 'serial-recording-stopped',
          id: 1,
          path: 'home:/Recordings/bench_test.fbcap',
          bytes: 512,
        }),
      );
      expect(onRecordingStopped).toHaveBeenCalledWith('home:/Recordings/bench_test.fbcap', 512);
      ws.receive(bin({ type: 'serial-closed', id: 1 }));
      expect(onClosed).toHaveBeenCalledTimes(1);

      // After serial-closed the session is gone: a stray report reaches nobody.
      ws.receive(
        bin({
          type: 'serial-recording-stopped',
          id: 1,
          path: 'home:/Recordings/x.fbcap',
          bytes: 1,
        }),
      );
      expect(onRecordingStopped).toHaveBeenCalledTimes(1);
    });

    it('does not report a closed session as lost when the connection drops', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const onClosed = vi.fn();
      const session = s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed,
        onError: () => {},
      });
      ws.receive(bin({ type: 'serial-opened', id: 1, port: 'serial:COM3' }));
      session.close();
      ws.drop();
      expect(onClosed).toHaveBeenCalledTimes(1);
      expect(onClosed).toHaveBeenCalledWith();
    });

    it('does not ask to record before the port is open', () => {
      const s = make();
      s.connect();
      const ws = FakeWebSocket.latest();
      ws.open();
      const session = s.openSerial('serial:COM3', 9600, {
        onOpened: () => {},
        onData: () => {},
        onClosed: () => {},
        onError: () => {},
      });
      session.startRecording('x');
      expect(sentMsgpack(ws).map((m) => (m as { type: string }).type)).toEqual(['serial-open']);
    });
  });
});
