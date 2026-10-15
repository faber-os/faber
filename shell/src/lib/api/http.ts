import type { ApiError } from './generated/ApiError';
import type { ConnectorInfo } from './generated/ConnectorInfo';
import type { ErrorBody } from './generated/ErrorBody';
import type { NewPart } from './generated/NewPart';
import type { PartInfo } from './generated/PartInfo';
import type { PortInfo } from './generated/PortInfo';
import type { SystemInfo } from './generated/SystemInfo';
import type { DeveloperChange } from './generated/DeveloperChange';
import type { FileRoot } from './generated/FileRoot';
import type { FolderListing } from './generated/FolderListing';
import type { Settings } from './generated/Settings';
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

  getRoots(): Promise<FileRoot[]>;
  listFolder(path: string): Promise<FolderListing>;
  readFile(path: string): Promise<Uint8Array>;
  /** Creates or overwrites the file (at most 16 MB). */
  writeFile(path: string, bytes: Uint8Array | Blob): Promise<void>;
  createFolder(path: string): Promise<void>;
  move(from: string, to: string): Promise<void>;
  trash(paths: string[]): Promise<void>;
  restore(paths: string[]): Promise<void>;
  /** For good: only for paths in the Trash, or in `machine:`. */
  deleteForGood(paths: string[]): Promise<void>;
  emptyTrash(): Promise<void>;
  /** Copies the LD19 sample into Recordings. `409 already-exists` if it is already there. */
  addSamples(): Promise<{ path: string }>;
  /** Where the browser can download a file from (a plain link, no fetch). */
  downloadUrl(path: string): string;

  getSettings(): Promise<Settings>;
  putSettings(settings: Settings): Promise<Settings>;
  setDeveloper(change: DeveloperChange): Promise<Settings>;
}

/** What one request sends: JSON for everything except file contents, which go as raw bytes. */
type Body = { json: unknown } | { raw: Uint8Array | Blob } | undefined;

export function createHttp(
  fetchFn: typeof fetch = fetch.bind(globalThis),
  base = '/api/v0',
): CoreHttp {
  async function send(method: string, path: string, body: Body): Promise<Response> {
    try {
      if (body === undefined) return await fetchFn(base + path, { method });
      if ('json' in body) {
        return await fetchFn(base + path, {
          method,
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(body.json),
        });
      }
      return await fetchFn(base + path, {
        method,
        headers: { 'content-type': 'application/octet-stream' },
        body: body.raw as BodyInit,
      });
    } catch {
      throw new CoreError(0, {
        code: 'core-unreachable',
        message: 'The shell could not reach the Faber core.',
        hint: 'Check that faber-core is running and that this computer can reach the machine, then try again.',
      });
    }
  }

  /** A JSON answer, or `undefined` for an empty success (204, or a 201 without a body). */
  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await send(method, path, body === undefined ? undefined : { json: body });
    if (res.status === 204) return undefined as T;
    const text = await res.text();
    if (!res.ok) throw failure(res, method, path, text);
    if (text === '') return undefined as T;
    const json = parseJson(text);
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

  async function requestBytes(method: string, path: string, body?: Body): Promise<Uint8Array> {
    const res = await send(method, path, body);
    if (!res.ok) throw failure(res, method, path, await res.text());
    return new Uint8Array(await res.arrayBuffer());
  }

  function failure(res: Response, method: string, path: string, text: string): CoreError {
    const err = (parseJson(text) as Partial<ApiError> | undefined)?.error;
    if (err && typeof err.code === 'string' && typeof err.message === 'string') {
      return new CoreError(res.status, {
        code: err.code,
        message: err.message,
        hint: err.hint ?? '',
      });
    }
    return new CoreError(res.status, {
      code: 'bad-response',
      message:
        `The core answered ${method} ${path} with ${res.status} ${res.statusText}`.trim() + '.',
      hint: 'Try again. If it keeps happening, look in the Logs tool for what the core reported.',
    });
  }

  const q = (path: string) => `?path=${encodeURIComponent(path)}`;

  return {
    getSystem: () => request('GET', '/system'),
    getConnectors: () => request('GET', '/connectors'),
    getPorts: () => request('GET', '/ports'),
    getParts: () => request('GET', '/parts'),
    addPart: (part) => request('POST', '/parts', part),
    removePart: (name) => request('DELETE', `/parts/${encodeURIComponent(name)}`),

    getRoots: () => request('GET', '/files/roots'),
    listFolder: (path) => request('GET', `/files/list${q(path)}`),
    readFile: (path) => requestBytes('GET', `/files/content${q(path)}`),
    writeFile: async (path, bytes) => {
      await requestBytes('PUT', `/files/content${q(path)}`, { raw: bytes });
    },
    createFolder: (path) => request('POST', '/files/folder', { path }),
    move: (from, to) => request('POST', '/files/move', { from, to }),
    trash: (paths) => request('POST', '/files/trash', { paths }),
    restore: (paths) => request('POST', '/files/restore', { paths }),
    deleteForGood: (paths) => request('POST', '/files/delete', { paths }),
    emptyTrash: () => request('POST', '/files/empty-trash'),
    addSamples: () => request('POST', '/files/samples'),
    downloadUrl: (path) => `${base}/files/content${q(path)}&download=1`,

    getSettings: () => request('GET', '/settings'),
    putSettings: (settings) => request('PUT', '/settings', settings),
    setDeveloper: (change) => request('POST', '/settings/developer', change),
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
