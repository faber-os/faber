import type { ClientMessage } from './generated/ClientMessage';
import type { ErrorBody } from './generated/ErrorBody';
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
}

export interface SerialSession {
  send(data: Uint8Array): void;
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
  private readonly serials = new Map<number, { handlers: SerialHandlers; opened: boolean }>();
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
    this.endSerials('The shell closed its connection to the core.');
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
        message: 'The shell is not connected to the core right now.',
        hint: 'Wait until the taskbar says Connected, then open the port again.',
      });
      handlers.onClosed();
      return { send: () => {}, close: () => {} };
    }
    this.serials.set(id, { handlers, opened: false });
    this.send({ type: 'serial-open', id, port, baud });
    return {
      send: (data) => {
        if (this.serials.get(id)?.opened) this.send({ type: 'serial-send', id, data });
      },
      close: () => {
        const s = this.serials.get(id);
        if (!s) return;
        this.serials.delete(id);
        this.send({ type: 'serial-close', id });
        s.handlers.onClosed();
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
      this.endSerials('The connection to the core was lost.');
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
    if (serial) {
      const h = serial.handlers;
      if (msg.type === 'serial-opened') {
        serial.opened = true;
        h.onOpened(msg.port);
      } else if (msg.type === 'serial-data') h.onData(msg.data, msg.tUs);
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

    const sub = this.subs.get(id);
    if (!sub) return; // A late message for something already unsubscribed.
    if (msg.type === 'error') {
      if (sub.handlers.onError) sub.handlers.onError(msg.error);
      else this.emitError(msg.error);
      return;
    }
    sub.handlers.onMessage(msg);
  }

  private endSerials(reason: string): void {
    const all = [...this.serials.values()];
    this.serials.clear();
    for (const s of all) s.handlers.onClosed(reason);
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
