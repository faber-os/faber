import type { ApiError } from './generated/ApiError';
import type { ConnectorInfo } from './generated/ConnectorInfo';
import type { ErrorBody } from './generated/ErrorBody';
import type { NewPart } from './generated/NewPart';
import type { PartInfo } from './generated/PartInfo';
import type { PortInfo } from './generated/PortInfo';
import type { SystemInfo } from './generated/SystemInfo';
import { dropNullFields } from './wire';

/** Something the core (or the way to it) said no to, with what to try next. */
export class CoreError extends Error {
  /** HTTP status, or 0 when the core could not be reached at all. */
  readonly status: number;
  readonly body: ErrorBody;
  constructor(status: number, body: ErrorBody) {
    super(body.message);
    this.name = 'CoreError';
    this.status = status;
    this.body = body;
  }
}

export interface CoreHttp {
  getSystem(): Promise<SystemInfo>;
  getConnectors(): Promise<ConnectorInfo[]>;
  getPorts(): Promise<PortInfo[]>;
  getParts(): Promise<PartInfo[]>;
  addPart(part: NewPart): Promise<PartInfo>;
  removePart(name: string): Promise<void>;
}

export function createHttp(
  fetchFn: typeof fetch = fetch.bind(globalThis),
  base = '/api/v0',
): CoreHttp {
  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    let res: Response;
    try {
      res = await fetchFn(base + path, {
        method,
        headers: body === undefined ? undefined : { 'content-type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      throw new CoreError(0, {
        code: 'core-unreachable',
        message: 'The shell could not reach the Faber core.',
        hint: 'Check that faber-core is running and that this computer can reach the machine, then try again.',
      });
    }
    if (res.status === 204) return undefined as T;
    const text = await res.text();
    const json = parseJson(text);
    if (!res.ok) {
      const err = (json as Partial<ApiError> | undefined)?.error;
      if (err && typeof err.code === 'string' && typeof err.message === 'string') {
        throw new CoreError(res.status, {
          code: err.code,
          message: err.message,
          hint: err.hint ?? '',
        });
      }
      throw new CoreError(res.status, {
        code: 'bad-response',
        message:
          `The core answered ${method} ${path} with ${res.status} ${res.statusText}`.trim() + '.',
        hint: 'Try again. If it keeps happening, look in the Logs tool for what the core reported.',
      });
    }
    if (json === undefined) {
      throw new CoreError(res.status, {
        code: 'bad-response',
        message: `The core's answer to ${method} ${path} was not valid JSON.`,
        hint: 'The shell and the core may be different versions. Reload the page.',
      });
    }
    // Same rule as the WebSocket: an optional field is absent, never null.
    dropNullFields(json);
    return json as T;
  }

  return {
    getSystem: () => request('GET', '/system'),
    getConnectors: () => request('GET', '/connectors'),
    getPorts: () => request('GET', '/ports'),
    getParts: () => request('GET', '/parts'),
    addPart: (part) => request('POST', '/parts', part),
    removePart: (name) => request('DELETE', `/parts/${encodeURIComponent(name)}`),
  };
}

function parseJson(text: string): unknown {
  if (text === '') return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}
