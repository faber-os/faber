/**
 * `@faber-os/shell`: the JavaScript SDK for a Faber app's UI (docs/apps.md §9).
 *
 * An app's UI runs in a sandboxed window inside the Faber shell. It can't reach the core or the
 * network; everything goes through the shell, which checks each request against the app's roles.
 *
 * ```js
 * import { faber } from './faber-shell.js';
 *
 * faber.subscribe('lidar', (scan) => draw(scan));
 * const base = await faber.drive('base');
 * await base.command({ capability: 'planar-base@1', command: { kind: 'set-velocity', vx: 0.2, vy: 0, omega: 0 } });
 * await base.release();
 * ```
 *
 * No dependencies; one ES module. The protocol it speaks is in `protocol.ts`.
 */
import type {
  AppStatus,
  Command,
  CommandResult,
  ErrorBody,
  Hello,
  PublishedInfo,
  Reading,
  ReplyValues,
  Request,
  RequestType,
  RoleInfo,
  ShellMessage,
  Source,
  Theme,
} from './protocol';

export type {
  AppStatus,
  Command,
  CommandResult,
  ErrorBody,
  Hello,
  PublishedInfo,
  Reading,
  RoleInfo,
  Theme,
} from './protocol';

/** The protocol version this SDK speaks. */
export const PROTOCOL = 1;

/**
 * Something the shell (or the SDK) said no to, in Faber's house style: `message` says what
 * happened, `hint` what to try next, and `code` is stable for programs (`unknown-role`,
 * `lease-held`, `estop-engaged`, …).
 */
export class FaberError extends Error {
  readonly code: string;
  readonly hint: string;

  constructor(body: ErrorBody) {
    super(body.message);
    this.name = 'FaberError';
    this.code = body.code;
    this.hint = body.hint;
  }
}

/** Where messages go: the shell's window (`window.parent`). */
export interface Endpoint {
  postMessage(message: unknown, targetOrigin: string): void;
}

/** Where the shell's messages arrive: this app's own window. */
export interface Inbox {
  addEventListener(type: 'message', listener: (event: MessageEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: MessageEvent) => void): void;
}

export interface ConnectOptions {
  /** The shell's window. Default: `window.parent`. */
  parent?: Endpoint;
  /** This window. Default: `window`. */
  self?: Inbox;
  /** How long to wait for an answer before giving up (ms). Default 5000. */
  timeoutMs?: number;
}

export interface SubscribeOptions {
  /** Which part of a `count = "1.."` role (default 0). */
  index?: number;
  /** At most this many readings a second (default 10; the shell caps it at 20). */
  maxHz?: number;
  /** Called if the subscription can't start or ends (its part went away). */
  onError?: (error: FaberError) => void;
}

/** A role being driven: the lease is held until `release()` or until it ends on its own. */
export interface Drive {
  readonly role: string;
  readonly index: number;
  /** True once the lease ended, released or not. */
  readonly ended: boolean;
  /** Sends a command; it also keeps the lease alive. */
  command(command: Command): Promise<CommandResult>;
  /** Stops driving: the part gets its safe state (stopped). */
  release(): Promise<void>;
  /**
   * Called once if the lease ends without `release()`: the e-stop, an operator taking over, the
   * part going away, the app turned off, or this page too busy to keep it alive.
   */
  onEnd(callback: (error: FaberError) => void): void;
}

export interface Faber {
  /** The app, its roles and derived signals, its status and the shell's theme. */
  hello(): Promise<Hello>;
  /** The app's roles: name, capability, whether it may drive it, and how many parts fill it. */
  roles(): Promise<RoleInfo[]>;
  /** The newest reading of a role's part, or `null` before there is one. */
  latest(role: string, options?: { index?: number }): Promise<Reading | null>;
  /** Calls `callback` with each new reading of a role's part. Returns a function that stops it. */
  subscribe(
    role: string,
    callback: (reading: Reading) => void,
    options?: SubscribeOptions,
  ): () => void;
  /** The newest reading of one of this app's own derived signals (`[[publishes]]`). */
  published(name: string): Promise<Reading | null>;
  /** Each new reading of one of this app's own derived signals. Returns a stop function. */
  subscribePublished(
    name: string,
    callback: (reading: Reading) => void,
    options?: Omit<SubscribeOptions, 'index'>,
  ): () => void;
  /**
   * Starts driving a `command = true` role: Faber asks the user first (once per session) and
   * takes the lease. Keep sending commands, or the lease lapses within 250 ms of this page going
   * quiet; the SDK keeps it alive while the page runs.
   */
  drive(role: string, options?: { index?: number }): Promise<Drive>;
  /** Whether the app is on, and how it runs. */
  status(): Promise<AppStatus>;
  /** Calls `callback` whenever the app is turned on or off or its state changes. */
  onStatus(callback: (status: AppStatus) => void): () => void;
  /** The shell's look (light or dark, and its CSS tokens). See `applyTheme`. */
  theme(): Promise<Theme>;
  /** Calls `callback` whenever the shell's look changes. */
  onTheme(callback: (theme: Theme) => void): () => void;
  /** Stops listening to the shell (tests; a page that goes away needs nothing). */
  close(): void;
}

/** A lease must be renewed within 250 ms; every 100 ms survives one late beat. */
const RENEW_MS = 100;

function noShell(): FaberError {
  return new FaberError({
    code: 'no-shell',
    message: "The Faber shell didn't answer.",
    hint: 'Open this app from Faber (its window in the shell); its page does nothing on its own.',
  });
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null;
}

/** Connects to the shell around this page. Most apps use the ready-made `faber` instead. */
export function connect(options: ConnectOptions = {}): Faber {
  const parent: Endpoint = options.parent ?? (globalThis as unknown as { parent: Endpoint }).parent;
  const self: Inbox = options.self ?? (globalThis as unknown as Inbox);
  const timeoutMs = options.timeoutMs ?? 5000;

  let nextId = 1;
  const pending = new Map<
    number,
    {
      resolve: (v: unknown) => void;
      reject: (e: FaberError) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  const subscriptions = new Map<
    number,
    { onReading: (r: Reading) => void; onError?: (e: FaberError) => void }
  >();
  const drives = new Map<string, DriveHandle>();
  const statusWatchers = new Set<(s: AppStatus) => void>();
  const themeWatchers = new Set<(t: Theme) => void>();
  let hello: Promise<Hello> | null = null;
  let closed = false;

  function request<T extends RequestType>(
    req: Extract<Request, { type: T }>,
  ): Promise<ReplyValues[T]> {
    if (closed) return Promise.reject(noShell());
    const id = nextId++;
    return new Promise<ReplyValues[T]>((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(noShell());
      }, timeoutMs);
      pending.set(id, { resolve: resolve as (v: unknown) => void, reject, timer });
      // The shell's origin can't be named from an opaque-origin page; nothing sent here is secret,
      // and only the shell may frame an app's page (frame-ancestors).
      parent.postMessage({ faber: PROTOCOL, id, ...req }, '*');
    });
  }

  function onMessage(event: MessageEvent): void {
    if (event.source !== (parent as unknown)) return;
    const m = event.data as ShellMessage;
    if (!isObject(m) || m.faber !== PROTOCOL) return;
    if ('re' in m) {
      const p = pending.get(m.re);
      if (!p) return;
      pending.delete(m.re);
      clearTimeout(p.timer);
      if (m.ok) p.resolve(m.value);
      else p.reject(new FaberError(m.error));
      return;
    }
    switch (m.event) {
      case 'signal':
        subscriptions.get(m.subscription)?.onReading(m.reading);
        break;
      case 'signal-ended': {
        const s = subscriptions.get(m.subscription);
        subscriptions.delete(m.subscription);
        s?.onError?.(new FaberError(m.error));
        break;
      }
      case 'status':
        for (const w of [...statusWatchers]) w(m.status);
        break;
      case 'theme':
        for (const w of [...themeWatchers]) w(m.theme);
        break;
      case 'lease-ended':
        drives.get(key(m.role, m.index))?.lost(new FaberError(m.error));
        break;
    }
  }
  self.addEventListener('message', onMessage);

  function watch(
    source: Source,
    callback: (r: Reading) => void,
    opts: SubscribeOptions = {},
  ): () => void {
    let id: number | null = null;
    let stopped = false;
    const req: Extract<Request, { type: 'subscribe' }> = { type: 'subscribe', source };
    if (opts.maxHz !== undefined) req.maxHz = opts.maxHz;
    request<'subscribe'>(req).then(
      ({ subscription }) => {
        if (stopped) {
          void request<'unsubscribe'>({ type: 'unsubscribe', subscription }).catch(() => undefined);
          return;
        }
        id = subscription;
        subscriptions.set(subscription, { onReading: callback, onError: opts.onError });
      },
      (e: FaberError) => opts.onError?.(e),
    );
    return () => {
      if (stopped) return;
      stopped = true;
      if (id === null) return;
      subscriptions.delete(id);
      void request<'unsubscribe'>({ type: 'unsubscribe', subscription: id }).catch(() => undefined);
    };
  }

  function key(role: string, index: number): string {
    return `${role}#${index}`;
  }

  class DriveHandle implements Drive {
    ended = false;
    private readonly callbacks: ((e: FaberError) => void)[] = [];
    private timer: ReturnType<typeof setInterval> | null;
    private lastSent = Date.now();

    constructor(
      readonly role: string,
      readonly index: number,
    ) {
      // The page's own heartbeat keeps the lease: a page that hangs lets it lapse, and the part stops.
      this.timer = setInterval(() => this.beat(), RENEW_MS);
    }

    async command(command: Command): Promise<CommandResult> {
      if (this.ended) throw this.endedError();
      this.lastSent = Date.now();
      return request<'command'>({ type: 'command', role: this.role, index: this.index, command });
    }

    async release(): Promise<void> {
      if (this.ended) return;
      this.finish();
      await request<'release'>({ type: 'release', role: this.role, index: this.index });
    }

    onEnd(callback: (error: FaberError) => void): void {
      this.callbacks.push(callback);
    }

    lost(error: FaberError): void {
      if (this.ended) return;
      this.finish();
      for (const c of this.callbacks) c(error);
    }

    private beat(): void {
      if (Date.now() - this.lastSent < RENEW_MS - 5) return;
      this.lastSent = Date.now();
      request<'renew'>({ type: 'renew', role: this.role, index: this.index }).catch(
        (e: FaberError) => {
          if (['lease-ended', 'lease-required', 'no-shell'].includes(e.code)) this.lost(e);
        },
      );
    }

    private finish(): void {
      this.ended = true;
      if (this.timer !== null) clearInterval(this.timer);
      this.timer = null;
      drives.delete(key(this.role, this.index));
    }

    private endedError(): FaberError {
      return new FaberError({
        code: 'lease-ended',
        message: `This app no longer drives "${this.role}".`,
        hint: 'Call faber.drive() again to drive it.',
      });
    }
  }

  const api: Faber = {
    hello() {
      hello ??= request<'hello'>({ type: 'hello' }).catch((e: unknown) => {
        hello = null;
        throw e;
      });
      return hello;
    },
    roles: () => request<'roles'>({ type: 'roles' }),
    latest: (role, opts = {}) =>
      request<'latest'>({ type: 'latest', source: { role, index: opts.index ?? 0 } }),
    subscribe: (role, callback, opts = {}) =>
      watch({ role, index: opts.index ?? 0 }, callback, opts),
    published: (name) => request<'latest'>({ type: 'latest', source: { published: name } }),
    subscribePublished: (name, callback, opts = {}) => watch({ published: name }, callback, opts),
    async drive(role, opts = {}) {
      const index = opts.index ?? 0;
      const existing = drives.get(key(role, index));
      if (existing) return existing;
      await request<'drive'>({ type: 'drive', role, index });
      const handle = new DriveHandle(role, index);
      drives.set(key(role, index), handle);
      return handle;
    },
    status: () => request<'status'>({ type: 'status' }),
    onStatus(callback) {
      statusWatchers.add(callback);
      return () => void statusWatchers.delete(callback);
    },
    theme: () => request<'theme'>({ type: 'theme' }),
    onTheme(callback) {
      themeWatchers.add(callback);
      return () => void themeWatchers.delete(callback);
    },
    close() {
      closed = true;
      self.removeEventListener('message', onMessage);
      for (const [, p] of pending) {
        clearTimeout(p.timer);
        p.reject(noShell());
      }
      pending.clear();
      for (const d of [...drives.values()]) d.lost(noShell());
    },
  };
  return api;
}

/** Something with a `style` that takes CSS custom properties: `document.documentElement`. */
export interface Styled {
  style: { setProperty(name: string, value: string): void };
}

/**
 * Makes the page look like the shell: sets every theme token (`--surface`, `--text`, `--accent`,
 * `--font-ui`, …) on `root` and its `color-scheme`. Use the tokens in your CSS:
 * `background: var(--surface); color: var(--text);`.
 */
export function applyTheme(theme: Theme, root?: Styled): void {
  const el =
    root ??
    (globalThis as unknown as { document: { documentElement: Styled } }).document.documentElement;
  for (const [name, value] of Object.entries(theme.tokens)) {
    if (/^--[a-z0-9-]+$/.test(name)) el.style.setProperty(name, value);
  }
  el.style.setProperty('color-scheme', theme.scheme);
}

let shared: Faber | null = null;

/** The connection to the shell around this page, made on first use. */
export const faber: Faber = new Proxy({} as Faber, {
  get(_, name: keyof Faber) {
    shared ??= connect();
    const value = shared[name];
    return typeof value === 'function' ? value.bind(shared) : value;
  },
});
