import { t } from '../i18n';
import { decode, encode } from '@msgpack/msgpack';
import type { ErrorBody } from './generated/ErrorBody';
import type { Reading } from './generated/Reading';
import type { ClientMessage } from './generated/ClientMessage';
import type { ServerMessage } from './generated/ServerMessage';

/** MessagePack is the default; JSON is for debugging (D26, docs/api.md). */
export type WireFormat = 'msgpack' | 'json';

/** A frame from the core that the shell could not understand. */
export class WireError extends Error {
  readonly body: ErrorBody;
  constructor(message: string) {
    super(message);
    this.name = 'WireError';
    this.body = {
      code: 'bad-message',
      message,
      hint: t('errors.wire.hint'),
    };
  }
}

// Keyed by the generated union, so a message type added to the core's contract without being
// listed here is a compile error, not a message the shell silently drops.
const SERVER_TYPE_LIST: Record<ServerMessage['type'], true> = {
  signal: true,
  parts: true,
  ports: true,
  log: true,
  'serial-opened': true,
  'serial-data': true,
  'serial-closed': true,
  'serial-recording': true,
  'serial-recording-stopped': true,
  settings: true,
  'files-changed': true,
  leases: true,
  'lease-granted': true,
  'lease-ended': true,
  estop: true,
  packages: true,
  apps: true,
  world: true,
  'world-changed': true,
  flash: true,
  error: true,
};
const SERVER_TYPES = new Set<string>(Object.keys(SERVER_TYPE_LIST));

export function encodeClientMessage(msg: ClientMessage, format: WireFormat): string | Uint8Array {
  if (format === 'msgpack') return encode(msg);
  return JSON.stringify(msg, (_key, value: unknown) =>
    value instanceof Uint8Array ? Array.from(value) : value,
  );
}

export function decodeServerMessage(
  data: string | ArrayBuffer | ArrayBufferView,
  format: WireFormat,
): ServerMessage {
  let raw: unknown;
  try {
    if (format === 'json') {
      const text = typeof data === 'string' ? data : new TextDecoder().decode(toBytes(data));
      raw = JSON.parse(text);
    } else {
      if (typeof data === 'string') throw new Error('text frame in msgpack mode');
      raw = decode(toBytes(data));
    }
  } catch (e) {
    throw new WireError(t('errors.wire.unreadable', { reason: (e as Error).message }));
  }
  return normaliseServerMessage(raw);
}

function toBytes(data: ArrayBuffer | ArrayBufferView): Uint8Array {
  return ArrayBuffer.isView(data)
    ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
    : new Uint8Array(data);
}

/**
 * Make both wire formats look the same to the rest of the shell: `null` in a scan's ranges
 * becomes `Infinity` (JSON has no infinity), byte arrays become `Uint8Array`, and an optional
 * field sent as `null`/nil is dropped, because the generated types say "absent", never "null".
 */
function normaliseServerMessage(raw: unknown): ServerMessage {
  if (!isRecord(raw) || typeof raw.type !== 'string' || !SERVER_TYPES.has(raw.type)) {
    throw new WireError(t('errors.wire.unknownType'));
  }
  dropNullFields(raw);
  const msg = raw as ServerMessage;
  if (msg.type === 'serial-data' && !(msg.data instanceof Uint8Array)) {
    msg.data = Uint8Array.from(msg.data as unknown as number[]);
  }
  if (msg.type === 'signal') msg.reading = normaliseReading(msg.reading);
  return msg;
}

function normaliseReading(reading: Reading): Reading {
  switch (reading.capability) {
    case 'laser-scan@1': {
      const r = reading.reading;
      r.ranges = (r.ranges as (number | null)[]).map((v) => (v === null ? Infinity : v));
      return reading;
    }
    default:
      return reading;
  }
}

/** Remove `null` object fields, in place. Array items are kept: `null` there means no return. */
export function dropNullFields(v: unknown): void {
  if (Array.isArray(v)) {
    for (const item of v) if (typeof item === 'object' && item !== null) dropNullFields(item);
    return;
  }
  if (!isRecord(v) || v instanceof Uint8Array) return;
  for (const key of Object.keys(v)) {
    if (v[key] === null) delete v[key];
    else if (typeof v[key] === 'object') dropNullFields(v[key]);
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** `?wire=json` in the page address switches the WebSocket to JSON for debugging. */
export function wireFormatFromSearch(search: string): WireFormat {
  return new URLSearchParams(search).get('wire') === 'json' ? 'json' : 'msgpack';
}
