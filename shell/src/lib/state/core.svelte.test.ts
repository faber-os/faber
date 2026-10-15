import { beforeEach, describe, expect, it, vi } from 'vitest';
import { decode, encode } from '@msgpack/msgpack';
import { CoreSocket } from '../api/socket';
import { createHttp } from '../api/http';
import { FakeWebSocket } from '../../test/fakeWebSocket';
import { fakeFetch, type FakeFetch } from '../../test/fakeFetch';
import { CoreState, LOG_CAPACITY } from './core.svelte';
import type { PartInfo, Settings } from './types';

const demo: PartInfo = {
  name: 'demo',
  connector: 'ldrobot-ld19',
  port: 'replay:ld19-sample',
  state: { kind: 'running' },
  signals: [{ address: 'demo/laser-scan', capability: 'laser-scan@1' }],
  mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
};

function bin(msg: unknown): ArrayBuffer {
  const b = encode(msg);
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
}

const SETTINGS: Settings = {
  appearance: { theme: 'system', style: 'aluminium', wallpaper: 'aluminium' },
  accessibility: { reduceMotion: 'system', higherContrast: 'system', textSize: '100' },
  units: { length: 'metric' },
  time: { timeZone: '', clock: '24h' },
  desktop: { icons: [] },
  taskbar: { pinned: ['tool:parts'], size: 'small' },
  developer: { fullMachineFiles: false },
};

let f: FakeFetch;

const SYSTEM = {
  name: 'Faber',
  version: '0.1.0',
  api: FABER_API,
  timeUs: 5_000_000,
  timeZone: 'Europe/Copenhagen',
};

function setup(system: object = SYSTEM) {
  f = fakeFetch([
    { path: '/api/v0/system', body: system },
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
  it('accepts a core built from the same API', async () => {
    const { core } = setup();
    await vi.waitFor(() => expect(core.system).not.toBeNull());
    expect(core.apiMismatch).toBe(false);
  });

  it('notices a core built from a different API, or one too old to say', async () => {
    for (const system of [
      { ...SYSTEM, api: '0000000000000000' },
      { name: 'Faber', timeUs: 1 },
    ]) {
      FakeWebSocket.reset();
      const { core } = setup(system);
      await vi.waitFor(() => expect(core.system).not.toBeNull());
      expect(core.apiMismatch).toBe(true);
    }
  });

  it('asks the core about itself again on every connection, as it may have been restarted', async () => {
    vi.useFakeTimers();
    try {
      const { ws } = setup();
      ws.drop();
      vi.advanceTimersByTime(10_000);
      FakeWebSocket.latest().open();
      await vi.waitFor(() =>
        expect(f.calls.filter((c) => c.path === '/api/v0/system')).toHaveLength(2),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows wall-clock times in the machine's zone unless the settings pick one", async () => {
    const { core, ws } = setup();
    expect(core.wallClock).toEqual({ timeZone: undefined, clock: '24h' });
    await vi.waitFor(() => expect(core.wallClock.timeZone).toBe('Europe/Copenhagen'));
    const id = sentTopics(ws).find((m) => m.topic?.kind === 'settings')!.id;
    const push = (time: Settings['time']) =>
      ws.receive(bin({ type: 'settings', id, settings: { ...SETTINGS, time } }));
    push({ timeZone: 'America/New_York', clock: '12h' });
    expect(core.wallClock).toEqual({ timeZone: 'America/New_York', clock: '12h' });
    push({ timeZone: '', clock: '24h' });
    expect(core.wallClock).toEqual({ timeZone: 'Europe/Copenhagen', clock: '24h' });
  });

  it('loads about and storage, and makes and restores backups', async () => {
    const { core } = setup();
    const about = {
      version: '0.1.0',
      api: 'x',
      dataFolder: '/data',
      uptimeS: 5,
      os: 'linux',
      timeZone: 'UTC',
    };
    const storage = {
      recordingsBytes: 1,
      documentsBytes: 2,
      desktopBytes: 3,
      backupsBytes: 4,
      freeBytes: 5,
      totalBytes: 20,
    };
    f.route({ path: '/api/v0/about', body: about });
    f.route({ path: '/api/v0/storage', body: storage });
    f.route({
      method: 'POST',
      path: '/api/v0/backups',
      status: 201,
      body: { path: 'home:/Backups/b.zip', bytes: 9 },
    });
    f.route({ method: 'POST', path: '/api/v0/backups/restore', status: 204 });
    expect(await core.loadAbout()).toEqual({ ok: true, value: about });
    expect(await core.loadStorage()).toEqual({ ok: true, value: storage });
    expect(await core.createBackup(true)).toEqual({
      ok: true,
      value: { path: 'home:/Backups/b.zip', bytes: 9 },
    });
    expect(f.calls.at(-1)?.body).toEqual({ includeRecordings: true });
    expect((await core.restoreBackup('home:/Backups/b.zip')).ok).toBe(true);
    expect(f.calls.at(-1)?.body).toEqual({ path: 'home:/Backups/b.zip' });
  });

  it('resets the settings and keeps what the core answered', async () => {
    const { core } = setup();
    f.route({ method: 'POST', path: '/api/v0/settings/reset', body: SETTINGS });
    const r = await core.resetSettings();
    expect(r.ok).toBe(true);
    expect(core.settings).toEqual(SETTINGS);
  });

  it('tracks the connection status', () => {
    const { core, ws } = setup();
    expect(core.connection).toBe('connected');
    ws.drop();
    expect(core.connection).toBe('reconnecting');
  });

  it('subscribes to parts, ports, logs, settings and files on start', () => {
    const { ws } = setup();
    expect(sentTopics(ws).map((m) => m.topic?.kind)).toEqual([
      'parts',
      'ports',
      'logs',
      'settings',
      'files',
    ]);
  });

  it('keeps the settings from their topic', () => {
    const { core, ws } = setup();
    expect(core.settings).toBeNull();
    const sub = sentTopics(ws).find((m) => m.topic?.kind === 'settings')!;
    ws.receive(bin({ type: 'settings', id: sub.id, settings: SETTINGS }));
    expect(core.settings).toEqual(SETTINGS);
  });

  it('saves settings and keeps what the core answered', async () => {
    const { core } = setup();
    const answer = {
      ...SETTINGS,
      appearance: { theme: 'dark', style: 'cobalt', wallpaper: 'graphite' },
    };
    f.route({ method: 'PUT', path: '/api/v0/settings', body: answer });
    const r = await core.saveSettings(answer as Settings);
    expect(r.ok).toBe(true);
    expect(core.settings).toEqual(answer);
  });

  it('tells file watchers which folders changed', () => {
    const { core, ws } = setup();
    const a = vi.fn();
    const b = vi.fn();
    const stopA = core.watchFiles(a);
    core.watchFiles(b);
    const sub = sentTopics(ws).find((m) => m.topic?.kind === 'files')!;
    ws.receive(bin({ type: 'files-changed', id: sub.id, folders: ['home:/Desktop'] }));
    expect(a).toHaveBeenCalledWith(['home:/Desktop']);
    stopA();
    ws.receive(bin({ type: 'files-changed', id: sub.id, folders: ['home:/Trash'] }));
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledWith(['home:/Trash']);
  });

  it('wraps file requests in results with the core error', async () => {
    const { core } = setup();
    const error = {
      code: 'protected',
      message: 'Desktop is a fixed folder.',
      hint: 'Pick another.',
    };
    f.route({ method: 'POST', path: '/api/v0/files/trash', status: 409, body: { error } });
    expect(await core.trash(['home:/Desktop'])).toEqual({ ok: false, error });
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
      port: 'replay:ld19-sample',
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
      port: 'replay:ld19-sample',
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
