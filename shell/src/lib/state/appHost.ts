import { t } from '../i18n';
import type {
  AppStatus,
  ErrorBody,
  Hello,
  RoleInfo,
  ShellMessage,
  Source,
  Theme,
} from '../../../../sdk/js/shell/src/protocol';
import { LeaseKeeper, type LeaseClient, type LeaseStatus } from './driving';
import type { Result } from './result';
import type {
  AppInfo,
  AppRole,
  Command,
  LeaseEndReason,
  LeaseInfo,
  PartInfo,
  Reading,
  SignalValue,
} from './types';

/**
 * The shell's side of an app window (docs/apps.md §9): the app's UI runs in a sandboxed iframe
 * with an opaque origin and talks to the shell only through `postMessage`, in the v1 protocol of
 * `sdk/js/shell/src/protocol.ts`. This host answers it, and it is the security boundary: every
 * message is checked (the frame it came from, its origin, its shape) and every request against
 * the app's roles before anything reaches the core.
 *
 * - Only messages from this window's own frame, with the opaque origin `null`, are read at all.
 * - An app sees roles, never which part fills them or any other part.
 * - Reading: a role's part's signal of the role's capability, or one of the app's own derived
 *   signals. Subscriptions are throttled (at most 20 a second, 16 at once).
 * - Driving: only a `command = true` role, only while the app is on, after the D46 confirm, and
 *   only with a command of the role's capability. The lease is held as `app:<id>`, like the app's
 *   logic: the core's lease rules don't depend on the holder's name, and every lease list shows
 *   the user which app drives. It is renewed only by the page itself (its commands and `renew`),
 *   so a page that hangs lets it lapse within 250 ms and the part stops.
 * - A new page in the frame, the app turned off or a role given another part end what the old
 *   page had; closing the window ends everything.
 *
 * Plain TypeScript with the core and the frame behind interfaces: the hostile-message tests run
 * without a browser.
 */

/** The protocol version this host speaks. */
export const PROTOCOL = 1 as const;

/** The most readings a second a subscription gets, and the default. */
export const MAX_HZ = 20;
export const DEFAULT_HZ = 10;
/** The most subscriptions one window may hold. */
export const MAX_SUBSCRIPTIONS = 16;
/** The most messages one window may send a second (commands at 20 Hz need far fewer). */
export const MAX_MESSAGES_PER_S = 200;

/** A message as `window` hands it over: who sent it, from which origin, and what. */
export interface Incoming {
  origin: string;
  source: unknown;
  data: unknown;
}

export interface AppHostDeps {
  /** The app's id: the host only ever serves this app. */
  appId: string;
  /** The app as the core says it now (the `apps` topic); `null` once it is uninstalled. */
  app(): AppInfo | null;
  parts(): readonly PartInfo[];
  /** The frame's own window (`iframe.contentWindow`): the only source the host listens to. */
  frame(): unknown;
  /** Sends a message to the frame. */
  post(message: ShellMessage): void;
  readSignal(address: string): Promise<Result<SignalValue>>;
  watchSignal(
    address: string,
    maxHz: number,
    handlers: { onReading(r: Reading): void; onError(e: ErrorBody): void },
  ): () => void;
  leases: LeaseClient;
  watchLeaseEnds(onEnd: (lease: LeaseInfo, reason: LeaseEndReason) => void): () => void;
  /**
   * D46: the user is about to drive through this window. Resolves `true` once they said yes (or
   * already did this session).
   */
  confirmDriving(part: string): Promise<boolean>;
  theme(): Theme;
  /** Called with the parts the window drives, whenever that changes (the window shows them). */
  onDriving?: (parts: string[]) => void;
  /** Milliseconds, for the message rate. */
  now?: () => number;
}

type ErrorCode =
  | 'bad-request'
  | 'unknown-request'
  | 'unsupported-version'
  | 'unknown-role'
  | 'unknown-signal'
  | 'no-part'
  | 'not-commandable'
  | 'app-off'
  | 'not-confirmed'
  | 'lease-required'
  | 'bad-command'
  | 'too-many-requests'
  | 'too-many-subscriptions'
  | 'app-gone';

function fail(
  code: ErrorCode,
  params: { role?: string; name?: string; app?: string } = {},
): ErrorBody {
  const p = { role: params.role ?? '', name: params.name ?? '', app: params.app ?? '' };
  switch (code) {
    case 'bad-request':
      return { code, message: t('appHost.badRequest.message'), hint: t('appHost.badRequest.hint') };
    case 'unknown-request':
      return {
        code,
        message: t('appHost.unknownRequest.message', p),
        hint: t('appHost.unknownRequest.hint'),
      };
    case 'unsupported-version':
      return {
        code,
        message: t('appHost.unsupportedVersion.message'),
        hint: t('appHost.unsupportedVersion.hint'),
      };
    case 'unknown-role':
      return {
        code,
        message: t('appHost.unknownRole.message', p),
        hint: t('appHost.unknownRole.hint'),
      };
    case 'unknown-signal':
      return {
        code,
        message: t('appHost.unknownSignal.message', p),
        hint: t('appHost.unknownSignal.hint'),
      };
    case 'no-part':
      return { code, message: t('appHost.noPart.message', p), hint: t('appHost.noPart.hint') };
    case 'not-commandable':
      return {
        code,
        message: t('appHost.notCommandable.message', p),
        hint: t('appHost.notCommandable.hint'),
      };
    case 'app-off':
      return { code, message: t('appHost.appOff.message', p), hint: t('appHost.appOff.hint') };
    case 'not-confirmed':
      return {
        code,
        message: t('appHost.notConfirmed.message'),
        hint: t('appHost.notConfirmed.hint'),
      };
    case 'lease-required':
      return {
        code,
        message: t('appHost.leaseRequired.message', p),
        hint: t('appHost.leaseRequired.hint'),
      };
    case 'bad-command':
      return {
        code,
        message: t('appHost.badCommand.message', p),
        hint: t('appHost.badCommand.hint'),
      };
    case 'too-many-requests':
      return {
        code,
        message: t('appHost.tooManyRequests.message'),
        hint: t('appHost.tooManyRequests.hint'),
      };
    case 'too-many-subscriptions':
      return {
        code,
        message: t('appHost.tooManySubscriptions.message'),
        hint: t('appHost.tooManySubscriptions.hint'),
      };
    case 'app-gone':
      return { code, message: t('appHost.appGone.message'), hint: t('appHost.appGone.hint') };
  }
}

/** What a lease holder is called for an app: `app:<id>`, as the core names the app's logic. */
export function appHolder(appId: string): string {
  return `app:${appId}`;
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function isIndex(v: unknown): v is number {
  return Number.isSafeInteger(v) && (v as number) >= 0;
}

/** A role's part index: absent means 0; anything else must be a whole number from 0. */
function indexOf(v: unknown): number | null {
  if (v === undefined) return 0;
  return isIndex(v) ? v : null;
}

/** A `Source`, checked: exactly a role (with an index) or a published name. */
function sourceOf(v: unknown): Source | null {
  if (!isObject(v)) return null;
  const keys = Object.keys(v);
  if (typeof v.role === 'string' && keys.every((k) => k === 'role' || k === 'index')) {
    const index = indexOf(v.index);
    return index === null ? null : { role: v.role, index };
  }
  if (typeof v.published === 'string' && keys.length === 1) return { published: v.published };
  return null;
}

class Refused {
  constructor(readonly error: ErrorBody) {}
}

interface Subscription {
  source: Source;
  address: string;
  stop: () => void;
}

interface Driving {
  role: string;
  index: number;
  keeper: LeaseKeeper;
}

export class AppHost {
  /** Messages dropped without an answer (wrong source or origin, or not a protocol message). */
  ignored = 0;

  private readonly subs = new Map<number, Subscription>();
  private readonly drives = new Map<string, Driving>();
  private nextSub = 1;
  /** Bumped by every new page and by `dispose`: answers for an older page are dropped. */
  private generation = 0;
  private windowStart = 0;
  private count = 0;
  private lastStatus = '';
  private disposed = false;
  private readonly stopLeaseWatch: () => void;
  private readonly now: () => number;

  constructor(private readonly deps: AppHostDeps) {
    this.now = deps.now ?? Date.now;
    this.stopLeaseWatch = deps.watchLeaseEnds((lease, reason) => {
      for (const d of this.drives.values()) d.keeper.onLeaseEnded(lease, reason);
    });
    this.lastStatus = JSON.stringify(this.status());
  }

  /** One `message` event from `window`. Anything not from this window's frame is ignored. */
  receive(e: Incoming): void {
    if (this.disposed) return;
    const frame = this.deps.frame();
    // The frame is sandboxed without allow-same-origin: its origin is always the opaque `null`.
    if (frame === null || frame === undefined || e.source !== frame || e.origin !== 'null') {
      this.ignored++;
      return;
    }
    const m = e.data;
    if (!isObject(m) || !isIndex(m.id)) {
      this.ignored++;
      return;
    }
    const id = m.id;
    if (m.faber !== PROTOCOL) return this.reply(id, fail('unsupported-version'));
    if (!this.withinRate()) return this.reply(id, fail('too-many-requests'));
    if (typeof m.type !== 'string') return this.reply(id, fail('bad-request'));
    const generation = this.generation;
    this.handle(m.type, m)
      .then(
        (value) => ({ ok: true as const, value }),
        (e: unknown) => {
          if (e instanceof Refused) return { ok: false as const, error: e.error };
          throw e;
        },
      )
      .then((r) => {
        // An answer for a page that has gone (reloaded, or the window closed) goes nowhere.
        if (generation !== this.generation || this.disposed) return;
        if (r.ok) this.deps.post({ faber: PROTOCOL, re: id, ok: true, value: r.value });
        else this.reply(id, r.error);
      });
  }

  /** The frame loaded a page (its first, or another): forget everything the old page had. */
  reset(): void {
    this.generation++;
    void this.endAll();
    this.count = 0;
  }

  /** The app changed (the `apps` topic): tell the page, and end what it may no longer have. */
  appChanged(): void {
    if (this.disposed) return;
    const app = this.deps.app();
    const status = JSON.stringify(this.status());
    if (status !== this.lastStatus) {
      this.lastStatus = status;
      this.deps.post({ faber: PROTOCOL, event: 'status', status: this.status() });
    }
    for (const [id, sub] of [...this.subs]) {
      let still: string | null = null;
      try {
        still = this.address(sub.source);
      } catch {
        still = null;
      }
      if (still !== sub.address) this.endSubscription(id, fail('no-part', this.named(sub.source)));
    }
    for (const [key, d] of [...this.drives]) {
      const role = app?.roles.find((r) => r.role === d.role);
      const part = role?.parts[d.index];
      if (!app || !app.enabled || !role?.command || part !== d.keeper.part) {
        this.drives.delete(key);
        this.drivingChanged();
        void d.keeper.release();
        const error =
          app && app.enabled
            ? fail('no-part', { role: d.role })
            : fail('app-off', { app: app?.name ?? '' });
        this.deps.post({
          faber: PROTOCOL,
          event: 'lease-ended',
          role: d.role,
          index: d.index,
          reason: 'released',
          error,
        });
      }
    }
  }

  /** The shell's look changed: the page may follow it. */
  themeChanged(): void {
    if (this.disposed) return;
    this.deps.post({ faber: PROTOCOL, event: 'theme', theme: this.deps.theme() });
  }

  /** The window closed: its leases end (the parts stop) and its subscriptions stop. */
  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    this.generation++;
    this.stopLeaseWatch();
    await this.endAll();
  }

  /**
   * The user pressed Stop in the window's own bar (drawn by the shell, outside the frame, so an
   * app can't hide it): every lease ends, the parts stop, and the page hears why.
   */
  async stopDriving(): Promise<void> {
    const drives = [...this.drives.values()];
    this.drives.clear();
    this.drivingChanged();
    for (const d of drives) {
      this.deps.post({
        faber: PROTOCOL,
        event: 'lease-ended',
        role: d.role,
        index: d.index,
        reason: 'released',
        error: {
          code: 'lease-ended',
          message: t('appHost.stopped.message', { role: d.role }),
          hint: t('appHost.stopped.hint'),
        },
      });
    }
    await Promise.all(drives.map((d) => d.keeper.release()));
  }

  private drivingChanged(): void {
    const parts = [...this.drives.values()].filter((d) => d.keeper.held).map((d) => d.keeper.part);
    this.deps.onDriving?.([...new Set(parts)].sort());
  }

  /** How many leases the window holds (tests, and the window's own display). */
  get leases(): number {
    return this.drives.size;
  }

  get subscriptions(): number {
    return this.subs.size;
  }

  private async endAll(): Promise<void> {
    for (const id of [...this.subs.keys()]) {
      this.subs.get(id)?.stop();
      this.subs.delete(id);
    }
    const drives = [...this.drives.values()];
    this.drives.clear();
    this.drivingChanged();
    await Promise.all(drives.map((d) => d.keeper.release()));
  }

  private withinRate(): boolean {
    const now = this.now();
    if (now - this.windowStart >= 1000) {
      this.windowStart = now;
      this.count = 0;
    }
    this.count++;
    return this.count <= MAX_MESSAGES_PER_S;
  }

  private reply(id: number, error: ErrorBody): void {
    this.deps.post({ faber: PROTOCOL, re: id, ok: false, error });
  }

  private app(): AppInfo {
    const app = this.deps.app();
    if (!app || app.id !== this.deps.appId) throw new Refused(fail('app-gone'));
    return app;
  }

  private status(): AppStatus {
    const app = this.deps.app();
    if (!app) return { enabled: false, state: 'stopped' };
    const s: AppStatus = { enabled: app.enabled, state: app.state.kind };
    if (app.state.message) s.message = app.state.message;
    return s;
  }

  private named(source: Source): { role?: string; name?: string } {
    return 'role' in source ? { role: source.role } : { name: source.published };
  }

  private role(name: unknown): AppRole {
    if (typeof name !== 'string') throw new Refused(fail('bad-request'));
    const role = this.app().roles.find((r) => r.role === name);
    if (!role) throw new Refused(fail('unknown-role', { role: name }));
    return role;
  }

  /** The part filling `role` at `index`, or a refusal saying there is none. */
  private part(role: AppRole, index: number): PartInfo {
    const name = role.parts[index];
    const part = name === undefined ? undefined : this.deps.parts().find((p) => p.name === name);
    if (!part) throw new Refused(fail('no-part', { role: role.role }));
    return part;
  }

  /** The signal a source reads: the role's part's signal of its capability, or a derived one. */
  private address(source: Source): string {
    if ('published' in source) {
      const s = this.app().publishes.find((p) => p.name === source.published);
      if (!s) throw new Refused(fail('unknown-signal', { name: source.published }));
      return s.address;
    }
    const role = this.role(source.role);
    const part = this.part(role, source.index ?? 0);
    const signal = part.signals.find((s) => s.capability === role.capability);
    if (!signal) throw new Refused(fail('no-part', { role: role.role }));
    return signal.address;
  }

  private source(v: unknown): Source {
    const s = sourceOf(v);
    if (!s) throw new Refused(fail('bad-request'));
    return s;
  }

  private async handle(type: string, m: Record<string, unknown>): Promise<unknown> {
    switch (type) {
      case 'hello':
        return this.hello();
      case 'roles':
        return this.roles();
      case 'status':
        this.app();
        return this.status();
      case 'theme':
        return this.deps.theme();
      case 'latest':
        return this.latest(this.source(m.source));
      case 'subscribe':
        return this.subscribe(this.source(m.source), m.maxHz);
      case 'unsubscribe':
        return this.unsubscribe(m.subscription);
      case 'drive':
        return this.drive(m.role, m.index);
      case 'renew':
        return this.renew(m.role, m.index);
      case 'command':
        return this.command(m.role, m.index, m.command);
      case 'release':
        return this.release(m.role, m.index);
      default:
        throw new Refused(fail('unknown-request', { name: type }));
    }
  }

  private hello(): Hello {
    const app = this.app();
    return {
      app: { id: app.id, name: app.name, version: app.version },
      roles: this.roles(),
      published: app.publishes.map((p) => ({ name: p.name, capability: p.capability })),
      status: this.status(),
      theme: this.deps.theme(),
    };
  }

  private roles(): RoleInfo[] {
    return this.app().roles.map((r) => ({
      role: r.role,
      capability: r.capability,
      command: r.command,
      count: r.count,
      parts: r.parts.length,
    }));
  }

  private async latest(source: Source): Promise<Reading | null> {
    const address = this.address(source);
    const r = await this.deps.readSignal(address);
    if (!r.ok) throw new Refused(r.error);
    return r.value.reading ?? null;
  }

  private subscribe(source: Source, maxHz: unknown): { subscription: number } {
    if (maxHz !== undefined && (typeof maxHz !== 'number' || !Number.isFinite(maxHz) || maxHz <= 0))
      throw new Refused(fail('bad-request'));
    const address = this.address(source);
    if (this.subs.size >= MAX_SUBSCRIPTIONS) throw new Refused(fail('too-many-subscriptions'));
    const hz = Math.min(MAX_HZ, (maxHz as number | undefined) ?? DEFAULT_HZ);
    const id = this.nextSub++;
    const stop = this.deps.watchSignal(address, hz, {
      onReading: (reading) => {
        if (this.subs.has(id))
          this.deps.post({ faber: PROTOCOL, event: 'signal', subscription: id, reading });
      },
      onError: (error) => this.endSubscription(id, error),
    });
    this.subs.set(id, { source, address, stop });
    return { subscription: id };
  }

  private endSubscription(id: number, error: ErrorBody): void {
    const sub = this.subs.get(id);
    if (!sub) return;
    this.subs.delete(id);
    sub.stop();
    this.deps.post({ faber: PROTOCOL, event: 'signal-ended', subscription: id, error });
  }

  private unsubscribe(id: unknown): null {
    if (!isIndex(id)) throw new Refused(fail('bad-request'));
    const sub = this.subs.get(id);
    this.subs.delete(id);
    sub?.stop();
    return null;
  }

  /** The role and part index a drive request names, checked: a `command = true` role. */
  private drivable(roleName: unknown, rawIndex: unknown): { role: AppRole; index: number } {
    const role = this.role(roleName);
    const index = indexOf(rawIndex);
    if (index === null) throw new Refused(fail('bad-request'));
    if (!role.command) throw new Refused(fail('not-commandable', { role: role.role }));
    return { role, index };
  }

  private async drive(roleName: unknown, rawIndex: unknown): Promise<null> {
    const { role, index } = this.drivable(roleName, rawIndex);
    const app = this.app();
    if (!app.enabled) throw new Refused(fail('app-off', { app: app.name }));
    const part = this.part(role, index);
    const key = `${role.role}#${index}`;
    if (this.drives.get(key)?.keeper.held) return null;
    const generation = this.generation;
    if (!(await this.deps.confirmDriving(part.name))) throw new Refused(fail('not-confirmed'));
    if (generation !== this.generation || this.disposed) throw new Refused(fail('app-gone'));
    const keeper = new LeaseKeeper(
      this.deps.leases,
      part.name,
      appHolder(app.id),
      (status) => this.leaseChanged(key, keeper, status),
      { autoRenew: false },
    );
    const driving: Driving = { role: role.role, index, keeper };
    this.drives.set(key, driving);
    const got = await keeper.acquire(false);
    if (generation !== this.generation || this.disposed) {
      // The page went away while the lease was on its way: give it straight back.
      void keeper.release();
      throw new Refused(fail('app-gone'));
    }
    if (!got) {
      this.drives.delete(key);
      this.drivingChanged();
      const s = keeper.status;
      throw new Refused(
        s.kind === 'refused' ? s.error : fail('lease-required', { role: role.role }),
      );
    }
    return null;
  }

  private leaseChanged(key: string, keeper: LeaseKeeper, status: LeaseStatus): void {
    if (status.kind === 'held') this.drivingChanged();
    if (status.kind !== 'ended') return;
    const d = this.drives.get(key);
    if (!d || d.keeper !== keeper) return;
    this.drives.delete(key);
    this.drivingChanged();
    const reason = status.reason ?? 'expired';
    this.deps.post({
      faber: PROTOCOL,
      event: 'lease-ended',
      role: d.role,
      index: d.index,
      reason,
      error: status.error ?? {
        code: 'lease-ended',
        message: t('appHost.leaseEnded.message', { role: d.role }),
        hint: t('appHost.leaseEnded.hint'),
      },
    });
  }

  private held(roleName: unknown, rawIndex: unknown): Driving {
    const { role, index } = this.drivable(roleName, rawIndex);
    const d = this.drives.get(`${role.role}#${index}`);
    if (!d || !d.keeper.held) throw new Refused(fail('lease-required', { role: role.role }));
    return d;
  }

  private async renew(roleName: unknown, rawIndex: unknown): Promise<null> {
    const d = this.held(roleName, rawIndex);
    const r = await d.keeper.renew();
    if (r && !r.ok) throw new Refused(r.error);
    return null;
  }

  private async command(
    roleName: unknown,
    rawIndex: unknown,
    command: unknown,
  ): Promise<{ applied: Command; clamped: boolean }> {
    const { role } = this.drivable(roleName, rawIndex);
    if (!isObject(command) || command.capability !== role.capability || !isObject(command.command))
      throw new Refused(fail('bad-command', { role: role.role }));
    const d = this.held(roleName, rawIndex);
    const r = await d.keeper.send(command as unknown as Command);
    if (r === null) throw new Refused(fail('lease-required', { role: role.role }));
    if (!r.ok) throw new Refused(r.error);
    return { applied: r.value.applied, clamped: r.value.clamped };
  }

  private async release(roleName: unknown, rawIndex: unknown): Promise<null> {
    const { role, index } = this.drivable(roleName, rawIndex);
    const key = `${role.role}#${index}`;
    const d = this.drives.get(key);
    this.drives.delete(key);
    this.drivingChanged();
    await d?.keeper.release();
    return null;
  }
}
