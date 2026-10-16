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
import type { AppMessageData, AppWindowHandlers, AppWindowSession } from '../api/socket';
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
 * - State: the app's saved state (`state-get/set/keys/watch`), shared with its logic through the
 *   core. Keys are 1-256 bytes and everything together at most 64 KB, checked here before the
 *   core checks it again.
 * - Messages with the app's logic (`message-send/watch`, docs/apps.md §12): live, never saved,
 *   over the window's own link to the core. A kind of 1-64 characters of `a-z`, `0-9`, `-`, at
 *   most 4 KB and 50 a second, checked here before the core checks them again.
 * - Driving through the logic (`drive-via`): only a role declared `drive = "window"`, only while
 *   the app is on, after the same D46 confirm, with the same Stop bar. The core lets the logic
 *   hold the role's lease only while this window drives it, and ends it when the page lets go,
 *   the user presses Stop, the window closes or the page's `heartbeat` stops for 300 ms.
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
/** An app's state: its keys and values together, in bytes, as the core counts them (UTF-8). */
export const MAX_STATE_BYTES = 64 * 1024;
/** The longest state key (bytes). */
export const MAX_STATE_KEY_BYTES = 256;
/** A message to the app's logic: its kind and data together, in bytes, as the core counts them. */
export const MAX_APP_MESSAGE_BYTES = 4 * 1024;
/** The most messages a second one window may send its logic. */
export const MAX_APP_MESSAGES_PER_S = 50;
/** A message's kind: lowercase words joined by `-`. */
const KIND = /^[a-z0-9-]{1,64}$/;
/** Kinds only the core sends the logic. */
const RESERVED_KINDS = new Set(['drive-started', 'drive-ended']);

/** The app's saved state in the core (docs/api.md, "Apps"), as the host reaches it. */
export interface AppStateClient {
  /** Everything now (`onValues`, again after missed changes), then every change. */
  watch(handlers: {
    onValues(values: Record<string, string>): void;
    onChange(key: string, value: string | null): void;
    onError(e: ErrorBody): void;
  }): () => void;
  /** Sets a key, or removes it with `null`. */
  set(key: string, value: string | null): Promise<Result<unknown>>;
}

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
  /** The app's saved state, shared with its logic. */
  appState: AppStateClient;
  /**
   * Opens the window's live link to the app's logic (messages, driving through it). The host
   * opens it when the page first needs it, and closes it with the page.
   */
  openChannel(handlers: AppWindowHandlers): AppWindowSession;
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
  | 'app-gone'
  | 'bad-key'
  | 'state-full'
  | 'not-window-driven'
  | 'bad-kind'
  | 'message-too-large'
  | 'too-many-messages'
  | 'not-connected';

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
    case 'bad-key':
      return { code, message: t('appHost.badKey.message'), hint: t('appHost.badKey.hint') };
    case 'state-full':
      return { code, message: t('appHost.stateFull.message'), hint: t('appHost.stateFull.hint') };
    case 'not-window-driven':
      return {
        code,
        message: t('appHost.notWindowDriven.message', p),
        hint: t('appHost.notWindowDriven.hint'),
      };
    case 'bad-kind':
      return { code, message: t('appHost.badKind.message'), hint: t('appHost.badKind.hint') };
    case 'message-too-large':
      return {
        code,
        message: t('appHost.messageTooLarge.message'),
        hint: t('appHost.messageTooLarge.hint'),
      };
    case 'too-many-messages':
      return {
        code,
        message: t('appHost.tooManyMessages.message'),
        hint: t('appHost.tooManyMessages.hint'),
      };
    case 'not-connected':
      return {
        code,
        message: t('appHost.notConnected.message'),
        hint: t('appHost.notConnected.hint'),
      };
  }
}

/** Why a drive through the logic ended, for the page. */
function driveEnded(role: string, reason: string): ErrorBody {
  const code = 'drive-ended';
  switch (reason) {
    case 'stopped':
      return {
        code,
        message: t('appHost.stopped.message', { role }),
        hint: t('appHost.stopped.hint'),
      };
    case 'heartbeat-lost':
      return {
        code,
        message: t('appHost.heartbeatLost.message', { role }),
        hint: t('appHost.heartbeatLost.hint'),
      };
    case 'connection-lost':
      return {
        code,
        message: t('appHost.connectionLost.message', { role }),
        hint: t('appHost.connectionLost.hint'),
      };
    default:
      return {
        code,
        message: t('appHost.leaseEnded.message', { role }),
        hint: t('appHost.leaseEnded.hint'),
      };
  }
}

const utf8 = new TextEncoder();

/** A part with no mount in parts.toml sits at the machine's origin. */
const ORIGIN = { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 };

function bytes(s: string): number {
  return utf8.encode(s).length;
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

/** A role this window drives through the app's logic. */
interface DrivingVia {
  role: string;
  index: number;
  part: string;
}

/** The window's link to the app's logic, while it is open or opening. */
interface Channel {
  session: AppWindowSession;
  /** Resolves true once the core opened it, false if it never did or it closed. */
  opened: Promise<boolean>;
  settle(opened: boolean): void;
  /** `drive-via`s waiting for the core's answer, by role and index. */
  pending: Map<string, (error: ErrorBody | null) => void>;
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
  /** The app's state as the core has it, once a page asked for it; kept by the `app-state` topic. */
  private stateValues: Map<string, string> | null = null;
  private stateReady: Promise<Map<string, string>> | null = null;
  private stopStateWatch: (() => void) | null = null;
  /** The page asked for `state` events (`state-watch`). */
  private stateWatched = false;
  private channel: Channel | null = null;
  private readonly vias = new Map<string, DrivingVia>();
  /** The page asked for `message` events (`message-watch`). */
  private messagesWatched = false;
  private messageWindowStart = 0;
  private messageCount = 0;

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
    this.stateWatched = false;
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
    for (const [key, v] of [...this.vias]) {
      const role = app?.roles.find((r) => r.role === v.role);
      const part = role?.parts[v.index];
      if (!app || !app.enabled || !role?.command || role.drive !== 'window' || part !== v.part) {
        this.vias.delete(key);
        this.drivingChanged();
        this.channel?.session.release(v.role, v.index);
        const error =
          app && app.enabled
            ? fail('no-part', { role: v.role })
            : fail('app-off', { app: app?.name ?? '' });
        this.deps.post({
          faber: PROTOCOL,
          event: 'drive-ended',
          role: v.role,
          index: v.index,
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
    this.stopStateWatch?.();
    this.stopStateWatch = null;
    await this.endAll();
  }

  /**
   * The user pressed Stop in the window's own bar (drawn by the shell, outside the frame, so an
   * app can't hide it): every lease ends, the parts stop, and the page hears why.
   */
  async stopDriving(): Promise<void> {
    const drives = [...this.drives.values()];
    this.drives.clear();
    const vias = [...this.vias.values()];
    this.vias.clear();
    this.drivingChanged();
    // The core ends what the window drives through the logic, and the logic's lease with it.
    if (vias.length > 0) this.channel?.session.stop();
    for (const v of vias) {
      this.deps.post({
        faber: PROTOCOL,
        event: 'drive-ended',
        role: v.role,
        index: v.index,
        reason: 'stopped',
        error: driveEnded(v.role, 'stopped'),
      });
    }
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
    for (const v of this.vias.values()) parts.push(v.part);
    this.deps.onDriving?.([...new Set(parts)].sort());
  }

  /** How many roles the window drives through the app's logic (tests). */
  get drivingVia(): number {
    return this.vias.size;
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
    this.closeChannel();
    this.drivingChanged();
    await Promise.all(drives.map((d) => d.keeper.release()));
  }

  /** Closes the window's link to the logic: the core ends what it drove through it. */
  private closeChannel(): void {
    const ch = this.channel;
    this.channel = null;
    this.vias.clear();
    this.messagesWatched = false;
    if (!ch) return;
    ch.session.close();
    ch.settle(false);
    for (const p of ch.pending.values()) p(fail('app-gone'));
    ch.pending.clear();
  }

  /** The window's link to the logic, opened the first time the page needs it. */
  private async link(): Promise<AppWindowSession> {
    let ch = this.channel;
    if (!ch) {
      let settle: (opened: boolean) => void = () => {};
      const opened = new Promise<boolean>((resolve) => (settle = resolve));
      const made: Channel = {
        session: { send() {}, heartbeat() {}, drive() {}, release() {}, stop() {}, close() {} },
        opened,
        settle: (v) => settle(v),
        pending: new Map(),
      };
      ch = made;
      this.channel = made;
      made.session = this.deps.openChannel({
        onOpened: () => made.settle(true),
        onMessage: (kind, data) => this.fromLogic(made, kind, data),
        onDrive: (role, index, error) => {
          const key = `${role}#${index}`;
          const answer = made.pending.get(key);
          made.pending.delete(key);
          answer?.(error);
        },
        onDriveEnded: (role, index, reason) => {
          if (this.channel === made) this.viaEnded(`${role}#${index}`, reason);
        },
        onClosed: () => {
          made.settle(false);
          for (const p of made.pending.values()) p(fail('not-connected'));
          made.pending.clear();
          if (this.channel !== made) return;
          this.channel = null;
          this.messagesWatched = false;
          for (const key of [...this.vias.keys()]) this.viaEnded(key, 'connection-lost');
        },
        // Refusals the host didn't foresee: the page already has its answer.
        onError: () => {},
      });
    }
    if (!(await ch.opened)) throw new Refused(fail('not-connected'));
    return ch.session;
  }

  private fromLogic(ch: Channel, kind: string, data: AppMessageData): void {
    if (this.channel !== ch || !this.messagesWatched || this.disposed) return;
    this.deps.post({ faber: PROTOCOL, event: 'message', kind, data });
  }

  /** A drive through the logic ended without the page letting go: the page hears why. */
  private viaEnded(key: string, reason: string): void {
    const v = this.vias.get(key);
    if (!v) return;
    this.vias.delete(key);
    this.drivingChanged();
    this.deps.post({
      faber: PROTOCOL,
      event: 'drive-ended',
      role: v.role,
      index: v.index,
      reason,
      error: driveEnded(v.role, reason),
    });
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
      case 'state-get':
        return (await this.state()).get(this.stateKey(m.key)) ?? null;
      case 'state-keys':
        return [...(await this.state()).keys()].sort();
      case 'state-set':
        return this.setState(m.key, m.value);
      case 'state-watch':
        await this.state();
        this.stateWatched = true;
        return null;
      case 'message-send':
        return this.sendMessage(m.kind, m.data);
      case 'message-watch':
        this.app();
        await this.link();
        this.messagesWatched = true;
        return null;
      case 'drive-via':
        return this.driveVia(m.role, m.index);
      case 'drive-via-release':
        return this.releaseVia(m.role, m.index);
      case 'heartbeat':
        this.channel?.session.heartbeat();
        return null;
      default:
        throw new Refused(fail('unknown-request', { name: type }));
    }
  }

  /** A state key, checked: 1 to 256 bytes. */
  private stateKey(key: unknown): string {
    if (typeof key !== 'string') throw new Refused(fail('bad-request'));
    const n = bytes(key);
    if (n === 0 || n > MAX_STATE_KEY_BYTES) throw new Refused(fail('bad-key'));
    return key;
  }

  /** The app's state, following the core's from the first time a page asks. */
  private state(): Promise<Map<string, string>> {
    this.app();
    this.stateReady ??= new Promise((resolve, reject) => {
      this.stopStateWatch = this.deps.appState.watch({
        onValues: (values) => {
          const before = this.stateValues;
          const now = new Map(Object.entries(values));
          this.stateValues = now;
          if (before) {
            for (const [k] of before) if (!now.has(k)) this.stateChanged(k, null);
            for (const [k, v] of now) if (before.get(k) !== v) this.stateChanged(k, v);
          }
          resolve(now);
        },
        onChange: (key, value) => {
          const values = this.stateValues;
          if (!values) return;
          if (value === null) values.delete(key);
          else values.set(key, value);
          this.stateChanged(key, value);
        },
        onError: (e) => {
          this.stopStateWatch?.();
          this.stopStateWatch = null;
          this.stateReady = null;
          this.stateValues = null;
          reject(new Refused(e));
        },
      });
    });
    return this.stateReady;
  }

  private stateChanged(key: string, value: string | null): void {
    if (this.stateWatched && !this.disposed)
      this.deps.post({ faber: PROTOCOL, event: 'state', key, value });
  }

  private async setState(rawKey: unknown, value: unknown): Promise<null> {
    const key = this.stateKey(rawKey);
    if (value !== null && typeof value !== 'string') throw new Refused(fail('bad-request'));
    const values = await this.state();
    const old = values.get(key);
    let total = 0;
    for (const [k, v] of values) total += bytes(k) + bytes(v);
    if (old !== undefined) total -= bytes(key) + bytes(old);
    if (value !== null && total + bytes(key) + bytes(value) > MAX_STATE_BYTES)
      throw new Refused(fail('state-full'));
    const r = await this.deps.appState.set(key, value);
    if (!r.ok) throw new Refused(r.error);
    // The topic says so too; keep the copy right for the next request already.
    if (value === null) values.delete(key);
    else values.set(key, value);
    return null;
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
    const parts = this.deps.parts();
    return this.app().roles.map((r) => ({
      role: r.role,
      capability: r.capability,
      command: r.command,
      count: r.count,
      parts: r.parts.length,
      // Where each part sits (an app applies it to the part's readings), never which part it is.
      mounts: r.parts.map((name) => ({ ...ORIGIN, ...parts.find((p) => p.name === name)?.mount })),
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

  private messageWithinRate(): boolean {
    const now = this.now();
    if (now - this.messageWindowStart >= 1000) {
      this.messageWindowStart = now;
      this.messageCount = 0;
    }
    this.messageCount++;
    return this.messageCount <= MAX_APP_MESSAGES_PER_S;
  }

  private async sendMessage(kind: unknown, data: unknown): Promise<null> {
    if (typeof kind !== 'string' || !KIND.test(kind) || RESERVED_KINDS.has(kind))
      throw new Refused(fail('bad-kind'));
    if (typeof data !== 'string' && !(data instanceof Uint8Array))
      throw new Refused(fail('bad-request'));
    const size = bytes(kind) + (typeof data === 'string' ? bytes(data) : data.byteLength);
    if (size > MAX_APP_MESSAGE_BYTES) throw new Refused(fail('message-too-large'));
    if (!this.messageWithinRate()) throw new Refused(fail('too-many-messages'));
    const app = this.app();
    if (!app.enabled) throw new Refused(fail('app-off', { app: app.name }));
    const session = await this.link();
    // A copy: nothing the page keeps a hold of reaches the core.
    session.send(kind, typeof data === 'string' ? data : new Uint8Array(data));
    return null;
  }

  private async driveVia(roleName: unknown, rawIndex: unknown): Promise<null> {
    const { role, index } = this.drivable(roleName, rawIndex);
    if (role.drive !== 'window') throw new Refused(fail('not-window-driven', { role: role.role }));
    const app = this.app();
    if (!app.enabled) throw new Refused(fail('app-off', { app: app.name }));
    const part = this.part(role, index);
    const key = `${role.role}#${index}`;
    if (this.vias.has(key)) return null;
    const generation = this.generation;
    const gone = () => generation !== this.generation || this.disposed;
    if (!(await this.deps.confirmDriving(part.name))) throw new Refused(fail('not-confirmed'));
    if (gone()) throw new Refused(fail('app-gone'));
    const session = await this.link();
    const ch = this.channel;
    if (gone() || !ch) throw new Refused(fail('app-gone'));
    const error = await new Promise<ErrorBody | null>((answer) => {
      ch.pending.get(key)?.(fail('app-gone'));
      ch.pending.set(key, answer);
      session.drive(role.role, index);
    });
    if (error) throw new Refused(error);
    // The page went away meanwhile: closing its link ended the drive in the core already.
    if (gone() || this.channel !== ch) throw new Refused(fail('app-gone'));
    this.vias.set(key, { role: role.role, index, part: part.name });
    this.drivingChanged();
    return null;
  }

  private releaseVia(roleName: unknown, rawIndex: unknown): null {
    const { role, index } = this.drivable(roleName, rawIndex);
    if (this.vias.delete(`${role.role}#${index}`)) {
      this.drivingChanged();
      this.channel?.session.release(role.role, index);
    }
    return null;
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
