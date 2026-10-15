import { t } from '../i18n';
import type { Result } from './result';
import type {
  Command,
  CommandResult,
  ErrorBody,
  LeaseEndReason,
  LeaseGrant,
  LeaseInfo,
  PartInfo,
  Twist2d,
} from './types';

/**
 * Driving from the shell (docs/api.md, "Driving"; docs/simulation.md, sections 5 and 6): holding a
 * lease while something is driven, and sending commands at a steady rate. Plain TypeScript with
 * the core behind an interface, so every rule here is tested without a core or a browser.
 */

/** Capabilities that move, and so need a lease to command. */
export const MOVING_CAPABILITIES = ['planar-base@1', 'motor@1'] as const;

/** Whether a part has something that moves. */
export function canMove(part: PartInfo): boolean {
  return part.signals.some((s) =>
    (MOVING_CAPABILITIES as readonly string[]).includes(s.capability),
  );
}

export function baseCommand(twist: Twist2d): Command {
  return { capability: 'planar-base@1', command: { kind: 'set-velocity', ...twist } };
}

export function motorCommand(velocityRadS: number): Command {
  return { capability: 'motor@1', command: { kind: 'set-velocity', velocityRadS } };
}

/** A command that holds the part still. */
export function isStill(c: Command): boolean {
  if (c.capability === 'planar-base@1') {
    const { vx, vy, omega } = c.command;
    return vx === 0 && vy === 0 && omega === 0;
  }
  const m = c.command;
  if (m.kind === 'set-velocity') return m.velocityRadS === 0;
  if (m.kind === 'set-duty') return m.duty === 0;
  return false;
}

/** The still command of the same kind. */
export function stillLike(c: Command): Command {
  return c.capability === 'planar-base@1'
    ? baseCommand({ vx: 0, vy: 0, omega: 0 })
    : motorCommand(0);
}

/** What the lease keeper needs from the core (CoreState has exactly these). */
export interface LeaseClient {
  requestLease(part: string, holder: string, takeOver: boolean): Promise<Result<LeaseGrant>>;
  renewLease(part: string, lease: string): Promise<Result<LeaseGrant>>;
  releaseLease(part: string, lease: string): Promise<Result<void>>;
  command(part: string, lease: string, command: Command): Promise<Result<CommandResult>>;
}

export type LeaseStatus =
  /** Not holding it, and nothing went wrong. */
  | { kind: 'free' }
  | { kind: 'requesting' }
  | { kind: 'held'; grant: LeaseGrant }
  /**
   * It ended without us letting go. `reason` comes from the leases topic or a refused command's
   * `lease-ended`; it is `null` when the core only said the lease isn't there (`error` has its words).
   */
  | { kind: 'ended'; reason: LeaseEndReason | null; error?: ErrorBody }
  /** Asking for it failed: someone else drives (`lease-held`), the e-stop is engaged … */
  | { kind: 'refused'; error: ErrorBody };

/** A lease lasts 250 ms; renewing every 100 ms survives one slow answer. */
const RENEW_MS = 100;

/**
 * Holds one part's lease for one holder: requests it, renews it while nothing else keeps it
 * alive (every command renews it too), and lets go of it. Requests go to the core one at a time,
 * in order, so a release never overtakes a command still on its way.
 */
export class LeaseKeeper {
  status: LeaseStatus = { kind: 'free' };

  private requesting: Promise<boolean> | null = null;
  /** The deadline of the grant, to tell our own lease's end from an older one's. */
  private firstDeadlineUs = 0;
  private queue: Promise<unknown> = Promise.resolve();
  private inFlight = 0;
  private lastActivity = 0;
  private timer: ReturnType<typeof setInterval> | null = null;

  /**
   * `autoRenew: false` leaves keeping the lease alive to whoever drives (`renew()` and commands):
   * an app window's page renews it itself, so a page that hangs lets it lapse and the part stops.
   */
  constructor(
    private readonly client: LeaseClient,
    readonly part: string,
    readonly holder: string,
    private readonly onChange: (status: LeaseStatus) => void = () => {},
    private readonly options: { autoRenew?: boolean } = {},
  ) {}

  get held(): boolean {
    return this.status.kind === 'held';
  }

  /** A request is on its way; a steady sender skips a beat rather than piling them up. */
  get busy(): boolean {
    return this.inFlight > 0;
  }

  /** Resolves true once the lease is held. `takeOver` ends whoever holds it now. */
  acquire(takeOver = false): Promise<boolean> {
    if (this.status.kind === 'held') return Promise.resolve(true);
    if (this.requesting) return this.requesting;
    this.set({ kind: 'requesting' });
    this.requesting = this.client.requestLease(this.part, this.holder, takeOver).then((r) => {
      this.requesting = null;
      if (this.status.kind !== 'requesting') {
        // Let go of while the request was on its way: give it straight back.
        if (r.ok) void this.run(() => this.client.releaseLease(this.part, r.value.lease));
        return false;
      }
      if (!r.ok) {
        this.set({ kind: 'refused', error: r.error });
        return false;
      }
      this.firstDeadlineUs = r.value.deadlineUs;
      this.lastActivity = Date.now();
      this.set({ kind: 'held', grant: r.value });
      if (this.options.autoRenew !== false)
        this.timer = setInterval(() => this.renewIfQuiet(), RENEW_MS);
      return true;
    });
    return this.requesting;
  }

  /** Sends one command with the lease (which renews it). `null` without a lease. */
  async send(command: Command): Promise<Result<CommandResult> | null> {
    if (this.status.kind !== 'held') return null;
    const lease = this.status.grant.lease;
    this.lastActivity = Date.now();
    const r = await this.run(() => this.client.command(this.part, lease, command));
    if (!r.ok) this.refusedWith(lease, r.error);
    return r;
  }

  /** Renews the lease now. `null` without a lease. */
  async renew(): Promise<Result<LeaseGrant> | null> {
    if (this.status.kind !== 'held') return null;
    const lease = this.status.grant.lease;
    this.lastActivity = Date.now();
    const r = await this.run(() => this.client.renewLease(this.part, lease));
    if (!r.ok) this.refusedWith(lease, r.error);
    return r;
  }

  /** Lets go of the lease; the core gives the part its safe state. */
  async release(): Promise<void> {
    this.stopRenewing();
    const s = this.status;
    if (s.kind === 'requesting' || s.kind === 'held') this.set({ kind: 'free' });
    if (s.kind !== 'held') return;
    // A failed release needs nothing more: the lease runs out on its own within 250 ms.
    await this.run(() => this.client.releaseLease(this.part, s.grant.lease));
  }

  /** Forget why the last lease ended or was refused (the window shows "ready" again). */
  clear(): void {
    if (this.status.kind === 'ended' || this.status.kind === 'refused') this.set({ kind: 'free' });
  }

  /** From the `leases` topic: any lease that ended. Only our own counts. */
  onLeaseEnded(lease: LeaseInfo, reason: LeaseEndReason): void {
    if (this.status.kind !== 'held') return;
    if (lease.part !== this.part || lease.holder !== this.holder) return;
    // An older lease with our holder name (another Drive window we just took over) ended
    // before ours was granted, so its deadline is before ours.
    if (lease.deadlineUs < this.firstDeadlineUs) return;
    this.end({ kind: 'ended', reason });
  }

  private renewIfQuiet(): void {
    if (this.status.kind !== 'held' || this.busy) return;
    if (Date.now() - this.lastActivity < RENEW_MS - 5) return;
    const lease = this.status.grant.lease;
    this.lastActivity = Date.now();
    void this.run(() => this.client.renewLease(this.part, lease)).then((r) => {
      if (!r.ok) this.refusedWith(lease, r.error);
    });
  }

  /** A command or renewal was refused: the lease may be gone. */
  private refusedWith(lease: string, error: ErrorBody): void {
    if (this.status.kind !== 'held' || this.status.grant.lease !== lease) return;
    if (error.code === 'estop-engaged') this.end({ kind: 'ended', reason: 'estop', error });
    // `lease-ended` says why in `reason`; the others mean the lease isn't there at all.
    else if (['lease-ended', 'lease-required', 'unknown-part'].includes(error.code))
      this.end({ kind: 'ended', reason: error.reason ?? null, error });
  }

  private end(status: LeaseStatus): void {
    this.stopRenewing();
    this.set(status);
  }

  private stopRenewing(): void {
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }

  private run<T>(fn: () => Promise<T>): Promise<T> {
    this.inFlight++;
    const p = this.queue.then(fn).finally(() => this.inFlight--);
    this.queue = p.catch(() => undefined);
    return p;
  }

  private set(status: LeaseStatus): void {
    this.status = status;
    this.onChange(status);
  }
}

/** Commands while driving: 20 a second. */
export const DRIVE_HZ = 20;

/**
 * Sends the newest command at a steady rate while something moves, and one still command when
 * it stops. Moving takes the lease when it isn't held; a refused lease is only asked for again on
 * the next move, so a held key doesn't hammer the core.
 */
export class DriveLoop {
  private latest: Command | null = null;
  private moving = false;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private readonly keeper: LeaseKeeper,
    private readonly hz = DRIVE_HZ,
  ) {}

  /** The command the controls ask for now; a still one stops. */
  set(command: Command): void {
    this.latest = command;
    const still = isStill(command);
    if (!still && !this.moving) {
      this.moving = true;
      void this.start();
    } else if (still && this.moving) {
      this.moving = false;
      this.stopTimer();
      void this.keeper.send(command);
    }
  }

  /** Stops, and lets go of the lease. */
  async release(): Promise<void> {
    const wasMoving = this.moving;
    this.moving = false;
    this.stopTimer();
    if (wasMoving && this.latest && this.keeper.held)
      await this.keeper.send(stillLike(this.latest));
    await this.keeper.release();
  }

  private async start(): Promise<void> {
    if (!this.keeper.held && !(await this.keeper.acquire())) return;
    if (!this.moving || this.timer !== null) return;
    this.tick();
    this.timer = setInterval(() => this.tick(), 1000 / this.hz);
  }

  private tick(): void {
    if (!this.moving || !this.latest || !this.keeper.held || this.keeper.busy) return;
    void this.keeper.send(this.latest);
  }

  private stopTimer(): void {
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }
}

/** Why a lease ended, in words a person understands. */
export function leaseEndText(
  reason: LeaseEndReason | null,
  part: string,
  error?: ErrorBody,
): string {
  switch (reason) {
    case 'released':
      return t('drive.ended.released', { part });
    case 'expired':
      return t('drive.ended.expired', { part });
    case 'estop':
      return t('drive.ended.estop');
    case 'taken-over':
      return t('drive.ended.takenOver', { part });
    case 'part-removed':
      return t('drive.ended.partRemoved', { part });
    case 'part-restarted':
      return t('drive.ended.partRestarted', { part });
    case 'part-failed':
      return t('drive.ended.partFailed', { part });
    case 'shutdown':
      return t('drive.ended.shutdown');
    case null:
      return error?.message ?? t('drive.ended.unknown', { part });
  }
}
