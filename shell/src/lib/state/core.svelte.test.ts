import { beforeEach, describe, expect, it, vi } from 'vitest';
import { decode, encode } from '@msgpack/msgpack';
import { CoreSocket } from '../api/socket';
import { createHttp } from '../api/http';
import { FakeWebSocket } from '../../test/fakeWebSocket';
import { fakeFetch, type FakeFetch } from '../../test/fakeFetch';
import { CoreState, LOG_CAPACITY } from './core.svelte';
import type { PartInfo } from './types';

const demo: PartInfo = {
  name: 'demo',
  connector: 'ldrobot-ld19',
  port: 'replay:demo-ld19',
  state: { kind: 'running' },
  signals: [{ address: 'demo/laser-scan', capability: 'laser-scan@1' }],
  mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
};

function bin(msg: unknown): ArrayBuffer {
  const b = encode(msg);
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
}

let f: FakeFetch;

function setup() {
  f = fakeFetch([
    { path: '/api/v0/system', body: { name: 'Faber', version: '0.1.0', timeUs: 5_000_000 } },
    { path: '/api/v0/connectors', body: [] },
  ]);
  const socket = new CoreSocket({
    url: 'ws://x/api/v0/ws',
    format: 'msgpack',
    WebSocketImpl: FakeWebSocket,
  });
  const core = new CoreState({
    http: createHttp(f.fetch),
    socket,
    format: 'msgpack',
    schedule: (fn) => fn(),
    now: () => 1_000_000,
  });
  core.start();
  const ws = FakeWebSocket.latest();
  ws.open();
  return { core, ws };
}

function sentTopics(ws: FakeWebSocket) {
  return ws.sent.map(
    (d) => decode(d as Uint8Array) as { type: string; id: number; topic?: { kind: string } },
  );
}

beforeEach(() => FakeWebSocket.reset());

describe('CoreState', () => {
  it('tracks the connection status', () => {
    const { core, ws } = setup();
    expect(core.connection).toBe('connected');
    ws.drop();
    expect(core.connection).toBe('reconnecting');
  });

  it('subscribes to parts, ports and logs on start', () => {
    const { ws } = setup();
    expect(sentTopics(ws).map((m) => m.topic?.kind)).toEqual(['parts', 'ports', 'logs']);
  });

  it('keeps parts and ports from their topics', () => {
    const { core, ws } = setup();
    expect(core.parts).toBeNull();
    const [parts, ports] = sentTopics(ws);
    ws.receive(bin({ type: 'parts', id: parts!.id, parts: [demo] }));
    ws.receive(bin({ type: 'ports', id: ports!.id, ports: [] }));
    expect(core.parts).toEqual([demo]);
    expect(core.ports).toEqual([]);
  });

  it('keeps at most the newest 1000 log entries', () => {
    const { core, ws } = setup();
    const logs = sentTopics(ws)[2]!;
    for (let i = 0; i < LOG_CAPACITY + 5; i++) {
      ws.receive(
        bin({
          type: 'log',
          id: logs.id,
          entry: { tUs: i, level: 'info', source: 'core', message: `m${i}` },
        }),
      );
    }
    expect(LOG_CAPACITY).toBe(1000);
    expect(core.logs).toHaveLength(1000);
    expect(core.logs[0]!.tUs).toBe(5);
  });

  it('starts the log afresh after a reconnect, because the core replays recent entries', () => {
    const { core, ws } = setup();
    const logs = sentTopics(ws)[2]!;
    ws.receive(
      bin({
        type: 'log',
        id: logs.id,
        entry: { tUs: 1, level: 'info', source: 'core', message: 'a' },
      }),
    );
    vi.useFakeTimers();
    ws.drop();
    vi.advanceTimersByTime(500);
    vi.useRealTimers();
    const ws2 = FakeWebSocket.latest();
    ws2.open();
    ws2.receive(
      bin({
        type: 'log',
        id: logs.id,
        entry: { tUs: 1, level: 'info', source: 'core', message: 'a' },
      }),
    );
    expect(core.logs).toHaveLength(1);
  });

  it('loads the system info and connectors', async () => {
    const { core } = setup();
    await vi.waitFor(() => expect(core.system?.version).toBe('0.1.0'));
    await vi.waitFor(() => expect(core.connectors).toEqual([]));
  });

  it('maps core time to wall-clock time using the system info', async () => {
    const { core } = setup();
    await vi.waitFor(() => expect(core.system).not.toBeNull());
    // now() is 1 000 000 ms when the core clock read 5 s, so core time 4 s was one second earlier.
    expect(core.coreTimeToEpochMs(4_000_000)).toBe(999_000);
  });

  it('adds a part and returns it', async () => {
    const { core } = setup();
    f.route({ method: 'POST', path: '/api/v0/parts', status: 201, body: demo });
    const r = await core.addPart({
      name: 'demo',
      connector: 'ldrobot-ld19',
      port: 'replay:demo-ld19',
    });
    expect(r).toEqual({ ok: true, value: demo });
  });

  it('returns the core error when adding fails', async () => {
    const { core } = setup();
    const error = {
      code: 'name-taken',
      message: 'There is already a part called "demo".',
      hint: 'Pick another name.',
    };
    f.route({ method: 'POST', path: '/api/v0/parts', status: 409, body: { error } });
    const r = await core.addPart({
      name: 'demo',
      connector: 'ldrobot-ld19',
      port: 'replay:demo-ld19',
    });
    expect(r).toEqual({ ok: false, error });
  });

  it('removes a part', async () => {
    const { core } = setup();
    f.route({ method: 'DELETE', path: '/api/v0/parts/demo', status: 204 });
    expect(await core.removePart('demo')).toEqual({ ok: true, value: undefined });
  });

  it('watches a signal at a capped rate and stops when asked', () => {
    const { core, ws } = setup();
    const got = vi.fn();
    const stop = core.watchSignal('demo/laser-scan', 15, { onReading: got, onError: () => {} });
    const sub = sentTopics(ws).at(-1)!;
    expect(sub.topic).toEqual({ kind: 'signal', address: 'demo/laser-scan', maxHz: 15 });
    const reading = {
      capability: 'laser-scan@1',
      reading: { tUs: 1, angleMin: 0, angleIncrement: 1, scanTime: 0.1, ranges: [1] },
    };
    ws.receive(bin({ type: 'signal', id: sub.id, address: 'demo/laser-scan', reading }));
    expect(got).toHaveBeenCalledWith(reading);
    stop();
    expect(sentTopics(ws).at(-1)).toEqual({ type: 'unsubscribe', id: sub.id });
  });

  it('keeps errors that belong to nothing as the last error', () => {
    const { core, ws } = setup();
    const error = { code: 'bad', message: 'Something odd.', hint: 'Reload.' };
    ws.receive(bin({ type: 'error', error }));
    expect(core.lastError).toEqual(error);
  });
});
