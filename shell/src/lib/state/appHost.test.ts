import { describe, expect, it, vi } from 'vitest';
import { part, simBase, simDistance } from '../../test/fakeShell';
import {
  AppHost,
  MAX_STATE_BYTES,
  MAX_SUBSCRIPTIONS,
  appHolder,
  type AppHostDeps,
  type AppStateClient,
} from './appHost';
import type { LeaseClient } from './driving';
import type { Result } from './result';
import type { AppInfo, ErrorBody, LeaseEndReason, LeaseInfo, Reading } from './types';

const APP: AppInfo = {
  id: 'wall-follower',
  name: 'Wall follower',
  version: '0.1.0',
  enabled: true,
  state: { kind: 'running' },
  logic: true,
  tickHz: 20,
  roles: [
    { role: 'scanner', capability: 'laser-scan@1', command: false, count: '1', parts: ['lidar1'] },
    { role: 'wheels', capability: 'planar-base@1', command: true, count: '1', parts: ['base1'] },
    {
      role: 'ring',
      capability: 'distance@1',
      command: false,
      count: '1..',
      parts: ['front', 'back'],
    },
  ],
  publishes: [
    {
      name: 'wall-distance',
      capability: 'distance@1',
      address: 'app:wall-follower/wall-distance',
    },
  ],
  ui: '/apps/wall-follower/ui/index.html',
};

const GO = {
  capability: 'planar-base@1',
  command: { kind: 'set-velocity', vx: 0.2, vy: 0, omega: 0 },
};
const ok = <T>(value: T): Result<T> => ({ ok: true, value });
const flush = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

function setup(options: { confirm?: boolean } = {}) {
  const frame = { name: 'the app frame' };
  const posted: Record<string, unknown>[] = [];
  let app: AppInfo | null = structuredClone(APP);
  const parts = [part('lidar1'), simBase('base1'), simDistance('front'), simDistance('back')];
  const watches: {
    address: string;
    hz: number;
    handlers: { onReading(r: Reading): void; onError(e: ErrorBody): void };
    stopped: boolean;
  }[] = [];
  const core: string[] = [];
  const leaseEnds = new Set<(l: LeaseInfo, r: LeaseEndReason) => void>();
  const answers: { request: Result<unknown>; renew: Result<unknown> } = {
    request: ok({
      lease: 'L1',
      part: 'base1',
      holder: 'app:wall-follower',
      deadlineUs: 5,
      durationMs: 250,
      locks: [],
    }),
    renew: ok({
      lease: 'L1',
      part: 'base1',
      holder: 'x',
      deadlineUs: 6,
      durationMs: 250,
      locks: [],
    }),
  };
  const leases: LeaseClient = {
    requestLease: async (p, holder) => (
      core.push(`lease ${p} ${holder}`),
      answers.request as never
    ),
    renewLease: async (p) => (core.push(`renew ${p}`), answers.renew as never),
    releaseLease: async (p) => (core.push(`release ${p}`), ok(undefined)),
    command: async (p, _lease, c) => (
      core.push(`command ${p}`),
      ok({ applied: c, clamped: false, deadlineUs: 9 })
    ),
  };
  const confirm = vi.fn(async () => options.confirm ?? true);
  /** The core's side of the app's state: what it holds, and the host's live watch on it. */
  const stored = new Map<string, string>([['speed', '0.25']]);
  let stateHandlers: Parameters<AppStateClient['watch']>[0] | null = null;
  const appState: AppStateClient = {
    watch: (handlers) => {
      core.push('state watch');
      stateHandlers = handlers;
      queueMicrotask(() => handlers.onValues(Object.fromEntries(stored)));
      return () => {
        core.push('state unwatch');
        stateHandlers = null;
      };
    },
    set: async (key, value) => {
      core.push(`state set ${key}`);
      if (value === null) stored.delete(key);
      else stored.set(key, value);
      stateHandlers?.onChange(key, value);
      return ok({});
    },
  };
  const deps: AppHostDeps = {
    appId: 'wall-follower',
    app: () => app,
    parts: () => parts,
    frame: () => frame,
    post: (m) => void posted.push(m as Record<string, unknown>),
    readSignal: async (address) => {
      core.push(`read ${address}`);
      return ok({
        address,
        capability: 'distance@1',
        reading: { capability: 'distance@1', reading: { tUs: 1, meters: 0.3, status: 'ok' } },
      });
    },
    watchSignal: (address, hz, handlers) => {
      const w = { address, hz, handlers, stopped: false };
      watches.push(w);
      core.push(`watch ${address} ${hz}`);
      return () => void (w.stopped = true);
    },
    leases,
    watchLeaseEnds: (fn) => (leaseEnds.add(fn), () => void leaseEnds.delete(fn)),
    confirmDriving: confirm,
    theme: () => ({ scheme: 'dark', tokens: { '--surface': 'black' } }),
    appState,
  };
  const host = new AppHost(deps);
  let nextId = 1;
  /** Sends a request from the frame, as the SDK would. Returns its id. */
  function send(
    request: Record<string, unknown>,
    from: { source?: unknown; origin?: string } = {},
  ) {
    const id = nextId++;
    host.receive({
      source: 'source' in from ? from.source : frame,
      origin: from.origin ?? 'null',
      data: { faber: 1, id, ...request },
    });
    return id;
  }
  async function ask(request: Record<string, unknown>) {
    const id = send(request);
    await flush();
    const reply = posted.find((m) => m.re === id);
    if (!reply) throw new Error(`no reply to ${JSON.stringify(request)}`);
    return reply as { ok: boolean; value?: unknown; error?: ErrorBody };
  }
  return {
    host,
    frame,
    posted,
    watches,
    core,
    confirm,
    answers,
    send,
    ask,
    setApp: (a: AppInfo | null) => {
      app = a;
      host.appChanged();
    },
    leaseEnded: (l: LeaseInfo, reason: LeaseEndReason) => leaseEnds.forEach((fn) => fn(l, reason)),
    stored,
    /** A change made elsewhere: by the app's logic, or another window. */
    stateChangedElsewhere: (key: string, value: string | null) => {
      if (value === null) stored.delete(key);
      else stored.set(key, value);
      stateHandlers?.onChange(key, value);
    },
    stateError: (e: ErrorBody) => stateHandlers?.onError(e),
    events: (name: string) => posted.filter((m) => m.event === name),
  };
}

describe('the app window host: hostile messages', () => {
  it('ignores messages from any other origin, even the shell’s own', async () => {
    const s = setup();
    for (const origin of ['https://evil.example', 'http://127.0.0.1:7878', '', 'NULL'])
      s.send({ type: 'hello' }, { origin });
    await flush();
    expect(s.posted).toEqual([]);
    expect(s.host.ignored).toBe(4);
  });

  it('ignores messages from any other window, with the right origin too', async () => {
    const s = setup();
    for (const source of [{}, null, undefined, globalThis]) s.send({ type: 'hello' }, { source });
    await flush();
    expect(s.posted).toEqual([]);
    expect(s.host.ignored).toBe(4);
  });

  it('ignores what isn’t a message at all, and refuses what it can’t read', async () => {
    const s = setup();
    for (const data of ['hello', 42, null, [], { faber: 1, type: 'hello' }, { faber: 1, id: -1 }])
      s.host.receive({ source: s.frame, origin: 'null', data });
    await flush();
    expect(s.posted).toEqual([]);
    const cases: [Record<string, unknown>, string][] = [
      [{ type: 'doSomethingEvil' }, 'unknown-request'],
      [{ type: '__proto__' }, 'unknown-request'],
      [{ type: 7 }, 'bad-request'],
      [{ type: 'latest' }, 'bad-request'],
      [{ type: 'latest', source: { role: 'scanner', index: -1 } }, 'bad-request'],
      [{ type: 'latest', source: { role: 'scanner', index: 0.5 } }, 'bad-request'],
      [{ type: 'latest', source: { role: 'scanner', published: 'x' } }, 'bad-request'],
      [{ type: 'latest', source: { address: 'base1/planar-base' } }, 'bad-request'],
      [{ type: 'subscribe', source: { role: 'scanner' }, maxHz: 'fast' }, 'bad-request'],
      [{ type: 'subscribe', source: { role: 'scanner' }, maxHz: -5 }, 'bad-request'],
      [{ type: 'unsubscribe', subscription: 'all' }, 'bad-request'],
    ];
    for (const [request, code] of cases) {
      const r = await s.ask(request);
      expect(r, JSON.stringify(request)).toMatchObject({ ok: false, error: { code } });
      expect(r.error!.message).not.toBe('');
      expect(r.error!.hint).not.toBe('');
    }
    expect(s.core).toEqual([]);
  });

  it('answers another protocol version with unsupported-version', async () => {
    const s = setup();
    s.host.receive({ source: s.frame, origin: 'null', data: { faber: 2, id: 1, type: 'hello' } });
    await flush();
    expect(s.posted).toEqual([
      {
        faber: 1,
        re: 1,
        ok: false,
        error: expect.objectContaining({ code: 'unsupported-version' }),
      },
    ]);
  });

  it('refuses a role the app doesn’t have, and never reaches the core for it', async () => {
    const s = setup();
    for (const request of [
      { type: 'latest', source: { role: 'camera' } },
      { type: 'subscribe', source: { role: 'camera' } },
      { type: 'drive', role: 'camera' },
      { type: 'command', role: 'camera', command: GO },
      { type: 'latest', source: { published: 'pose' } },
    ]) {
      const r = await s.ask(request);
      expect(r.error?.code, JSON.stringify(request)).toMatch(/^unknown-(role|signal)$/);
    }
    expect(s.core).toEqual([]);
    expect(s.confirm).not.toHaveBeenCalled();
  });

  it('refuses to drive or command a role without command = true', async () => {
    const s = setup();
    expect((await s.ask({ type: 'drive', role: 'scanner' })).error?.code).toBe('not-commandable');
    const cmd = await s.ask({ type: 'command', role: 'ring', index: 1, command: GO });
    expect(cmd.error?.code).toBe('not-commandable');
    expect((await s.ask({ type: 'renew', role: 'scanner' })).error?.code).toBe('not-commandable');
    expect(s.core).toEqual([]);
    expect(s.confirm).not.toHaveBeenCalled();
  });

  it('refuses a command of another capability, or one without driving first', async () => {
    const s = setup();
    const motor = { capability: 'motor@1', command: { kind: 'set-duty', duty: 1 } };
    for (const command of [motor, 'go', { capability: 'planar-base@1' }, null])
      expect((await s.ask({ type: 'command', role: 'wheels', command })).error?.code).toBe(
        'bad-command',
      );
    expect((await s.ask({ type: 'command', role: 'wheels', command: GO })).error?.code).toBe(
      'lease-required',
    );
    expect(s.core).toEqual([]);
  });

  it('slows down a page that floods it', async () => {
    const s = setup();
    for (let i = 0; i < 201; i++) s.send({ type: 'theme' });
    await flush();
    const refused = s.posted.filter((m) => m.ok === false);
    expect(refused).toHaveLength(1);
    expect(refused[0]).toMatchObject({ re: 201, error: { code: 'too-many-requests' } });
  });
});

describe('the app window host: what an app may do', () => {
  it('says hello with the app’s roles but never which parts fill them', async () => {
    const s = setup();
    const r = await s.ask({ type: 'hello' });
    expect(r.value).toEqual({
      app: { id: 'wall-follower', name: 'Wall follower', version: '0.1.0' },
      roles: [
        { role: 'scanner', capability: 'laser-scan@1', command: false, count: '1', parts: 1 },
        { role: 'wheels', capability: 'planar-base@1', command: true, count: '1', parts: 1 },
        { role: 'ring', capability: 'distance@1', command: false, count: '1..', parts: 2 },
      ],
      published: [{ name: 'wall-distance', capability: 'distance@1' }],
      status: { enabled: true, state: 'running' },
      theme: { scheme: 'dark', tokens: { '--surface': 'black' } },
    });
    for (const name of ['lidar1', 'base1', 'front', 'back'])
      expect(JSON.stringify(r.value)).not.toContain(name);
  });

  it('reads a role’s part and the app’s own derived signals', async () => {
    const s = setup();
    const r = await s.ask({ type: 'latest', source: { role: 'ring', index: 1 } });
    expect(r.value).toMatchObject({ capability: 'distance@1' });
    await s.ask({ type: 'latest', source: { published: 'wall-distance' } });
    expect(s.core).toEqual(['read back/distance', 'read app:wall-follower/wall-distance']);
    const none = await s.ask({ type: 'latest', source: { role: 'ring', index: 2 } });
    expect(none.error?.code).toBe('no-part');
  });

  it('follows a signal, throttled, until unsubscribed or the signal ends', async () => {
    const s = setup();
    const a = await s.ask({ type: 'subscribe', source: { role: 'scanner' } });
    const b = await s.ask({
      type: 'subscribe',
      source: { published: 'wall-distance' },
      maxHz: 500,
    });
    expect(s.core).toEqual([
      'watch lidar1/laser-scan 10',
      'watch app:wall-follower/wall-distance 20',
    ]);
    const sub = (a.value as { subscription: number }).subscription;
    const reading = { capability: 'laser-scan@1', reading: { tUs: 1, points: [] } } as never;
    s.watches[0]!.handlers.onReading(reading);
    expect(s.events('signal')).toEqual([{ faber: 1, event: 'signal', subscription: sub, reading }]);
    await s.ask({ type: 'unsubscribe', subscription: sub });
    expect(s.watches[0]!.stopped).toBe(true);
    s.watches[1]!.handlers.onError({ code: 'signal-ended', message: 'gone', hint: 'wait' });
    expect(s.events('signal-ended')).toMatchObject([
      { subscription: (b.value as { subscription: number }).subscription },
    ]);
    expect(s.host.subscriptions).toBe(0);
  });

  it('holds at most 16 subscriptions per window', async () => {
    const s = setup();
    for (let i = 0; i < MAX_SUBSCRIPTIONS; i++)
      expect((await s.ask({ type: 'subscribe', source: { role: 'scanner' } })).ok).toBe(true);
    const r = await s.ask({ type: 'subscribe', source: { role: 'scanner' } });
    expect(r.error?.code).toBe('too-many-subscriptions');
  });

  it('drives only after the user’s go-ahead (D46), as app:<id>, kept alive only by the page', async () => {
    vi.useFakeTimers();
    try {
      const s = setup();
      const drive = s.send({ type: 'drive', role: 'wheels' });
      await vi.advanceTimersByTimeAsync(0);
      expect(s.confirm).toHaveBeenCalledWith('base1');
      expect(s.posted.find((m) => m.re === drive)).toMatchObject({ ok: true });
      expect(s.core).toEqual([`lease base1 ${appHolder('wall-follower')}`]);
      // The host doesn't keep the lease alive on its own: a page that goes quiet lets it lapse.
      await vi.advanceTimersByTimeAsync(1000);
      expect(s.core).toHaveLength(1);
      s.send({ type: 'renew', role: 'wheels' });
      const c = s.send({ type: 'command', role: 'wheels', command: GO });
      await vi.advanceTimersByTimeAsync(0);
      expect(s.core.slice(1)).toEqual(['renew base1', 'command base1']);
      expect(s.posted.find((m) => m.re === c)).toMatchObject({
        ok: true,
        value: { applied: GO, clamped: false },
      });
      s.send({ type: 'release', role: 'wheels' });
      await vi.advanceTimersByTimeAsync(0);
      expect(s.core.at(-1)).toBe('release base1');
      expect(s.host.leases).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('refuses to drive when the user says no, or the app is off', async () => {
    const s = setup({ confirm: false });
    expect((await s.ask({ type: 'drive', role: 'wheels' })).error?.code).toBe('not-confirmed');
    expect(s.core).toEqual([]);
    s.setApp({ ...APP, enabled: false, state: { kind: 'stopped' } });
    expect((await s.ask({ type: 'drive', role: 'wheels' })).error?.code).toBe('app-off');
    expect(s.events('status')).toEqual([
      { faber: 1, event: 'status', status: { enabled: false, state: 'stopped' } },
    ]);
  });

  it('passes the core’s refusal on (someone else drives)', async () => {
    const s = setup();
    s.answers.request = {
      ok: false,
      error: { code: 'lease-held', message: '"Drive window" is driving "base1".', hint: 'Wait.' },
    };
    expect((await s.ask({ type: 'drive', role: 'wheels' })).error?.code).toBe('lease-held');
    expect(s.host.leases).toBe(0);
  });

  it('tells the page when its lease ends on its own (the e-stop)', async () => {
    const s = setup();
    await s.ask({ type: 'drive', role: 'wheels' });
    s.leaseEnded({ part: 'base1', holder: 'app:wall-follower', deadlineUs: 5, locks: [] }, 'estop');
    expect(s.events('lease-ended')).toMatchObject([{ role: 'wheels', index: 0, reason: 'estop' }]);
    expect(s.host.leases).toBe(0);
    expect((await s.ask({ type: 'command', role: 'wheels', command: GO })).error?.code).toBe(
      'lease-required',
    );
  });

  it('ends the leases when the app is turned off or its part changes', async () => {
    const s = setup();
    await s.ask({ type: 'drive', role: 'wheels' });
    s.setApp({ ...APP, enabled: false, state: { kind: 'stopped' } });
    await flush();
    expect(s.core.at(-1)).toBe('release base1');
    expect(s.events('lease-ended')).toMatchObject([{ role: 'wheels', error: { code: 'app-off' } }]);

    const t = setup();
    await t.ask({ type: 'drive', role: 'wheels' });
    await t.ask({ type: 'subscribe', source: { role: 'scanner' } });
    const moved = structuredClone(APP);
    moved.roles[0]!.parts = ['other'];
    moved.roles[1]!.parts = ['other'];
    t.setApp(moved);
    await flush();
    expect(t.core.at(-1)).toBe('release base1');
    expect(t.events('signal-ended')).toHaveLength(1);
    expect(t.watches[0]!.stopped).toBe(true);
  });

  it('a new page in the frame, or closing the window, ends everything the old page had', async () => {
    const s = setup();
    await s.ask({ type: 'drive', role: 'wheels' });
    await s.ask({ type: 'subscribe', source: { role: 'scanner' } });
    const late = s.send({ type: 'latest', source: { role: 'scanner' } });
    s.host.reset();
    await flush();
    expect(s.core).toContain('release base1');
    expect(s.watches[0]!.stopped).toBe(true);
    expect(s.posted.find((m) => m.re === late)).toBeUndefined();

    const t = setup();
    await t.ask({ type: 'drive', role: 'wheels' });
    await t.host.dispose();
    expect(t.core.at(-1)).toBe('release base1');
    t.send({ type: 'hello' });
    await flush();
    expect(t.posted.filter((m) => m.re !== undefined)).toHaveLength(1);
  });

  it('pushes the theme when the shell’s look changes', () => {
    const s = setup();
    s.host.themeChanged();
    expect(s.events('theme')).toEqual([
      { faber: 1, event: 'theme', theme: { scheme: 'dark', tokens: { '--surface': 'black' } } },
    ]);
  });

  it('refuses everything once the app is uninstalled', async () => {
    const s = setup();
    s.setApp(null);
    expect((await s.ask({ type: 'roles' })).error?.code).toBe('app-gone');
  });
});

describe('the app window host: state', () => {
  it('reads, sets, lists and removes the state the app shares with its logic', async () => {
    const s = setup();
    expect(await s.ask({ type: 'state-get', key: 'speed' })).toMatchObject({
      ok: true,
      value: '0.25',
    });
    expect(await s.ask({ type: 'state-get', key: 'laps' })).toMatchObject({
      ok: true,
      value: null,
    });
    expect(await s.ask({ type: 'state-set', key: 'speed', value: '0.4' })).toMatchObject({
      ok: true,
    });
    expect(s.stored.get('speed')).toBe('0.4');
    expect(await s.ask({ type: 'state-get', key: 'speed' })).toMatchObject({ value: '0.4' });
    await s.ask({ type: 'state-set', key: 'laps', value: '2' });
    expect(await s.ask({ type: 'state-keys' })).toMatchObject({ value: ['laps', 'speed'] });
    await s.ask({ type: 'state-set', key: 'laps', value: null });
    expect(s.stored.has('laps')).toBe(false);
    expect(s.core.filter((c) => c === 'state watch')).toHaveLength(1);
  });

  it('refuses bad keys and values, and anything past 64 KB, before the core hears of it', async () => {
    const s = setup();
    for (const [request, code] of [
      [{ type: 'state-get', key: 42 }, 'bad-request'],
      [{ type: 'state-get', key: '' }, 'bad-key'],
      [{ type: 'state-set', key: 'k'.repeat(257), value: '1' }, 'bad-key'],
      [{ type: 'state-set', key: 'æ'.repeat(129), value: '1' }, 'bad-key'],
      [{ type: 'state-set', key: 'speed', value: 0.3 }, 'bad-request'],
      [{ type: 'state-set', key: 'big', value: 'x'.repeat(MAX_STATE_BYTES) }, 'state-full'],
    ] as const) {
      const r = await s.ask(request);
      expect(r.error?.code, JSON.stringify(request).slice(0, 60)).toBe(code);
      expect(r.error?.hint).not.toBe('');
    }
    expect(s.core.filter((c) => c.startsWith('state set'))).toEqual([]);
    const fits = 'x'.repeat(MAX_STATE_BYTES - 'big'.length - 'speed0.25'.length);
    expect(await s.ask({ type: 'state-set', key: 'big', value: fits })).toMatchObject({ ok: true });
  });

  it('tells a page that asked about every change, whoever made it, until a new page loads', async () => {
    const s = setup();
    await s.ask({ type: 'state-get', key: 'speed' });
    s.stateChangedElsewhere('speed', '0.1');
    expect(s.events('state'), 'not asked for yet').toEqual([]);
    expect(await s.ask({ type: 'state-get', key: 'speed' })).toMatchObject({ value: '0.1' });
    await s.ask({ type: 'state-watch' });
    s.stateChangedElsewhere('speed', '0.2');
    await s.ask({ type: 'state-set', key: 'laps', value: '1' });
    s.stateChangedElsewhere('laps', null);
    expect(s.events('state')).toEqual([
      { faber: 1, event: 'state', key: 'speed', value: '0.2' },
      { faber: 1, event: 'state', key: 'laps', value: '1' },
      { faber: 1, event: 'state', key: 'laps', value: null },
    ]);
    s.host.reset();
    s.stateChangedElsewhere('speed', '0.3');
    expect(s.events('state')).toHaveLength(3);
    await s.host.dispose();
    expect(s.core.at(-1)).toBe('state unwatch');
  });

  it('passes on why the state can’t be read, and tries again next time', async () => {
    const s = setup();
    const first = s.send({ type: 'state-get', key: 'speed' });
    s.stateError({ code: 'unknown-app', message: 'gone', hint: 'reinstall' });
    await flush();
    expect(s.posted.find((m) => m.re === first)).toMatchObject({
      ok: false,
      error: { code: 'unknown-app' },
    });
    expect(await s.ask({ type: 'state-get', key: 'speed' })).toMatchObject({ value: '0.25' });
    expect(s.core.filter((c) => c === 'state watch')).toHaveLength(2);
  });
});
