import { CoreError, type CoreHttp, type WriteMode } from '../api/http';
import type {
  AppWindowHandlers,
  AppWindowSession,
  CoreSocket,
  SerialHandlers,
  SerialSession,
} from '../api/socket';
import { RingBuffer } from './ringBuffer';
import { locale } from '../i18n';
import {
  BUILT_IN,
  languageChoices,
  type LanguageChoice,
  type MachineTranslation,
} from '../i18n/languages';
import type {
  BoardCheck,
  ConnectionStatus,
  ConnectorInfo,
  ConnectorProblem,
  PackageInfo,
  PackagePreview,
  AppInfo,
  AppPackagePreview,
  AppState,
  RoleParts,
  SignalValue,
  ErrorBody,
  LogEntry,
  NewPart,
  PartInfo,
  PortInfo,
  PortProblem,
  Reading,
  Settings,
  SystemInfo,
  WireFormat,
  DeveloperChange,
  FileRoot,
  FolderListing,
  AboutInfo,
  BackupInfo,
  ClockFormat,
  StorageInfo,
  Command,
  CommandResult,
  EstopStatus,
  FlashBoard,
  FlashRequest,
  FlashResult,
  LeaseEndReason,
  LeaseGrant,
  LeaseInfo,
  Notice,
  Pattern,
  Simulation,
  WorldInfo,
  WorldView,
} from './types';
import type { ServerMessage } from '../api/generated/ServerMessage';
import { canMove } from './driving';
import { SessionState } from './session.svelte';
import type { Result } from './result';

export const LOG_CAPACITY = 1000;
const LOG_FLUSH_MS = 50;
const MAX_NOTICES = 5;

/** A notice on screen, with a key to dismiss it by. */
export type ShownNotice = { key: number; notice: Notice };

export type { Result } from './result';

export interface CoreDeps {
  http: CoreHttp;
  socket: CoreSocket;
  format: WireFormat;
  /** Batches log updates; the default redraws the log at most every 50 ms. */
  schedule?: (fn: () => void) => void;
  /** Wall-clock time (ms since the epoch). */
  now?: () => number;
}

/** The machine in the simulated world now: a `world` message (docs/api.md). */
export type WorldUpdate = Omit<Extract<ServerMessage, { type: 'world' }>, 'type' | 'id'>;

/** The Arduino flasher's progress on a port: a `flash` message (docs/api.md). */
export type FlashProgress = Omit<Extract<ServerMessage, { type: 'flash' }>, 'type' | 'id'>;

export interface WorldHandlers {
  /** 20 times a second while watched. */
  onState(state: WorldUpdate): void;
  /** The world was switched; read its geometry again. */
  onChanged(name: string): void;
  onError?(error: ErrorBody): void;
}

export interface SignalHandlers {
  onReading(reading: Reading): void;
  onError(error: ErrorBody): void;
}

/**
 * Everything the shell knows about the core, kept live. `null` means "not known yet" (still
 * loading), which the UI shows differently from an empty list.
 */
export class CoreState {
  connection = $state<ConnectionStatus>('closed');
  parts = $state.raw<PartInfo[] | null>(null);
  ports = $state.raw<PortInfo[] | null>(null);
  /** Things plugged in that never became a port (docs/api.md, "Port problems"). */
  portProblems = $state.raw<PortProblem[]>([]);
  connectors = $state.raw<ConnectorInfo[] | null>(null);
  connectorsError = $state.raw<ErrorBody | null>(null);
  /** Installed connectors the core couldn't load, with why (docs/api.md). */
  connectorProblems = $state.raw<ConnectorProblem[]>([]);
  /**
   * Installed connector packages, live from the `packages` topic (so an install in another tab
   * shows here too); `null` until the core has sent them.
   */
  packages = $state.raw<PackageInfo[] | null>(null);
  /** Installed apps, live from the `apps` topic (docs/api.md, "Apps"); `null` until sent. */
  apps = $state.raw<AppInfo[] | null>(null);
  system = $state.raw<SystemInfo | null>(null);
  systemError = $state.raw<ErrorBody | null>(null);
  /**
   * True when the core was built from a different API than this shell (docs/api.md, "System").
   * Every request may then fail in confusing ways, so the shell asks for a restart instead.
   */
  apiMismatch = $derived(this.system !== null && this.system.api !== FABER_API);
  logs = $state.raw<LogEntry[]>([]);
  /** Shared by every browser and the attached screen; `null` until the core has sent them. */
  settings = $state.raw<Settings | null>(null);
  /**
   * How wall-clock times are shown (D42): in the zone the settings pick, else the machine's own
   * (from the system info), else (before either is known) the browser's.
   */
  wallClock = $derived<{ timeZone: string | undefined; clock: ClockFormat }>({
    timeZone: this.settings?.time.timeZone || this.system?.timeZone || undefined,
    clock: this.settings?.time.clock ?? '24h',
  });
  /**
   * Translations installed on the machine (`config/translations`), with their files once read.
   * Built-in ones (src/lib/i18n/locales) need no loading.
   */
  machineTranslations = $state.raw<MachineTranslation[]>([]);
  /** Translation files on the machine that can't be used, with why. */
  translationProblems = $derived(this.machineTranslations.filter((m) => m.problem));
  /** The newest error that belonged to no window. */
  lastError = $state.raw<ErrorBody | null>(null);
  /** Who drives what, live from the `leases` topic (docs/api.md, "Driving"). */
  leases = $state.raw<LeaseInfo[]>([]);
  /**
   * Notices to show as toasts, newest last (the `notices` topic): one per battery, so a battery
   * going from low to critical replaces its toast. At most five; dismissed by the user.
   */
  notices = $state.raw<ShownNotice[]>([]);
  /** The e-stop, live from its topic; `null` until the core has said. */
  estop = $state.raw<EstopStatus | null>(null);
  /** Parts with something that moves (a `planar-base@1` or `motor@1`). */
  movableParts = $derived((this.parts ?? []).filter(canMove));
  /**
   * D46: Faber asks once per session (this page) before anything moves the motors. It is not a
   * setting yet; a switch to turn it off comes later.
   */
  drivingConfirmed = $state(false);

  readonly format: WireFormat;
  /** Who uses this shell and what they may do (docs/auth.md). */
  readonly auth: SessionState;

  private readonly http: CoreHttp;
  private readonly socket: CoreSocket;
  private readonly schedule: (fn: () => void) => void;
  private readonly now: () => number;
  private readonly logBuffer = new RingBuffer<LogEntry>(LOG_CAPACITY);
  private logFlushPending = false;
  private clockOffsetMs: number | null = null;
  private stops: (() => void)[] = [];
  private wantedLanguage = 'en';
  private readonly fileWatchers = new Set<(folders: string[]) => void>();
  private readonly leaseEndWatchers = new Set<(l: LeaseInfo, reason: LeaseEndReason) => void>();

  constructor(deps: CoreDeps) {
    this.http = deps.http;
    this.auth = new SessionState(deps.http);
    this.socket = deps.socket;
    this.format = deps.format;
    this.schedule = deps.schedule ?? ((fn) => void setTimeout(fn, LOG_FLUSH_MS));
    this.now = deps.now ?? Date.now;
  }

  start(): void {
    const s = this.socket;
    this.stops.push(
      s.onStatus((status) => {
        // The logs topic replays recent entries on every new connection; start from them.
        if (status === 'connected') this.clearLogs();
        this.connection = status;
        // Every connection may be a different core (restarted, rebuilt), so ask again each time.
        if (status === 'connected') void this.loadSystem();
      }),
      s.onError((e) => (this.lastError = e)),
      s.subscribe(
        { kind: 'parts' },
        {
          onMessage: (m) => {
            if (m.type === 'parts') this.parts = m.parts;
          },
        },
      ),
      s.subscribe(
        { kind: 'ports' },
        {
          onMessage: (m) => {
            if (m.type !== 'ports') return;
            this.ports = m.ports;
            this.portProblems = m.problems ?? [];
          },
        },
      ),
      s.subscribe(
        { kind: 'logs' },
        {
          onMessage: (m) => {
            if (m.type === 'log') this.pushLog(m.entry);
          },
        },
      ),
      s.subscribe(
        { kind: 'settings' },
        {
          onMessage: (m) => {
            if (m.type === 'settings') this.settings = m.settings;
          },
        },
      ),
      s.subscribe(
        { kind: 'files' },
        {
          onMessage: (m) => {
            if (m.type !== 'files-changed') return;
            for (const w of [...this.fileWatchers]) w(m.folders);
          },
        },
      ),
      s.subscribe(
        { kind: 'leases' },
        {
          onMessage: (m) => {
            if (m.type === 'leases') this.leases = m.leases;
            else if (m.type === 'lease-granted')
              this.leases = [...this.leases.filter((l) => l.part !== m.lease.part), m.lease];
            else if (m.type === 'lease-ended') {
              this.leases = this.leases.filter((l) => l.part !== m.lease.part);
              for (const w of [...this.leaseEndWatchers]) w(m.lease, m.reason);
            }
          },
        },
      ),
      s.subscribe(
        { kind: 'estop' },
        {
          onMessage: (m) => {
            if (m.type === 'estop') this.estop = m.status;
          },
        },
      ),
      s.subscribe(
        { kind: 'packages' },
        {
          onMessage: (m) => {
            if (m.type !== 'packages') return;
            this.packages = m.packages;
            // An install or uninstall, maybe in another tab: Add part's connectors change with it.
            void this.loadConnectors();
          },
        },
      ),
      s.subscribe(
        { kind: 'apps' },
        {
          onMessage: (m) => {
            if (m.type === 'apps') this.apps = m.apps;
          },
        },
      ),
      s.subscribe(
        { kind: 'notices' },
        {
          onMessage: (m) => {
            if (m.type === 'notice') this.addNotice(m.notice);
          },
        },
      ),
    );
    s.connect();
    void this.loadConnectors();
  }

  stop(): void {
    for (const stop of this.stops.splice(0)) stop();
    this.socket.close();
  }

  private noticeKey = 0;

  private addNotice(notice: Notice): void {
    const shown = { key: ++this.noticeKey, notice };
    const others = this.notices.filter((n) => n.notice.address !== notice.address);
    this.notices = [...others, shown].slice(-MAX_NOTICES);
  }

  /** Takes a notice's toast away. */
  dismissNotice(key: number): void {
    this.notices = this.notices.filter((n) => n.key !== key);
  }

  /**
   * Reads the machine's translations: the list, then each usable file. A core that can't list
   * them leaves the shell in English rather than failing (translations are optional).
   */
  async loadTranslations(): Promise<void> {
    const list = await this.call(() => this.http.getTranslations());
    if (!list.ok) return;
    this.machineTranslations = await Promise.all(
      list.value.map(async (info): Promise<MachineTranslation> => {
        if (info.problem) return info;
        const file = await this.call(() => this.http.getTranslation(info.code));
        return file.ok
          ? { ...info, translation: file.value }
          : { ...info, problem: file.error.message };
      }),
    );
  }

  /** English, then every translation the shell can show (D44: a picker only when more than one). */
  languages(englishName: string): LanguageChoice[] {
    return languageChoices(BUILT_IN, this.machineTranslations, englishName);
  }

  /**
   * Shows the shell in `code` (the `appearance.language` setting). A translation on the machine
   * is read first if needed; a language with no usable translation shows English.
   */
  async useLanguage(code: string): Promise<void> {
    this.wantedLanguage = code;
    const find = () => this.languages('English').find((c) => c.code === code);
    if (code !== 'en' && !find()) await this.loadTranslations();
    if (this.wantedLanguage !== code) return; // changed again while the file was loading
    const choice = find();
    locale.use(choice?.translation ? code : 'en', choice?.translation ?? null);
  }

  /** The connectors, and the installed ones that couldn't be loaded. */
  async loadConnectors(): Promise<void> {
    const [r, problems] = await Promise.all([
      this.call(() => this.http.getConnectors()),
      this.call(() => this.http.getConnectorProblems()),
    ]);
    if (r.ok) {
      this.connectors = r.value;
      this.connectorsError = null;
    } else this.connectorsError = r.error;
    if (problems.ok) this.connectorProblems = problems.value;
  }

  async loadPackages(): Promise<Result<PackageInfo[]>> {
    const r = await this.call(() => this.http.getPackages());
    if (r.ok) this.packages = r.value;
    return r;
  }

  /**
   * What a package file would install, checked in full by the core; nothing is installed. A
   * connector's package answers a `PackagePreview`, an app's an `AppPackagePreview`.
   */
  previewPackage(
    file: Uint8Array | Blob,
    name: string,
  ): Promise<Result<PackagePreview | AppPackagePreview>> {
    return this.call(() => this.http.previewPackage(file, name));
  }

  /** Installs a package; its connector is offered in Add part as soon as this returns. */
  async installPackage(file: Uint8Array | Blob, name: string): Promise<Result<PackageInfo>> {
    const r = await this.call(() => this.http.installPackage(file, name));
    if (r.ok) await Promise.all([this.loadConnectors(), this.loadPackages()]);
    return r;
  }

  /** Refused (`package-in-use`, naming the parts) while a part uses its connector. */
  async uninstallPackage(id: string): Promise<Result<void>> {
    const r = await this.call(() => this.http.uninstallPackage(id));
    if (r.ok) await Promise.all([this.loadConnectors(), this.loadPackages()]);
    return r;
  }

  /**
   * Turns an app on or off. Off ends its leases (and stops its logic) before this answers. The
   * list (`apps`) follows from the `apps` topic only: an answer can be older than what the topic
   * already said (the app went from starting to running meanwhile), so it never overwrites it.
   */
  setAppEnabled(id: string, enabled: boolean): Promise<Result<AppInfo>> {
    return this.call(() => this.http.setAppEnabled(id, enabled));
  }

  /** Exactly these parts for an app's roles; roles left out get none. The topic follows. */
  setAppRoles(id: string, roles: Record<string, RoleParts>): Promise<Result<AppInfo>> {
    return this.call(() => this.http.setAppRoles(id, roles));
  }

  /** Sets one key of an app's saved state (`null` removes it); its logic sees it at once. */
  setAppState(id: string, key: string, value: string | null): Promise<Result<AppState>> {
    return this.call(() => this.http.setAppState(id, key, value));
  }

  /**
   * An app's saved state, live (the `app-state` topic): all of it now, and again after missed
   * changes, then every change its logic or any window makes.
   */
  watchAppState(
    id: string,
    handlers: {
      onValues(values: Record<string, string>): void;
      onChange(key: string, value: string | null): void;
      onError(e: ErrorBody): void;
    },
  ): () => void {
    return this.socket.subscribe(
      { kind: 'app-state', app: id },
      {
        onMessage: (m) => {
          if (m.type === 'app-state') handlers.onValues(m.values);
          else if (m.type === 'app-state-changed') handlers.onChange(m.key, m.value ?? null);
        },
        onError: (e) => handlers.onError(e),
      },
    );
  }

  /** The newest reading of a signal, once (an app's derived signals too). */
  readSignal(address: string): Promise<Result<SignalValue>> {
    return this.call(() => this.http.getSignal(address));
  }

  async loadSystem(): Promise<void> {
    const r = await this.call(() => this.http.getSystem());
    if (r.ok) {
      this.clockOffsetMs = this.now() - r.value.timeUs / 1000;
      this.system = r.value;
      this.systemError = null;
    } else this.systemError = r.error;
  }

  addPart(part: NewPart): Promise<Result<PartInfo>> {
    return this.call(() => this.http.addPart(part));
  }

  removePart(name: string): Promise<Result<void>> {
    return this.call(() => this.http.removePart(name));
  }

  /** Asks a free port whether a FaberLink board is on it; a board found is suggested for it. */
  checkBoard(port: string): Promise<Result<BoardCheck>> {
    return this.call(() => this.http.checkBoard(port));
  }

  flasherBoards(): Promise<Result<FlashBoard[]>> {
    return this.call(() => this.http.getFlasherBoards());
  }

  /** Replaces the program on the board on `port`. Only ever after the user confirmed it. */
  flash(port: string, req: FlashRequest): Promise<Result<FlashResult>> {
    return this.call(() => this.http.flash(port, req));
  }

  /** The flasher's progress on every port, while watched. */
  watchFlash(onProgress: (p: FlashProgress) => void): () => void {
    return this.socket.subscribe(
      { kind: 'flash' },
      {
        onMessage: (m) => {
          if (m.type !== 'flash') return;
          const { type: _type, id: _id, ...progress } = m;
          onProgress(progress);
        },
      },
    );
  }

  /** Replace the settings. `developer` must be unchanged; use `setDeveloper` for that. */
  async saveSettings(settings: Settings): Promise<Result<Settings>> {
    const r = await this.call(() => this.http.putSettings(settings));
    if (r.ok) this.settings = r.value;
    return r;
  }

  /** Every setting back to its default. Parts and files are not touched. */
  async resetSettings(): Promise<Result<Settings>> {
    const r = await this.call(() => this.http.resetSettings());
    if (r.ok) this.settings = r.value;
    return r;
  }

  loadAbout(): Promise<Result<AboutInfo>> {
    return this.call(() => this.http.getAbout());
  }

  loadStorage(): Promise<Result<StorageInfo>> {
    return this.call(() => this.http.getStorage());
  }

  createBackup(includeRecordings: boolean): Promise<Result<BackupInfo>> {
    return this.call(() => this.http.createBackup(includeRecordings));
  }

  /** Settings → Storage → Share setup: this machine's parts, apps and settings, without its files. */
  shareSetup(): Promise<Result<BackupInfo>> {
    return this.call(() => this.http.shareSetup());
  }

  /** `users` also replaces the users and tokens with the backup's (everyone signs in again). */
  restoreBackup(path: string, users = false): Promise<Result<void>> {
    return this.call(() => this.http.restoreBackup(path, users));
  }

  async setDeveloper(change: DeveloperChange): Promise<Result<Settings>> {
    const r = await this.call(() => this.http.setDeveloper(change));
    if (r.ok) this.settings = r.value;
    return r;
  }

  /** Called with the folders whose contents changed through the core. Returns a stop function. */
  watchFiles(onChange: (folders: string[]) => void): () => void {
    this.fileWatchers.add(onChange);
    return () => this.fileWatchers.delete(onChange);
  }

  getRoots(): Promise<Result<FileRoot[]>> {
    return this.call(() => this.http.getRoots());
  }
  listFolder(path: string): Promise<Result<FolderListing>> {
    return this.call(() => this.http.listFolder(path));
  }
  readFile(path: string): Promise<Result<Uint8Array>> {
    return this.call(() => this.http.readFile(path));
  }
  /** The whole file, of any size, e.g. a package to install. */
  downloadFile(path: string): Promise<Result<Blob>> {
    return this.call(() => this.http.downloadFile(path));
  }
  /** `create` never overwrites: an existing file is refused with `already-exists`. */
  writeFile(
    path: string,
    bytes: Uint8Array | Blob,
    mode: WriteMode = 'replace',
  ): Promise<Result<void>> {
    return this.call(() => this.http.writeFile(path, bytes, mode));
  }
  createFolder(path: string): Promise<Result<void>> {
    return this.call(() => this.http.createFolder(path));
  }
  move(from: string, to: string): Promise<Result<void>> {
    return this.call(() => this.http.move(from, to));
  }
  trash(paths: string[]): Promise<Result<void>> {
    return this.call(() => this.http.trash(paths));
  }
  restore(paths: string[]): Promise<Result<void>> {
    return this.call(() => this.http.restore(paths));
  }
  deleteForGood(paths: string[]): Promise<Result<void>> {
    return this.call(() => this.http.deleteForGood(paths));
  }
  emptyTrash(): Promise<Result<void>> {
    return this.call(() => this.http.emptyTrash());
  }
  addSamples(): Promise<Result<{ path: string }>> {
    return this.call(() => this.http.addSamples());
  }
  downloadUrl(path: string): string {
    return this.http.downloadUrl(path);
  }

  /** Live readings of one signal, at most `maxHz` per second (latest-value delivery). */
  watchSignal(address: string, maxHz: number, handlers: SignalHandlers): () => void {
    return this.socket.subscribe(
      { kind: 'signal', address, maxHz },
      {
        onMessage: (m) => {
          if (m.type === 'signal') handlers.onReading(m.reading);
        },
        onError: handlers.onError,
      },
    );
  }

  /** The lease that covers `part`: on the part itself, or on a base built from it. */
  leaseOn(part: string): LeaseInfo | null {
    return this.leases.find((l) => l.part === part || l.locks.includes(part)) ?? null;
  }

  /** Called with every lease that ends, anyone's, and why. Returns a stop function. */
  watchLeaseEnds(onEnd: (lease: LeaseInfo, reason: LeaseEndReason) => void): () => void {
    this.leaseEndWatchers.add(onEnd);
    return () => this.leaseEndWatchers.delete(onEnd);
  }

  requestLease(part: string, holder: string, takeOver = false): Promise<Result<LeaseGrant>> {
    return this.call(() => this.http.requestLease(part, holder, takeOver));
  }
  renewLease(part: string, lease: string): Promise<Result<LeaseGrant>> {
    return this.call(() => this.http.renewLease(part, lease));
  }
  releaseLease(part: string, lease: string): Promise<Result<void>> {
    return this.call(() => this.http.releaseLease(part, lease));
  }
  command(part: string, lease: string, command: Command): Promise<Result<CommandResult>> {
    return this.call(() => this.http.command(part, lease, command));
  }

  /**
   * Engages the e-stop. The shell shows it engaged at once; the core never refuses it, so only a
   * core that can't be reached undoes that (and says so).
   */
  async engageEstop(reason: string): Promise<Result<EstopStatus>> {
    const before = this.estop;
    this.estop = { ...(before ?? {}), engaged: true };
    const r = await this.call(() => this.http.engageEstop(reason));
    if (r.ok) this.estop = r.value;
    else {
      this.estop = before;
      this.lastError = r.error;
    }
    return r;
  }

  async releaseEstop(): Promise<Result<EstopStatus>> {
    const r = await this.call(() => this.http.releaseEstop());
    if (r.ok) this.estop = r.value;
    return r;
  }

  confirmDriving(): void {
    this.drivingConfirmed = true;
  }

  getSimulation(part: string): Promise<Result<Simulation>> {
    return this.call(() => this.http.getSimulation(part));
  }
  putSimulation(part: string, pattern: Pattern): Promise<Result<Simulation>> {
    return this.call(() => this.http.putSimulation(part, pattern));
  }

  /**
   * The machine in the simulated world, live. The core sends it 20 times a second only while
   * someone subscribes, so the topic is taken per watcher and given back when it stops.
   */
  watchWorld(handlers: WorldHandlers): () => void {
    return this.socket.subscribe(
      { kind: 'world' },
      {
        onMessage: (m) => {
          if (m.type === 'world') {
            const { type: _type, id: _id, ...state } = m;
            handlers.onState(state);
          } else if (m.type === 'world-changed') handlers.onChanged(m.name);
        },
        onError: handlers.onError,
      },
    );
  }

  loadWorlds(): Promise<Result<WorldInfo[]>> {
    return this.call(() => this.http.getWorlds());
  }
  loadWorld(): Promise<Result<WorldView>> {
    return this.call(() => this.http.getWorld());
  }
  setWorld(name: string): Promise<Result<WorldView>> {
    return this.call(() => this.http.putWorld(name));
  }
  resetWorld(): Promise<Result<void>> {
    return this.call(() => this.http.resetWorld());
  }

  openSerial(port: string, baud: number, handlers: SerialHandlers): SerialSession {
    return this.socket.openSerial(port, baud, handlers);
  }

  /** An app window's live link to its app's logic (docs/apps.md §12), for the window's host. */
  openAppWindow(app: string, handlers: AppWindowHandlers): AppWindowSession {
    return this.socket.openAppWindow(app, handlers);
  }

  /** Wall-clock time (ms) of a core timestamp, or null before the system info has loaded. */
  coreTimeToEpochMs(tUs: number): number | null {
    return this.clockOffsetMs === null ? null : this.clockOffsetMs + tUs / 1000;
  }

  clearLogs(): void {
    this.logBuffer.clear();
    this.logs = [];
  }

  private pushLog(entry: LogEntry): void {
    this.logBuffer.push(entry);
    if (this.logFlushPending) return;
    this.logFlushPending = true;
    this.schedule(() => {
      this.logFlushPending = false;
      this.logs = this.logBuffer.toArray();
    });
  }

  private async call<T>(fn: () => Promise<T>): Promise<Result<T>> {
    try {
      return { ok: true, value: await fn() };
    } catch (e) {
      if (e instanceof CoreError) {
        // The session ended (signed out elsewhere, expired): find out, so the shell can say so.
        if (e.body.code === 'not-signed-in') void this.auth.load();
        return { ok: false, error: e.body };
      }
      throw e;
    }
  }
}
