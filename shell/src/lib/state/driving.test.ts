import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Result } from './core.svelte';
import {
  DriveLoop,
  LeaseKeeper,
  baseCommand,
  canMove,
  isStill,
  leaseEndText,
  type LeaseClient,
  type LeaseStatus,
} from './driving';
import type { Command, CommandResult, ErrorBody, LeaseGrant, PartInfo } from './types';

const grant = (deadlineUs = 1_000): LeaseGrant => ({
  lease: 'L1',
  part: 'base',
  holder: 'Drive window',
  deadlineUs,
  durationMs: 250,
  locks: [],
});

const ok = <T>(value: T): Result<T> => ({ ok: true, value });
const fail = (code: string, message = code): Result<never> => ({
  ok: false,
  error: { code, message, hint: 'try again' },
});

/** A lease client that answers at once; tests change the answers as they go. */
function fakeClient() {
  const calls: string[] = [];
  const commands: Command[] = [];
  const answers = {
    request: ok(grant()) as Result<LeaseGrant>,
    renew: ok(grant(2_000)) as Result<LeaseGrant>,
    command: null as Result<CommandResult> | null,
  };
  let hold: Promise<void> | null = null;
  const client: LeaseClient = {
    async requestLease(part, holder, takeOver) {
      calls.push(`request ${part} ${holder}${takeOver ? ' takeOver' : ''}`);
      return answers.request;
    },
    async renewLease(part, lease) {
      calls.push(`renew ${part} ${lease}`);
      return answers.renew;
    },
    async releaseLease(part, lease) {
      calls.push(`release ${part} ${lease}`);
      return ok(undefined);
    },
    async command(part, lease, command) {
      calls.push(`command ${part} ${lease}`);
      commands.push(command);
      if (hold) await hold;
      return answers.command ?? ok({ applied: command, clamped: false, deadlineUs: 3_000 });
    },
  };
  return {
    client,
    calls,
    commands,
    answers,
    /** Commands wait until the returned function is called, like a slow core. */
    slow: () => {
      let done!: () => void;
      hold = new Promise((r) => (done = r));
      return () => {
        hold = null;
        done();
      };
    },
  };
}

function keeper(c = fakeClient()) {
  const seen: LeaseStatus[] = [];
  const k = new LeaseKeeper(c.client, 'base', 'Drive window', (s) => seen.push(s));
  return { k, c, seen };
}

const go = baseCommand({ vx: 0.3, vy: 0, omega: 0 });
const still = baseCommand({ vx: 0, vy: 0, omega: 0 });

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('LeaseKeeper', () => {
  it('requests the lease once, also when asked twice at the same time', async () => {
    const { k, c } = keeper();
    const [a, b] = await Promise.all([k.acquire(), k.acquire()]);
    expect([a, b]).toEqual([true, true]);
    expect(c.calls).toEqual(['request base Drive window']);
    expect(k.status).toEqual({ kind: 'held', grant: grant() });
  });

  it('says why a lease was refused', async () => {
    const { k, c } = keeper();
    c.answers.request = fail('lease-held', '"drive-square.py" is driving "base".');
    expect(await k.acquire()).toBe(false);
    expect(k.status).toMatchObject({ kind: 'refused', error: { code: 'lease-held' } });
  });

  it('takes over when asked', async () => {
    const { k, c } = keeper();
    await k.acquire(true);
    expect(c.calls).toEqual(['request base Drive window takeOver']);
  });

  it('renews the lease while it is held and nothing else is sent', async () => {
    const { k, c } = keeper();
    await k.acquire();
    await vi.advanceTimersByTimeAsync(350);
    expect(c.calls.filter((x) => x.startsWith('renew'))).toHaveLength(3);
  });

  it('without autoRenew, only renew() and commands keep it alive (an app window’s page)', async () => {
    const c = fakeClient();
    const k = new LeaseKeeper(c.client, 'base', 'app:x', () => {}, { autoRenew: false });
    expect(await k.renew()).toBeNull();
    await k.acquire();
    await vi.advanceTimersByTimeAsync(500);
    expect(c.calls.filter((x) => x.startsWith('renew'))).toEqual([]);
    expect((await k.renew())?.ok).toBe(true);
    expect(c.calls.filter((x) => x.startsWith('renew'))).toEqual(['renew base L1']);
    c.answers.renew = fail('lease-ended', 'It expired.');
    await k.renew();
    expect(k.status).toMatchObject({ kind: 'ended' });
  });

  it('does not renew while commands keep it alive', async () => {
    const { k, c } = keeper();
    await k.acquire();
    for (let i = 0; i < 6; i++) {
      await k.send(go);
      await vi.advanceTimersByTimeAsync(50);
    }
    expect(c.calls.filter((x) => x.startsWith('renew'))).toEqual([]);
  });

  it('releases the lease and stops renewing', async () => {
    const { k, c } = keeper();
    await k.acquire();
    await k.release();
    expect(c.calls.at(-1)).toBe('release base L1');
    expect(k.status).toEqual({ kind: 'free' });
    await vi.advanceTimersByTimeAsync(500);
    expect(c.calls.filter((x) => x.startsWith('renew'))).toEqual([]);
  });

  it('releases only after a command that is still on its way', async () => {
    const { k, c } = keeper();
    await k.acquire();
    const done = c.slow();
    const sending = k.send(go);
    const releasing = k.release();
    await vi.advanceTimersByTimeAsync(0);
    expect(c.calls.at(-1)).toBe('command base L1');
    done();
    await Promise.all([sending, releasing]);
    expect(c.calls.at(-1)).toBe('release base L1');
  });

  it('ends with the reason the leases topic gives for its own lease', async () => {
    const { k } = keeper();
    await k.acquire();
    k.onLeaseEnded({ part: 'base', holder: 'Drive window', deadlineUs: 1_500, locks: [] }, 'estop');
    expect(k.status).toEqual({ kind: 'ended', reason: 'estop' });
  });

  it("ignores the end of someone else's lease, including the one it just took over", async () => {
    const { k } = keeper();
    await k.acquire(true);
    k.onLeaseEnded({ part: 'base', holder: 'p.py', deadlineUs: 1_500, locks: [] }, 'taken-over');
    k.onLeaseEnded(
      { part: 'arm', holder: 'Drive window', deadlineUs: 1_500, locks: [] },
      'expired',
    );
    // Another Drive window's lease that ended before this one was granted.
    k.onLeaseEnded(
      { part: 'base', holder: 'Drive window', deadlineUs: 900, locks: [] },
      'taken-over',
    );
    expect(k.status.kind).toBe('held');
  });

  it('ends with the core’s reason when a command finds the lease gone', async () => {
    const { k, c } = keeper();
    await k.acquire();
    c.answers.command = {
      ok: false,
      error: {
        code: 'lease-ended',
        message: 'Your lease on "base" has ended: it expired.',
        hint: '',
        reason: 'expired',
      },
    };
    await k.send(go);
    expect(k.status).toMatchObject({
      kind: 'ended',
      reason: 'expired',
      error: { code: 'lease-ended' },
    });
  });

  it('ends without a reason when the lease is simply not there any more', async () => {
    const { k, c } = keeper();
    await k.acquire();
    c.answers.command = fail('lease-required', 'Commanding "base" needs a lease.');
    await k.send(go);
    expect(k.status).toMatchObject({
      kind: 'ended',
      reason: null,
      error: { code: 'lease-required' },
    });
  });

  it('ends with the e-stop as the reason when a command meets it', async () => {
    const { k, c } = keeper();
    await k.acquire();
    c.answers.command = fail('estop-engaged');
    await k.send(go);
    expect(k.status).toMatchObject({ kind: 'ended', reason: 'estop' });
  });

  it('keeps the lease when a single command is refused for its values', async () => {
    const { k, c } = keeper();
    await k.acquire();
    c.answers.command = fail('bad-command');
    const r = await k.send(go);
    expect(r?.ok).toBe(false);
    expect(k.status.kind).toBe('held');
  });

  it('sends nothing without a lease', async () => {
    const { k, c } = keeper();
    expect(await k.send(go)).toBeNull();
    expect(c.calls).toEqual([]);
  });
});

describe('DriveLoop', () => {
  function loop() {
    const { k, c } = keeper();
    return { d: new DriveLoop(k), k, c };
  }

  it('takes the lease on the first move and sends commands at 20 Hz while held', async () => {
    const { d, c } = loop();
    d.set(go);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(c.calls[0]).toBe('request base Drive window');
    const sent = c.commands.length;
    expect(sent).toBeGreaterThanOrEqual(19);
    expect(sent).toBeLessThanOrEqual(22);
  });

  it('sends one stop command on release, then only renews', async () => {
    const { d, c } = loop();
    d.set(go);
    await vi.advanceTimersByTimeAsync(200);
    d.set(still);
    await vi.advanceTimersByTimeAsync(0);
    expect(c.commands.at(-1)).toEqual(still);
    const after = c.commands.length;
    await vi.advanceTimersByTimeAsync(500);
    expect(c.commands.length).toBe(after);
    expect(c.calls.at(-1)).toMatch(/^renew/);
  });

  it('skips a beat rather than piling up commands on a slow core', async () => {
    const { d, c } = loop();
    d.set(go);
    await vi.advanceTimersByTimeAsync(0);
    const done = c.slow();
    await vi.advanceTimersByTimeAsync(500);
    expect(c.commands.length).toBeLessThanOrEqual(2);
    done();
  });

  it('sends the newest command, not every one it was given', async () => {
    const { d, c } = loop();
    d.set(go);
    await vi.advanceTimersByTimeAsync(0);
    const faster = baseCommand({ vx: 0.5, vy: 0, omega: 0 });
    d.set(baseCommand({ vx: 0.4, vy: 0, omega: 0 }));
    d.set(faster);
    await vi.advanceTimersByTimeAsync(50);
    expect(c.commands.at(-1)).toEqual(faster);
  });

  it('stops and lets go of the lease', async () => {
    const { d, k, c } = loop();
    d.set(go);
    await vi.advanceTimersByTimeAsync(100);
    await d.release();
    expect(c.commands.at(-1)).toEqual(still);
    expect(c.calls.at(-1)).toBe('release base L1');
    expect(k.status.kind).toBe('free');
    const n = c.calls.length;
    await vi.advanceTimersByTimeAsync(300);
    expect(c.calls.length).toBe(n);
  });

  it('does not ask again for a lease that was refused until the next move', async () => {
    const { d, c } = loop();
    c.answers.request = fail('lease-held');
    d.set(go);
    await vi.advanceTimersByTimeAsync(500);
    expect(c.calls.filter((x) => x.startsWith('request'))).toHaveLength(1);
    d.set(still);
    d.set(go);
    await vi.advanceTimersByTimeAsync(0);
    expect(c.calls.filter((x) => x.startsWith('request'))).toHaveLength(2);
  });
});

describe('driving helpers', () => {
  it('knows still from moving', () => {
    expect(isStill(still)).toBe(true);
    expect(isStill(go)).toBe(false);
    expect(
      isStill({ capability: 'motor@1', command: { kind: 'set-velocity', velocityRadS: 0 } }),
    ).toBe(true);
  });

  it('knows which parts can move', () => {
    const p = (capability: string) =>
      ({ signals: [{ address: 'x/y', capability }] }) as unknown as PartInfo;
    expect(canMove(p('planar-base@1'))).toBe(true);
    expect(canMove(p('motor@1'))).toBe(true);
    expect(canMove(p('distance@1'))).toBe(false);
  });

  it('puts every way a lease ends into words', () => {
    const reasons = [
      'released',
      'expired',
      'estop',
      'taken-over',
      'part-removed',
      'part-restarted',
      'part-failed',
      'part-paused',
      'shutdown',
    ] as const;
    const texts = reasons.map((r) => leaseEndText(r, 'base'));
    expect(new Set(texts).size).toBe(reasons.length);
    for (const text of texts) expect(text).toMatch(/\.$/);
    expect(leaseEndText('estop', 'base')).toBe('Stopped by the e-stop.');
  });

  it('falls back to the core’s words when it gave no reason', () => {
    const error: ErrorBody = { code: 'lease-ended', message: 'It ended: expired.', hint: '' };
    expect(leaseEndText(null, 'base', error)).toBe('It ended: expired.');
  });
});
