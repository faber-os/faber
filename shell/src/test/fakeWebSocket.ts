/**
 * A WebSocket stand-in for tests. Each instance records what the code under test sent; the test
 * drives it with open(), receive() and drop().
 */
export class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;

  /** Every socket created, oldest first. */
  static instances: FakeWebSocket[] = [];

  static reset(): void {
    FakeWebSocket.instances = [];
  }

  static latest(): FakeWebSocket {
    const ws = FakeWebSocket.instances.at(-1);
    if (!ws) throw new Error('No FakeWebSocket was created');
    return ws;
  }

  readonly url: string;
  readyState = FakeWebSocket.CONNECTING;
  binaryType: BinaryType = 'blob';
  sent: (string | Uint8Array)[] = [];

  onopen: ((ev: Event) => void) | null = null;
  onclose: ((ev: CloseEvent) => void) | null = null;
  onerror: ((ev: Event) => void) | null = null;
  onmessage: ((ev: MessageEvent) => void) | null = null;

  constructor(url: string | URL) {
    this.url = String(url);
    FakeWebSocket.instances.push(this);
  }

  send(data: string | ArrayBufferLike | ArrayBufferView): void {
    if (this.readyState !== FakeWebSocket.OPEN) throw new Error('send() while not open');
    if (typeof data === 'string') this.sent.push(data);
    else if (ArrayBuffer.isView(data))
      this.sent.push(new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice());
    else this.sent.push(new Uint8Array(data).slice());
  }

  close(): void {
    if (this.readyState === FakeWebSocket.CLOSED) return;
    this.readyState = FakeWebSocket.CLOSED;
    this.onclose?.({ code: 1000, reason: '', wasClean: true } as CloseEvent);
  }

  // ---- test controls ----

  open(): void {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.(new Event('open'));
  }

  receive(data: string | ArrayBuffer): void {
    this.onmessage?.({ data } as MessageEvent);
  }

  /** The connection dies without the client asking (core stopped, network gone). */
  drop(): void {
    this.readyState = FakeWebSocket.CLOSED;
    this.onclose?.({ code: 1006, reason: '', wasClean: false } as CloseEvent);
  }
}
