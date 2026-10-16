import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyTheme, connect, FaberError, type Faber } from './faber-shell';

/**
 * The SDK against a fake shell: a parent window that records what the app posts, and an inbox
 * the test delivers the shell's messages to, as `window.postMessage` would.
 */
class FakeShell {
  readonly sent: { message: Record<string, unknown>; origin: string }[] = [];
  private readonly listeners = new Set<(e: MessageEvent) => void>();
  readonly parent = {
    postMessage: (message: unknown, origin: string) =>
      this.sent.push({ message: message as Record<string, unknown>, origin }),
  };
  readonly self = {
    addEventListener: (_: 'message', fn: (e: MessageEvent) => void) => this.listeners.add(fn),
    removeEventListener: (_: 'message', fn: (e: MessageEvent) => void) => this.listeners.delete(fn),
  };

  /** Delivers a message as if `source` posted it. */
  deliver(data: unknown, source: unknown = this.parent): void {
    for (const l of [...this.listeners]) l({ data, source } as MessageEvent);
  }

  /** The requests of one type the app sent, in order. */
  requests(type: string): Record<string, unknown>[] {
    return this.sent.map((s) => s.message).filter((m) => m.type === type);
  }

  last(type: string): Record<string, unknown> {
    const all = this.requests(type);
    const m = all[all.length - 1];
    if (!m) throw new Error(`no ${type} request`);
    return m;
  }

  ok(type: string, value: unknown): void {
    this.deliver({ faber: 1, re: this.last(type).id, ok: true, value });
  }

  fail(type: string, code: string): void {
    const error = { code, message: `${code} happened`, hint: `fix ${code}` };
    this.deliver({ faber: 1, re: this.last(type).id, ok: false, error });
  }

  get listening(): number {
    return this.listeners.size;
  }
}

const flush = () => new Promise((r) => setTimeout(r, 0));

let shell: FakeShell;
let faber: Faber;

beforeEach(() => {
  shell = new FakeShell();
  faber = connect({ parent: shell.parent, self: shell.self, timeoutMs: 1000 });
});

afterEach(() => {
  faber.close();
  vi.useRealTimers();
});

describe('requests and replies', () => {
  it('posts each request to the shell as protocol v1 and resolves with its reply', async () => {
    const roles = faber.roles();
    expect(shell.sent).toEqual([{ message: { faber: 1, id: 1, type: 'roles' }, origin: '*' }]);
    const value = [
      { role: 'lidar', capability: 'laser-scan@1', command: false, count: '1', parts: 1 },
    ];
    shell.ok('roles', value);
    await expect(roles).resolves.toEqual(value);
  });

  it('rejects with the shell’s error in the house style', async () => {
    const p = faber.latest('nope');
    expect(shell.last('latest')).toMatchObject({ source: { role: 'nope', index: 0 } });
    shell.fail('latest', 'unknown-role');
    const e = await p.catch((e: unknown) => e);
    expect(e).toBeInstanceOf(FaberError);
    expect(e).toMatchObject({
      code: 'unknown-role',
      message: 'unknown-role happened',
      hint: 'fix unknown-role',
    });
  });

  it('ignores messages that aren’t from the shell, or aren’t protocol v1', async () => {
    const p = faber.status();
    const id = shell.last('status').id;
    const status = { enabled: true, state: 'running' };
    shell.deliver({ faber: 1, re: id, ok: true, value: { enabled: false, state: 'stopped' } }, {});
    shell.deliver({ faber: 2, re: id, ok: true, value: { enabled: false, state: 'stopped' } });
    shell.deliver('not an object');
    shell.deliver(null);
    shell.deliver({ faber: 1, re: 999, ok: true, value: 1 });
    shell.deliver({ faber: 1, re: id, ok: true, value: status });
    await expect(p).resolves.toEqual(status);
  });

  it('gives up with no-shell when nothing answers (a page opened outside Faber)', async () => {
    vi.useFakeTimers();
    const p = faber.theme();
    vi.advanceTimersByTime(1001);
    await expect(p).rejects.toMatchObject({ code: 'no-shell' });
  });

  it('asks hello once and asks again after a failure', async () => {
    const a = faber.hello();
    const b = faber.hello();
    expect(shell.requests('hello')).toHaveLength(1);
    shell.fail('hello', 'bad-request');
    await expect(a).rejects.toMatchObject({ code: 'bad-request' });
    await expect(b).rejects.toMatchObject({ code: 'bad-request' });
    faber.hello().catch(() => {});
    expect(shell.requests('hello')).toHaveLength(2);
  });
});

describe('readings', () => {
  const reading = { capability: 'distance@1', reading: { meters: 0.3 } };

  it('delivers a subscription’s readings until it is stopped', async () => {
    const got: unknown[] = [];
    const stop = faber.subscribe('front', (r) => got.push(r), { index: 1, maxHz: 5 });
    expect(shell.last('subscribe')).toMatchObject({
      source: { role: 'front', index: 1 },
      maxHz: 5,
    });
    shell.ok('subscribe', { subscription: 7 });
    await flush();
    shell.deliver({ faber: 1, event: 'signal', subscription: 7, reading });
    shell.deliver({ faber: 1, event: 'signal', subscription: 8, reading });
    expect(got).toEqual([reading]);
    stop();
    expect(shell.last('unsubscribe')).toMatchObject({ subscription: 7 });
    shell.deliver({ faber: 1, event: 'signal', subscription: 7, reading });
    expect(got).toHaveLength(1);
  });

  it('stopped before the shell answered, it unsubscribes as soon as it has an id', async () => {
    const got: unknown[] = [];
    const stop = faber.subscribe('front', (r) => got.push(r));
    stop();
    expect(shell.requests('unsubscribe')).toHaveLength(0);
    shell.ok('subscribe', { subscription: 3 });
    await flush();
    expect(shell.last('unsubscribe')).toMatchObject({ subscription: 3 });
    shell.deliver({ faber: 1, event: 'signal', subscription: 3, reading });
    expect(got).toEqual([]);
  });

  it('says when a subscription can’t start or ends', async () => {
    const errors: FaberError[] = [];
    faber.subscribe('base', () => {}, { onError: (e) => errors.push(e) });
    shell.fail('subscribe', 'no-part');
    faber.subscribe('front', () => {}, { onError: (e) => errors.push(e) });
    shell.ok('subscribe', { subscription: 4 });
    await flush();
    const error = { code: 'signal-ended', message: 'gone', hint: 'wait' };
    shell.deliver({ faber: 1, event: 'signal-ended', subscription: 4, error });
    expect(errors.map((e) => e.code)).toEqual(['no-part', 'signal-ended']);
  });

  it('reads the app’s own derived signals by name', async () => {
    const p = faber.published('wall-distance');
    expect(shell.last('latest')).toMatchObject({ source: { published: 'wall-distance' } });
    shell.ok('latest', reading);
    await expect(p).resolves.toEqual(reading);
    faber.subscribePublished('wall-distance', () => {});
    expect(shell.last('subscribe')).toMatchObject({ source: { published: 'wall-distance' } });
  });
});

describe('driving', () => {
  const go = {
    capability: 'planar-base@1',
    command: { kind: 'set-velocity', vx: 0.2, vy: 0, omega: 0 },
  };

  async function driving() {
    const p = faber.drive('base');
    expect(shell.last('drive')).toMatchObject({ role: 'base', index: 0 });
    shell.ok('drive', null);
    return p;
  }

  it('keeps the lease alive with its own heartbeat while the page runs, and stops on release', async () => {
    vi.useFakeTimers();
    const base = await driving();
    vi.advanceTimersByTime(350);
    expect(shell.requests('renew').length).toBe(3);
    const sent = base.command(go);
    expect(shell.last('command')).toMatchObject({ role: 'base', index: 0, command: go });
    shell.ok('command', { applied: go, clamped: false });
    await expect(sent).resolves.toEqual({ applied: go, clamped: false });
    const renews = shell.requests('renew').length;
    vi.advanceTimersByTime(50);
    expect(shell.requests('renew').length).toBe(renews);
    const released = base.release();
    shell.ok('release', null);
    await released;
    vi.advanceTimersByTime(1000);
    expect(shell.requests('renew').length).toBe(renews);
    expect(base.ended).toBe(true);
  });

  it('says when the lease ends on its own, and refuses commands after', async () => {
    const base = await driving();
    const ends: string[] = [];
    base.onEnd((e) => ends.push(e.code));
    const error = { code: 'lease-ended', message: 'The e-stop', hint: 'Release it' };
    shell.deliver({
      faber: 1,
      event: 'lease-ended',
      role: 'base',
      index: 0,
      reason: 'estop',
      error,
    });
    expect(ends).toEqual(['lease-ended']);
    expect(base.ended).toBe(true);
    await expect(base.command(go)).rejects.toMatchObject({ code: 'lease-ended' });
    // Driving again asks the shell again.
    faber.drive('base').catch(() => {});
    expect(shell.requests('drive')).toHaveLength(2);
  });

  it('a refused drive rejects (the user said no, or someone else drives)', async () => {
    const p = faber.drive('base');
    shell.fail('drive', 'not-confirmed');
    await expect(p).rejects.toMatchObject({ code: 'not-confirmed' });
  });
});

describe('status and theme', () => {
  it('passes the shell’s status and theme events on until stopped', () => {
    const seen: unknown[] = [];
    const stopStatus = faber.onStatus((s) => seen.push(s));
    faber.onTheme((t) => seen.push(t));
    const status = { enabled: false, state: 'stopped' };
    const theme = { scheme: 'dark', tokens: { '--surface': 'black' } };
    shell.deliver({ faber: 1, event: 'status', status });
    shell.deliver({ faber: 1, event: 'theme', theme });
    stopStatus();
    shell.deliver({ faber: 1, event: 'status', status });
    expect(seen).toEqual([status, theme]);
  });

  it('applyTheme sets the tokens and the colour scheme, and nothing that isn’t a token', () => {
    const set: [string, string][] = [];
    const root = { style: { setProperty: (n: string, v: string) => void set.push([n, v]) } };
    applyTheme(
      { scheme: 'dark', tokens: { '--surface': 'black', color: 'red', '--x;y': 'z' } },
      root,
    );
    expect(set).toEqual([
      ['--surface', 'black'],
      ['color-scheme', 'dark'],
    ]);
  });

  it('close stops listening and fails what is still waiting', async () => {
    const p = faber.roles();
    faber.close();
    expect(shell.listening).toBe(0);
    await expect(p).rejects.toMatchObject({ code: 'no-shell' });
    await expect(faber.roles()).rejects.toMatchObject({ code: 'no-shell' });
  });
});

describe('state', () => {
  it('gets, sets, removes and lists values through the shell', async () => {
    const got = faber.state.get('speed');
    expect(shell.last('state-get')).toMatchObject({ faber: 1, key: 'speed' });
    shell.ok('state-get', '0.3');
    await expect(got).resolves.toBe('0.3');

    const set = faber.state.set('speed', '0.4');
    expect(shell.last('state-set')).toMatchObject({ key: 'speed', value: '0.4' });
    shell.ok('state-set', null);
    await expect(set).resolves.toBeUndefined();

    const removed = faber.state.remove('speed');
    expect(shell.last('state-set')).toMatchObject({ key: 'speed', value: null });
    shell.ok('state-set', null);
    await removed;

    const keys = faber.state.keys();
    shell.ok('state-keys', ['laps']);
    await expect(keys).resolves.toEqual(['laps']);
  });

  it('says why a value was refused', async () => {
    const set = faber.state.set('big', 'x');
    shell.fail('state-set', 'state-full');
    await expect(set).rejects.toMatchObject({ code: 'state-full', hint: 'fix state-full' });
  });

  it('asks for changes once, and hands every change to every watcher until it stops', async () => {
    const a: unknown[] = [];
    const b: unknown[] = [];
    const stopA = faber.state.onChange((key, value) => a.push([key, value]));
    faber.state.onChange((key, value) => b.push([key, value]));
    expect(shell.requests('state-watch')).toHaveLength(1);
    shell.ok('state-watch', null);
    await flush();
    shell.deliver({ faber: 1, event: 'state', key: 'speed', value: '0.2' });
    stopA();
    shell.deliver({ faber: 1, event: 'state', key: 'speed', value: null });
    expect(a).toEqual([['speed', '0.2']]);
    expect(b).toEqual([
      ['speed', '0.2'],
      ['speed', null],
    ]);
  });

  it('asks again for changes after the shell refused', async () => {
    faber.state.onChange(() => undefined);
    shell.fail('state-watch', 'app-gone');
    await flush();
    faber.state.onChange(() => undefined);
    expect(shell.requests('state-watch')).toHaveLength(2);
  });
});
