import { t } from '../i18n';
import type { ErrorBody } from './generated/ErrorBody';
import type { ClientMessage } from './generated/ClientMessage';
import type { ServerMessage } from './generated/ServerMessage';
import type { Topic } from './generated/Topic';
import { decodeServerMessage, encodeClientMessage, WireError, type WireFormat } from './wire';

export type ConnectionStatus = 'connecting' | 'connected' | 'reconnecting' | 'closed';

/** The part of the browser WebSocket that CoreSocket uses, so tests can pass a fake. */
export interface WebSocketLike {
  readyState: number;
  binaryType: BinaryType;
  send(data: string | ArrayBufferLike | ArrayBufferView): void;
  close(): void;
  onopen: ((ev: Event) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
}

export type WebSocketCtor = new (url: string) => WebSocketLike;

export interface SocketOptions {
  url: string;
  format: WireFormat;
  WebSocketImpl?: WebSocketCtor;
}

export interface SubscriptionHandlers {
  onMessage(msg: ServerMessage): void;
  onError?(error: ErrorBody): void;
}

export interface SerialHandlers {
  /** The core opened the port. Data only arrives after this. */
  onOpened(port: string): void;
  onData(data: Uint8Array, tUs: number): void;
  /** The session is over: closed by us, by the core (with its reason) or by a lost connection. */
  onClosed(reason?: string): void;
  onError(error: ErrorBody): void;
  /** Recording started (docs/api.md, "Recordings"). Failures arrive as `onError`. */
  onRecording?(path: string): void;
  onRecordingStopped?(path: string, bytes: number): void;
}

export interface SerialSession {
  send(data: Uint8Array): void;
  close(): void;
  /** Save everything received from now on to `home:/Recordings/<name>.fbcap`. */
  startRecording(name: string): void;
  stopRecording(): void;
}

/** What a message between an app's window and its logic carries. */
export type AppMessageData = string | Uint8Array;

export interface AppWindowHandlers {
  /** The link is open; `window` is the window's id as the app's logic sees it. */
  onOpened(window: number): void;
  /** A message from the app's logic. */
  onMessage(kind: string, data: AppMessageData): void;
  /** The core answered `app-window-drive`: the window now drives `role`, or `error` says why not. */
  onDrive(role: string, index: number, error: ErrorBody | null): void;
  /** The window no longer drives `role` through the logic (`stopped`, `heartbeat-lost`, …). */
  onDriveEnded(role: string, index: number, reason: string): void;
  /** The link is over: the core closed it (the app was removed), or the connection went. */
  onClosed(reason?: string): void;
  onError(error: ErrorBody): void;
}

/** An app window's live link to its app's logic (docs/apps.md §12). */
export interface AppWindowSession {
  send(kind: string, data: AppMessageData): void;
  heartbeat(): void;
  drive(role: string, index: number): void;
  release(role: string, index: number): void;
  /** The user pressed the shell's Stop for this window. */
  stop(): void;
  close(): void;
}

const OPEN = 1;
const BACKOFF_START_MS = 500;
const BACKOFF_MAX_MS = 10_000;

/** Wait before reconnect attempt `attempt` (0-based): 0.5 s, 1 s, 2 s … at most 10 s. */
export function backoffDelay(attempt: number): number {
  return Math.min(BACKOFF_MAX_MS, BACKOFF_START_MS * 2 ** attempt);
}

/** The core's WebSocket address for a page served from `location`. */
export function wsUrl(location: { protocol: string; host: string }, format: WireFormat): string {
  const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${scheme}//${location.host}/api/v0/ws?format=${format}`;
}

/**
 * The one WebSocket to the core (docs/api.md). Subscriptions survive reconnects: they are sent
 * again on every new connection. Serial sessions do not, because the core closes the port when
 * the connection goes.
 */
export class CoreSocket {
  status: ConnectionStatus = 'closed';

  private readonly url: string;
  private readonly format: WireFormat;
  private readonly WebSocketImpl: WebSocketCtor;
  private ws: WebSocketLike | null = null;
  private nextId = 1;
  private attempt = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private wanted = false;
  private readonly subs = new Map<number, { topic: Topic; handlers: SubscriptionHandlers }>();
  /**
   * Open serial sessions. After `serial-close` a session stays here (`closing`) until the core's
   * `serial-closed`, so what the core still sends about it (the `serial-recording-stopped` that
   * reports a recording the close ended) reaches its handlers.
   */
  private readonly serials = new Map<
    number,
    { handlers: SerialHandlers; opened: boolean; closing: boolean }
  >();
  /** Open app window links. Like serial sessions, they end with the connection. */
  private readonly appWindows = new Map<number, { handlers: AppWindowHandlers; opened: boolean }>();
  private readonly statusListeners = new Set<(s: ConnectionStatus) => void>();
  private readonly errorListeners = new Set<(e: ErrorBody) => void>();

  constructor(opts: SocketOptions) {
    this.url = opts.url;
    this.format = opts.format;
    this.WebSocketImpl = opts.WebSocketImpl ?? (WebSocket as unknown as WebSocketCtor);
  }

  onStatus(listener: (s: ConnectionStatus) => void): () => void {
    this.statusListeners.add(listener);
    return () => this.statusListeners.delete(listener);
  }

  /** Errors that belong to no subscription, and frames that could not be read. */
  onError(listener: (e: ErrorBody) => void): () => void {
    this.errorListeners.add(listener);
    return () => this.errorListeners.delete(listener);
  }

  connect(): void {
    if (this.wanted) return;
    this.wanted = true;
    this.setStatus('connecting');
    this.open();
  }

  close(): void {
    this.wanted = false;
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onclose = null;
      ws.close();
    }
    this.endSerials(t('errors.socket.closedByShell'));
    this.endAppWindows(t('errors.socket.closedByShell'));
    this.setStatus('closed');
  }

  subscribe(topic: Topic, handlers: SubscriptionHandlers): () => void {
    const id = this.nextId++;
    this.subs.set(id, { topic, handlers });
    this.send({ type: 'subscribe', id, topic });
    return () => {
      if (!this.subs.delete(id)) return;
      this.send({ type: 'unsubscribe', id });
    };
  }

  openSerial(port: string, baud: number, handlers: SerialHandlers): SerialSession {
    const id = this.nextId++;
    if (!this.isOpen()) {
      handlers.onError({
        code: 'not-connected',
        message: t('errors.socket.notConnected.message'),
        hint: t('errors.socket.notConnected.hint'),
      });
      handlers.onClosed();
      return { send: () => {}, close: () => {}, startRecording: () => {}, stopRecording: () => {} };
    }
    this.serials.set(id, { handlers, opened: false, closing: false });
    this.send({ type: 'serial-open', id, port, baud });
    const whileOpen = (fn: () => void) => {
      const s = this.serials.get(id);
      if (s?.opened && !s.closing) fn();
    };
    return {
      send: (data) => whileOpen(() => this.send({ type: 'serial-send', id, data })),
      startRecording: (name) =>
        whileOpen(() => this.send({ type: 'serial-record-start', id, name })),
      stopRecording: () => whileOpen(() => this.send({ type: 'serial-record-stop', id })),
      close: () => {
        const s = this.serials.get(id);
        if (!s || s.closing) return;
        this.send({ type: 'serial-close', id });
        // Closed as far as the window is concerned; the entry waits for the core's last word.
        if (s.opened && this.isOpen()) s.closing = true;
        else this.serials.delete(id);
        s.handlers.onClosed();
      },
    };
  }

  /**
   * Opens an app window's live link to its app's logic. It doesn't survive a lost connection:
   * the core ends what the window drove when the connection goes, and `onClosed` says so.
   */
  openAppWindow(app: string, handlers: AppWindowHandlers): AppWindowSession {
    const id = this.nextId++;
    const none: AppWindowSession = {
      send: () => {},
      heartbeat: () => {},
      drive: () => {},
      release: () => {},
      stop: () => {},
      close: () => {},
    };
    if (!this.isOpen()) {
      handlers.onError({
        code: 'not-connected',
        message: t('errors.socket.notConnected.message'),
        hint: t('errors.socket.notConnected.hint'),
      });
      handlers.onClosed();
      return none;
    }
    this.appWindows.set(id, { handlers, opened: false });
    this.send({ type: 'app-window-open', id, app });
    const whileOpen = (fn: () => void) => {
      if (this.appWindows.get(id)?.opened) fn();
    };
    return {
      send: (kind, data) =>
        whileOpen(() =>
          this.send({
            type: 'app-window-send',
            id,
            kind,
            data: typeof data === 'string' ? { text: data } : { bytes: data },
          }),
        ),
      heartbeat: () => whileOpen(() => this.send({ type: 'app-window-heartbeat', id })),
      drive: (role, index) =>
        whileOpen(() => this.send({ type: 'app-window-drive', id, role, index })),
      release: (role, index) =>
        whileOpen(() => this.send({ type: 'app-window-release', id, role, index })),
      stop: () => whileOpen(() => this.send({ type: 'app-window-stop', id })),
      close: () => {
        if (!this.appWindows.delete(id)) return;
        this.send({ type: 'app-window-close', id });
      },
    };
  }

  private isOpen(): boolean {
    return this.ws !== null && this.ws.readyState === OPEN;
  }

  private send(msg: ClientMessage): void {
    // Not connected: subscriptions are (re)sent on open, everything else needs a live session.
    if (!this.isOpen()) return;
    this.ws!.send(encodeClientMessage(msg, this.format));
  }

  private open(): void {
    const ws = new this.WebSocketImpl(this.url);
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    ws.onopen = () => {
      this.attempt = 0;
      this.setStatus('connected');
      for (const [id, { topic }] of this.subs) this.send({ type: 'subscribe', id, topic });
    };
    ws.onmessage = (ev) => this.handleFrame(ev.data as string | ArrayBuffer);
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.endSerials(t('errors.socket.lost'));
      this.endAppWindows(t('errors.socket.lost'));
      if (!this.wanted) return;
      this.setStatus('reconnecting');
      const delay = backoffDelay(this.attempt++);
      this.retryTimer = setTimeout(() => {
        this.retryTimer = null;
        if (this.wanted) this.open();
      }, delay);
    };
    // A failed connection also fires close, which is where the retry happens.
    ws.onerror = () => {};
  }

  private handleFrame(data: string | ArrayBuffer): void {
    let msg: ServerMessage;
    try {
      msg = decodeServerMessage(data, this.format);
    } catch (e) {
      if (e instanceof WireError) return this.emitError(e.body);
      throw e;
    }
    if (msg.type === 'error' && msg.id === undefined) return this.emitError(msg.error);
    const id = msg.id as number;

    const serial = this.serials.get(id);
    if (serial?.closing) {
      // Closed by us: only the recording report and the end of the session still matter.
      if (msg.type === 'serial-recording-stopped')
        serial.handlers.onRecordingStopped?.(msg.path, msg.bytes);
      else if (msg.type === 'serial-closed') this.serials.delete(id);
      return;
    }
    if (serial) {
      const h = serial.handlers;
      if (msg.type === 'serial-opened') {
        serial.opened = true;
        h.onOpened(msg.port);
      } else if (msg.type === 'serial-data') h.onData(msg.data, msg.tUs);
      else if (msg.type === 'serial-recording') h.onRecording?.(msg.path);
      else if (msg.type === 'serial-recording-stopped') h.onRecordingStopped?.(msg.path, msg.bytes);
      else if (msg.type === 'serial-closed') {
        this.serials.delete(id);
        h.onClosed(msg.reason);
      } else if (msg.type === 'error') {
        // docs/api.md: an error before `serial-opened` means the port never opened; after it,
        // only `serial-closed` ends the session.
        h.onError(msg.error);
        if (!serial.opened) {
          this.serials.delete(id);
          h.onClosed();
        }
      }
      return;
    }

    const win = this.appWindows.get(id);
    if (win) return this.appWindowFrame(id, win, msg);

    const sub = this.subs.get(id);
    if (!sub) return; // A late message for something already unsubscribed.
    if (msg.type === 'error') {
      if (sub.handlers.onError) sub.handlers.onError(msg.error);
      else this.emitError(msg.error);
      return;
    }
    sub.handlers.onMessage(msg);
  }

  private appWindowFrame(
    id: number,
    win: { handlers: AppWindowHandlers; opened: boolean },
    msg: ServerMessage,
  ): void {
    const h = win.handlers;
    const data = (d: { text: string } | { bytes: Uint8Array }): AppMessageData =>
      'text' in d ? d.text : d.bytes;
    switch (msg.type) {
      case 'app-window-opened':
        win.opened = true;
        h.onOpened(msg.window);
        break;
      case 'app-window-message':
        h.onMessage(msg.kind, data(msg.data));
        break;
      case 'app-window-driving':
        h.onDrive(msg.role, msg.index, null);
        break;
      case 'app-window-drive-refused':
        h.onDrive(msg.role, msg.index, msg.error);
        break;
      case 'app-window-drive-ended':
        h.onDriveEnded(msg.role, msg.index, msg.reason);
        break;
      case 'app-window-closed':
        this.appWindows.delete(id);
        h.onClosed(msg.reason);
        break;
      case 'error':
        h.onError(msg.error);
        // An error before `app-window-opened` means the link never opened.
        if (!win.opened) {
          this.appWindows.delete(id);
          h.onClosed();
        }
        break;
    }
  }

  private endAppWindows(reason: string): void {
    const all = [...this.appWindows.values()];
    this.appWindows.clear();
    for (const w of all) w.handlers.onClosed(reason);
  }

  private endSerials(reason: string): void {
    const all = [...this.serials.values()];
    this.serials.clear();
    for (const s of all) if (!s.closing) s.handlers.onClosed(reason);
  }

  private emitError(e: ErrorBody): void {
    for (const l of this.errorListeners) l(e);
  }

  private setStatus(s: ConnectionStatus): void {
    if (this.status === s) return;
    this.status = s;
    for (const l of this.statusListeners) l(s);
  }
}
