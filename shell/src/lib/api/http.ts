import { t } from '../i18n';
import type { ApiError } from './generated/ApiError';
import type { BoardCheck } from './generated/BoardCheck';
import type { HeaderPin } from './generated/HeaderPin';
import type { I2cScan } from './generated/I2cScan';
import type { ConnectorInfo } from './generated/ConnectorInfo';
import type { ConnectorProblem } from './generated/ConnectorProblem';
import type { PackageInfo } from './generated/PackageInfo';
import type { PackagePreview } from './generated/PackagePreview';
import type { AppInfo } from './generated/AppInfo';
import type { AppPackagePreview } from './generated/AppPackagePreview';
import type { AppState } from './generated/AppState';
import type { RoleParts } from './generated/RoleParts';
import type { SignalValue } from './generated/SignalValue';
import type { ErrorBody } from './generated/ErrorBody';
import type { NewPart } from './generated/NewPart';
import type { PartInfo } from './generated/PartInfo';
import type { PortInfo } from './generated/PortInfo';
import type { SystemInfo } from './generated/SystemInfo';
import type { DeveloperChange } from './generated/DeveloperChange';
import type { FileRoot } from './generated/FileRoot';
import type { FolderListing } from './generated/FolderListing';
import type { Settings } from './generated/Settings';
import type { AboutInfo } from './generated/AboutInfo';
import type { BackupInfo } from './generated/BackupInfo';
import type { StorageInfo } from './generated/StorageInfo';
import type { TranslationInfo } from './generated/TranslationInfo';
import type { Command } from './generated/Command';
import type { CommandResult } from './generated/CommandResult';
import type { EstopStatus } from './generated/EstopStatus';
import type { FlashBoard } from './generated/FlashBoard';
import type { ArduinoInfo } from './generated/ArduinoInfo';
import type { ArduinoTools } from './generated/ArduinoTools';
import type { BoardCodeInfo } from './generated/BoardCodeInfo';
import type { CompileRequest } from './generated/CompileRequest';
import type { CompileResult } from './generated/CompileResult';
import type { NewSketchRequest } from './generated/NewSketchRequest';
import type { SketchRef } from './generated/SketchRef';
import type { UploadRequest } from './generated/UploadRequest';
import type { UploadResult } from './generated/UploadResult';
import type { FlashRequest } from './generated/FlashRequest';
import type { FlashResult } from './generated/FlashResult';
import type { LeaseGrant } from './generated/LeaseGrant';
import type { LeaseInfo } from './generated/LeaseInfo';
import type { Pattern } from './generated/Pattern';
import type { Simulation } from './generated/Simulation';
import type { WorldInfo } from './generated/WorldInfo';
import type { WorldView } from './generated/WorldView';
import type { NewToken } from './generated/NewToken';
import type { NewUser } from './generated/NewUser';
import type { PasswordChange } from './generated/PasswordChange';
import type { SessionInfo } from './generated/SessionInfo';
import type { TokenCreated } from './generated/TokenCreated';
import type { TokenInfo } from './generated/TokenInfo';
import type { UserInfo } from './generated/UserInfo';
import type { UserRole } from './generated/UserRole';
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
  getAbout(): Promise<AboutInfo>;
  getConnectors(): Promise<ConnectorInfo[]>;
  /** Installed connectors the core couldn't load, each with why and what to do. */
  getConnectorProblems(): Promise<ConnectorProblem[]>;
  /** Installed connector packages (docs/api.md, "Packages"). */
  getPackages(): Promise<PackageInfo[]>;
  /**
   * Checks a package file in full and says what it would install, installing nothing: a
   * connector's package answers a `PackagePreview`, an app's an `AppPackagePreview`.
   */
  previewPackage(
    file: Uint8Array | Blob,
    name: string,
  ): Promise<PackagePreview | AppPackagePreview>;
  /** Installs a package file; `name` is remembered as where it came from. */
  installPackage(file: Uint8Array | Blob, name: string): Promise<PackageInfo>;
  /** `409 package-in-use` while a part uses its connector (the message names them). */
  uninstallPackage(id: string): Promise<void>;
  /** The installed apps (docs/api.md, "Apps"). */
  getApps(): Promise<AppInfo[]>;
  /** Turns an app on (`true`) or off; off ends its leases before it answers. */
  setAppEnabled(id: string, enabled: boolean): Promise<AppInfo>;
  /** Exactly these parts for these roles; roles left out get none. */
  setAppRoles(id: string, roles: Record<string, RoleParts>): Promise<AppInfo>;
  /** Sets one key of an app's saved state, or removes it with `null`. */
  setAppState(id: string, key: string, value: string | null): Promise<AppState>;
  /** The newest reading of a signal (`part/name`, or an app's `app:<id>/<name>`). */
  getSignal(address: string): Promise<SignalValue>;
  getPorts(): Promise<PortInfo[]>;
  /**
   * Asks a free port whether a FaberLink board is on it (a few seconds). Only when the user asks:
   * opening a port restarts most Arduino boards. `409 port-claimed` for a port in use.
   */
  checkBoard(port: string): Promise<BoardCheck>;
  /** A safe scan of an I²C bus (reads only), only when the user asks (docs/api.md, "I²C scan"). */
  scanI2c(port: string): Promise<I2cScan>;
  /** The Raspberry Pi's 40-pin header, pin by pin. */
  getHeader(): Promise<HeaderPin[]>;
  /** The boards the Arduino flasher can program (docs/api.md, "The Arduino flasher"). */
  getFlasherBoards(): Promise<FlashBoard[]>;
  /**
   * Puts a program on the board on a port. Answers once it is done (seconds, up to a minute);
   * the `flash` topic follows it. Replaces the program on the board: only on a user's confirm.
   */
  flash(port: string, req: FlashRequest): Promise<FlashResult>;
  /** The Arduino editor (docs/api.md, "The Arduino editor"). */
  getArduino(): Promise<ArduinoInfo>;
  /** Downloads and installs arduino-cli and the AVR core. Only on a user's confirm. */
  installArduinoTools(): Promise<ArduinoTools>;
  newSketch(req: NewSketchRequest): Promise<SketchRef>;
  compileSketch(req: CompileRequest): Promise<CompileResult>;
  /** Compiles and flashes. Replaces the program on the board: only on a user's confirm. */
  uploadSketch(req: UploadRequest): Promise<UploadResult>;
  /** What is on the board, as far as can be told without touching it. */
  getBoardCode(port: string): Promise<BoardCodeInfo>;
  /** Reads the program back and compares; resets the board. Only on a user's request. */
  checkBoardCode(port: string, stopPart: boolean): Promise<BoardCodeInfo>;
  openFlashed(id: string, copy: boolean): Promise<SketchRef>;
  getParts(): Promise<PartInfo[]>;
  addPart(part: NewPart): Promise<PartInfo>;
  removePart(name: string): Promise<void>;
  /** Stops the part and frees its port until it is resumed (`paused = true` in parts.toml). */
  pausePart(name: string): Promise<PartInfo>;
  resumePart(name: string): Promise<PartInfo>;

  getRoots(): Promise<FileRoot[]>;
  listFolder(path: string): Promise<FolderListing>;
  readFile(path: string): Promise<Uint8Array>;
  /** The whole file, of any size (`readFile` stops at 16 MB), e.g. a package to install. */
  downloadFile(path: string): Promise<Blob>;
  /**
   * Writes the file (at most 16 MB). `replace` (the default) creates or overwrites it; `create`
   * only creates it, and is refused with `409 already-exists` when it is already there.
   */
  writeFile(path: string, bytes: Uint8Array | Blob, mode?: WriteMode): Promise<void>;
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
  /** Every setting back to its default (whole-machine files off too). Parts and files stay. */
  resetSettings(): Promise<Settings>;

  getStorage(): Promise<StorageInfo>;
  /** Saves a backup in `home:/Backups`. `507 disk-full` if it won't fit. */
  createBackup(includeRecordings: boolean): Promise<BackupInfo>;
  /** Saves this machine's setup: a backup without the user's files or the machine's name. */
  shareSetup(): Promise<BackupInfo>;
  /**
   * Restores a backup from the home folder; checked first (`422 config-invalid` …). `users` also
   * replaces this machine's users and API tokens with the backup's, and signs everyone out.
   */
  restoreBackup(path: string, users?: boolean): Promise<void>;

  // Users and signing in (docs/auth.md).
  /** Who this browser is (public: the sign-in screen asks it first). */
  getSession(): Promise<SessionInfo>;
  /** `401 sign-in-failed`, `429 too-many-attempts`. Sets the session cookie. */
  signIn(name: string, password: string): Promise<SessionInfo>;
  signOut(): Promise<void>;
  /** The first admin, only while there are no users (`409 already-set-up`). Signs them in. */
  setup(name: string, password: string): Promise<SessionInfo>;
  getUsers(): Promise<UserInfo[]>;
  addUser(user: NewUser): Promise<UserInfo>;
  removeUser(name: string): Promise<void>;
  setUserRole(name: string, role: UserRole): Promise<void>;
  /** Your own needs `current`; an admin may set anyone's without it. */
  setPassword(name: string, change: PasswordChange): Promise<void>;
  /** Your API tokens (an admin's lists everyone's), never the tokens themselves. */
  getTokens(): Promise<TokenInfo[]>;
  /** The token's secret is in the answer, this once. */
  createToken(token: NewToken): Promise<TokenCreated>;
  revokeToken(id: string): Promise<void>;

  /** The translations installed in the machine's `config/translations` (docs/translations.md). */
  getTranslations(): Promise<TranslationInfo[]>;
  /** One translation file. `422 translation-invalid` (with the line) if it is broken. */
  getTranslation(code: string): Promise<Record<string, unknown>>;

  // Driving (docs/api.md, "Driving"). Every lease and e-stop change also arrives on the
  // `leases` and `estop` WebSocket topics.
  /** `409 lease-held` (the message says who), `423 estop-engaged`, `400 not-leasable` … */
  requestLease(part: string, holder: string, takeOver?: boolean): Promise<LeaseGrant>;
  renewLease(part: string, lease: string): Promise<LeaseGrant>;
  /** The part gets its safe state. */
  releaseLease(part: string, lease: string): Promise<void>;
  getLeases(): Promise<LeaseInfo[]>;
  /** Renews the lease too. Values beyond the part's limits are clamped, not refused. */
  command(part: string, lease: string, command: Command): Promise<CommandResult>;
  getEstop(): Promise<EstopStatus>;
  /** Always allowed and never refused. The shell says it was the shell. */
  engageEstop(reason: string): Promise<EstopStatus>;
  releaseEstop(): Promise<EstopStatus>;
  /** A manual simulated sensor's value pattern. `409 not-manual`, `400 not-a-sensor` … */
  getSimulation(part: string): Promise<Simulation>;
  putSimulation(part: string, pattern: Pattern): Promise<Simulation>;

  /** The prebuilt worlds, in menu order (docs/api.md, "The simulated world"). */
  getWorlds(): Promise<WorldInfo[]>;
  /** The current world: its geometry, start and the base's footprint. */
  getWorld(): Promise<WorldView>;
  /** Switches the world (written to parts.toml); the machine starts again at its start. */
  putWorld(name: string): Promise<WorldView>;
  /** Puts the machine back at its start, at rest. */
  resetWorld(): Promise<void>;
}

export type WriteMode = 'replace' | 'create';

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
        message: t('errors.coreUnreachable.message'),
        hint: t('errors.coreUnreachable.hint'),
      });
    }
  }

  /** A JSON answer, or `undefined` for an empty success (204, or a 201 without a body). */
  function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    return answer<T>(method, path, body === undefined ? undefined : { json: body });
  }

  async function answer<T>(method: string, path: string, body: Body): Promise<T> {
    const res = await send(method, path, body);
    if (res.status === 204) return undefined as T;
    const text = await res.text();
    if (!res.ok) throw failure(res, method, path, text);
    if (text === '') return undefined as T;
    const json = parseJson(text);
    if (json === undefined) {
      throw new CoreError(res.status, {
        code: 'bad-response',
        message: t('errors.badJson.message', { method, path }),
        hint: t('errors.badJson.hint'),
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
      const body: ErrorBody = { code: err.code, message: err.message, hint: err.hint ?? '' };
      if (err.needs) body.needs = err.needs;
      return new CoreError(res.status, body);
    }
    return new CoreError(res.status, {
      code: 'bad-response',
      message: t('errors.badStatus.message', {
        method,
        path,
        status: `${res.status} ${res.statusText}`.trim(),
      }),
      hint: t('errors.badStatus.hint'),
    });
  }

  const q = (path: string) => `?path=${encodeURIComponent(path)}`;
  const named = (name: string) => `name=${encodeURIComponent(name)}`;
  const partPath = (name: string) => `/parts/${encodeURIComponent(name)}`;

  return {
    getSystem: () => request('GET', '/system'),
    getAbout: () => request('GET', '/about'),
    getConnectors: () => request('GET', '/connectors'),
    getConnectorProblems: () => request('GET', '/connectors/problems'),
    getPackages: () => request('GET', '/packages'),
    previewPackage: (file, name) =>
      answer('POST', `/packages?mode=preview&${named(name)}`, { raw: file }),
    installPackage: (file, name) => answer('POST', `/packages?${named(name)}`, { raw: file }),
    uninstallPackage: (id) => request('DELETE', `/packages/${encodeURIComponent(id)}`),
    getApps: () => request('GET', '/apps'),
    setAppEnabled: (id, enabled) =>
      request('POST', `/apps/${encodeURIComponent(id)}/${enabled ? 'enable' : 'disable'}`),
    setAppRoles: (id, roles) => request('PUT', `/apps/${encodeURIComponent(id)}/roles`, { roles }),
    setAppState: (id, key, value) =>
      request('PUT', `/apps/${encodeURIComponent(id)}/state`, { key, value }),
    getSignal: (address) =>
      request('GET', `/signals/${address.split('/').map(encodeURIComponent).join('/')}`),
    getPorts: () => request('GET', '/ports'),
    checkBoard: (port) => request('POST', '/ports/check-board', { port }),
    scanI2c: (port) => request('POST', '/ports/scan-i2c', { port }),
    getHeader: () => request('GET', '/ports/header'),
    getFlasherBoards: () => request('GET', '/flasher/boards'),
    flash: (port, req) => request('POST', `/ports/${encodeURIComponent(port)}/flash`, req),
    getArduino: () => request('GET', '/arduino'),
    installArduinoTools: () => request('POST', '/arduino/tools/install'),
    newSketch: (req) => request('POST', '/arduino/sketches', req),
    compileSketch: (req) => request('POST', '/arduino/compile', req),
    uploadSketch: (req) => request('POST', '/arduino/upload', req),
    getBoardCode: (port) => request('GET', `/arduino/boards/${encodeURIComponent(port)}/code`),
    checkBoardCode: (port, stopPart) =>
      request(
        'POST',
        `/arduino/boards/${encodeURIComponent(port)}/check`,
        stopPart ? { stopPart } : {},
      ),
    openFlashed: (id, copy) =>
      request('POST', `/arduino/flashed/${encodeURIComponent(id)}/open`, copy ? { copy } : {}),
    getParts: () => request('GET', '/parts'),
    addPart: (part) => request('POST', '/parts', part),
    removePart: (name) => request('DELETE', `/parts/${encodeURIComponent(name)}`),
    pausePart: (name) => request('POST', `/parts/${encodeURIComponent(name)}/pause`),
    resumePart: (name) => request('POST', `/parts/${encodeURIComponent(name)}/resume`),

    getRoots: () => request('GET', '/files/roots'),
    listFolder: (path) => request('GET', `/files/list${q(path)}`),
    readFile: (path) => requestBytes('GET', `/files/content${q(path)}`),
    downloadFile: async (path) => {
      const res = await send('GET', `/files/content${q(path)}&download=1`, undefined);
      if (!res.ok) throw failure(res, 'GET', '/files/content', await res.text());
      return res.blob();
    },
    writeFile: async (path, bytes, mode = 'replace') => {
      const m = mode === 'create' ? '&mode=create' : '';
      await requestBytes('PUT', `/files/content${q(path)}${m}`, { raw: bytes });
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
    resetSettings: () => request('POST', '/settings/reset'),

    getStorage: () => request('GET', '/storage'),
    createBackup: (includeRecordings) => request('POST', '/backups', { includeRecordings }),
    shareSetup: () => request('POST', '/backups/setup'),
    restoreBackup: (path, users) =>
      request('POST', '/backups/restore', users ? { path, users } : { path }),

    getSession: () => request('GET', '/session'),
    signIn: (name, password) => request('POST', '/session', { name, password }),
    signOut: () => request('DELETE', '/session'),
    setup: (name, password) => request('POST', '/setup', { name, password }),
    getUsers: () => request('GET', '/users'),
    addUser: (user) => request('POST', '/users', user),
    removeUser: (name) => request('DELETE', `/users/${encodeURIComponent(name)}`),
    setUserRole: (name, role) =>
      request('PUT', `/users/${encodeURIComponent(name)}/role`, { role }),
    setPassword: (name, change) =>
      request('PUT', `/users/${encodeURIComponent(name)}/password`, change),
    getTokens: () => request('GET', '/tokens'),
    createToken: (token) => request('POST', '/tokens', token),
    revokeToken: (id) => request('DELETE', `/tokens/${encodeURIComponent(id)}`),

    getTranslations: () => request('GET', '/translations'),
    getTranslation: (code) => request('GET', `/translations/${encodeURIComponent(code)}`),

    requestLease: (part, holder, takeOver) =>
      request('POST', `${partPath(part)}/lease`, takeOver ? { holder, takeOver } : { holder }),
    renewLease: (part, lease) => request('POST', `${partPath(part)}/lease/renew`, { lease }),
    releaseLease: (part, lease) =>
      request('DELETE', `${partPath(part)}/lease?lease=${encodeURIComponent(lease)}`),
    getLeases: () => request('GET', '/leases'),
    command: (part, lease, command) =>
      request('POST', `${partPath(part)}/command`, { lease, command }),
    getEstop: () => request('GET', '/estop'),
    engageEstop: (reason) => request('POST', '/estop', { reason, source: 'shell' }),
    releaseEstop: () => request('POST', '/estop/release', { source: 'shell' }),
    getSimulation: (part) => request('GET', `${partPath(part)}/simulation`),
    putSimulation: (part, pattern) => request('PUT', `${partPath(part)}/simulation`, { pattern }),
    getWorlds: () => request('GET', '/worlds'),
    getWorld: () => request('GET', '/world'),
    putWorld: (name) => request('PUT', '/world', { name }),
    resetWorld: () => request('POST', '/world/reset'),
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
