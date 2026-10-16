/**
 * The message protocol between an app's UI and the Faber shell, version 1 (docs/apps.md §9).
 *
 * An app's UI runs in a sandboxed iframe with an opaque origin: it can't reach the core's API or
 * the network. It talks to the shell window around it with `postMessage`, and only in these
 * messages. The shell checks every request against the app's roles before it does anything.
 *
 * Types only: this file is shared by the SDK (`faber-shell.ts`) and the shell's host
 * (`shell/src/lib/state/appHost.ts`), so both sides agree on every message.
 */

/** Every message carries `faber: 1`. A later version changes this number. */
export type ProtocolVersion = 1;

/** Readings and commands are the core's own JSON, tagged with their capability (docs/api.md). */
export interface Reading {
  capability: string;
  reading: unknown;
}

export interface Command {
  capability: string;
  command: unknown;
}

/** What went wrong, in the house style: what happened, and what to try next. */
export interface ErrorBody {
  /** Stable and kebab-case, e.g. `unknown-role`. */
  code: string;
  message: string;
  hint: string;
}

/** Where a reading comes from: one of the app's roles, or one of its own derived signals. */
export type Source =
  /** A role, by name. `index` picks one part of a `count = "1.."` or `"0.."` role (default 0). */
  | { role: string; index?: number }
  /** One of the app's `[[publishes]]` signals, by name (`app:<id>/<name>`). */
  | { published: string };

/** One role the app requires, as the app sees it: never which part fills it. */
export interface RoleInfo {
  role: string;
  capability: string;
  /** The app may drive it. */
  command: boolean;
  /** `1`, `1..`, or for a role the app runs without, `0..1` or `0..`. */
  count: string;
  /** How many parts fill it now (0: none picked yet). */
  parts: number;
  /**
   * Where each of those parts sits on the machine, by index: readings are in the part's own
   * frame, and an app applies its mount to put them in the machine's (x forward, y left; m, rad).
   */
  mounts: Mount[];
}

/** Where a part sits on the machine (m, rad), as in parts.toml. */
export interface Mount {
  x: number;
  y: number;
  z: number;
  roll: number;
  pitch: number;
  yaw: number;
}

/** A derived signal the app publishes. */
export interface PublishedInfo {
  name: string;
  capability: string;
}

/** Whether the app is turned on, and how it runs (the same states as a part's). */
export interface AppStatus {
  enabled: boolean;
  state: 'invalid' | 'conflict' | 'starting' | 'running' | 'degraded' | 'failed' | 'stopped';
  /** What is wrong, when something is. */
  message?: string;
}

/** The shell's look, so an app can look native: CSS custom properties (`--surface`, …). */
export interface Theme {
  scheme: 'light' | 'dark';
  tokens: Record<string, string>;
}

/** What the shell answers to `hello`: everything an app needs to start. */
export interface Hello {
  app: { id: string; name: string; version: string };
  roles: RoleInfo[];
  published: PublishedInfo[];
  status: AppStatus;
  theme: Theme;
}

/** What a message between an app's window and its logic carries: text, or bytes. */
export type MessageData = string | Uint8Array;

export interface CommandResult {
  /** What the part got (values beyond its limits are clamped). */
  applied: Command;
  clamped: boolean;
}

/** Requests from the app, each answered by exactly one reply with the same `id`. */
export type Request =
  | { type: 'hello' }
  | { type: 'roles' }
  | { type: 'status' }
  | { type: 'theme' }
  /** The newest reading, or `null` before there is one. */
  | { type: 'latest'; source: Source }
  /** Readings as they come, at most `maxHz` a second (the shell caps it). */
  | { type: 'subscribe'; source: Source; maxHz?: number }
  | { type: 'unsubscribe'; subscription: number }
  /** Take the lease on a `command = true` role's part, after the user's go-ahead (D46). */
  | { type: 'drive'; role: string; index?: number }
  /** Keep the lease alive without a command: the app must send this or a command every 100 ms. */
  | { type: 'renew'; role: string; index?: number }
  | { type: 'command'; role: string; index?: number; command: Command }
  | { type: 'release'; role: string; index?: number }
  /** One value of the app's saved state (shared with its logic), or `null` when it isn't set. */
  | { type: 'state-get'; key: string }
  /** Saves one value (`null` removes it). Keys are 1-256 bytes; keys and values 64 KB together. */
  | { type: 'state-set'; key: string; value: string | null }
  | { type: 'state-keys' }
  /** From now on, a `state` event for every change, whoever made it (the logic, any window). */
  | { type: 'state-watch' }
  /**
   * A message for the app's logic (docs/apps.md §12): `kind` is 1-64 characters of `a-z`, `0-9`
   * and `-`; kind and data at most 4 KB together, at most 50 a second. Never saved: dropped while
   * the logic isn't running.
   */
  | { type: 'message-send'; kind: string; data: MessageData }
  /** From now on, a `message` event for every message the logic sends this window. */
  | { type: 'message-watch' }
  /**
   * Drive a `drive = "window"` role through the app's logic, after the user's go-ahead (D46):
   * the logic may drive it from now until `drive-via-release`, the user's Stop, the window
   * closing, or no `heartbeat` for 300 ms.
   */
  | { type: 'drive-via'; role: string; index?: number }
  | { type: 'drive-via-release'; role: string; index?: number }
  /** The page is alive: send one at least every 300 ms while driving through the logic. */
  | { type: 'heartbeat' };

export type RequestType = Request['type'];

/** What each request answers with. */
export interface ReplyValues {
  hello: Hello;
  roles: RoleInfo[];
  status: AppStatus;
  theme: Theme;
  latest: Reading | null;
  subscribe: { subscription: number };
  unsubscribe: null;
  drive: null;
  renew: null;
  command: CommandResult;
  release: null;
  'state-get': string | null;
  'state-set': null;
  'state-keys': string[];
  'state-watch': null;
  'message-send': null;
  'message-watch': null;
  'drive-via': null;
  'drive-via-release': null;
  heartbeat: null;
}

export type RequestMessage = { faber: ProtocolVersion; id: number } & Request;

export type Reply =
  | { faber: ProtocolVersion; re: number; ok: true; value: unknown }
  | { faber: ProtocolVersion; re: number; ok: false; error: ErrorBody };

/** Things the shell says without being asked. */
export type Event =
  | { faber: ProtocolVersion; event: 'signal'; subscription: number; reading: Reading }
  /** A subscription ended: its part went away, or the app lost the role. */
  | { faber: ProtocolVersion; event: 'signal-ended'; subscription: number; error: ErrorBody }
  | { faber: ProtocolVersion; event: 'theme'; theme: Theme }
  | { faber: ProtocolVersion; event: 'status'; status: AppStatus }
  /** A key of the app's state changed (after `state-watch`); `value` is `null` when removed. */
  | { faber: ProtocolVersion; event: 'state'; key: string; value: string | null }
  /** A message from the app's logic (after `message-watch`). */
  | { faber: ProtocolVersion; event: 'message'; kind: string; data: MessageData }
  /**
   * A `drive-via` ended without the page letting go: `reason` is `stopped` (the user pressed
   * Stop), `heartbeat-lost`, `app-stopped`, `connection-lost`, …
   */
  | {
      faber: ProtocolVersion;
      event: 'drive-ended';
      role: string;
      index: number;
      reason: string;
      error: ErrorBody;
    }
  /** A lease ended without the app letting go: `reason` as the core says it (`expired`, `estop`, …). */
  | {
      faber: ProtocolVersion;
      event: 'lease-ended';
      role: string;
      index: number;
      reason: string;
      error: ErrorBody;
    };

/** Every message the shell sends to an app. */
export type ShellMessage = Reply | Event;
