import { CoreError, type CoreHttp, type WriteMode } from '../api/http';
import type { CoreSocket, SerialHandlers, SerialSession } from '../api/socket';
import { RingBuffer } from './ringBuffer';
import type {
  ConnectionStatus,
  ConnectorInfo,
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
} from './types';

export const LOG_CAPACITY = 1000;
const LOG_FLUSH_MS = 50;

export type Result<T> = { ok: true; value: T } | { ok: false; error: ErrorBody };

export interface CoreDeps {
  http: CoreHttp;
  socket: CoreSocket;
  format: WireFormat;
  /** Batches log updates; the default redraws the log at most every 50 ms. */
  schedule?: (fn: () => void) => void;
  /** Wall-clock time (ms since the epoch). */
  now?: () => number;
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
  /** The newest error that belonged to no window. */
  lastError = $state.raw<ErrorBody | null>(null);

  readonly format: WireFormat;

  private readonly http: CoreHttp;
  private readonly socket: CoreSocket;
  private readonly schedule: (fn: () => void) => void;
  private readonly now: () => number;
  private readonly logBuffer = new RingBuffer<LogEntry>(LOG_CAPACITY);
  private logFlushPending = false;
  private clockOffsetMs: number | null = null;
  private stops: (() => void)[] = [];
  private readonly fileWatchers = new Set<(folders: string[]) => void>();

  constructor(deps: CoreDeps) {
    this.http = deps.http;
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
    );
    s.connect();
    void this.loadConnectors();
  }

  stop(): void {
    for (const stop of this.stops.splice(0)) stop();
    this.socket.close();
  }

  async loadConnectors(): Promise<void> {
    const r = await this.call(() => this.http.getConnectors());
    if (r.ok) {
      this.connectors = r.value;
      this.connectorsError = null;
    } else this.connectorsError = r.error;
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

  restoreBackup(path: string): Promise<Result<void>> {
    return this.call(() => this.http.restoreBackup(path));
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

  openSerial(port: string, baud: number, handlers: SerialHandlers): SerialSession {
    return this.socket.openSerial(port, baud, handlers);
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
      if (e instanceof CoreError) return { ok: false, error: e.body };
      throw e;
    }
  }
}
