import { CoreError, type CoreHttp } from '../api/http';
import { t } from '../i18n';
import type { Result } from './result';
import type { SessionInfo, TokenCreated, TokenInfo, UserInfo, UserRole } from './types';

const RANK: Record<UserRole, number> = { viewer: 0, operator: 1, admin: 2 };

/** Whether `have` may do what needs `need`: each role can do everything the one below can. */
export function roleAllows(have: UserRole | null, need: UserRole): boolean {
  return have !== null && RANK[have] >= RANK[need];
}

/** The roles a user with `own` may give a token: theirs and the ones below. */
export function rolesUpTo(own: UserRole): UserRole[] {
  return (['viewer', 'operator', 'admin'] as const).filter((r) => RANK[r] <= RANK[own]);
}

/** A role's name as people read it. */
export function roleName(role: UserRole): string {
  return t(`auth.role.${role}`);
}

/**
 * Who uses this shell, and what they may do (docs/auth.md). The core decides; the shell only
 * shows what a role can't do as disabled, with the reason, never hidden.
 */
export class SessionState {
  /** `null` until the core has answered `GET /session` once. */
  info = $state.raw<SessionInfo | null>(null);
  /** The role of whoever uses this shell; `null` when nobody is signed in. */
  role = $derived<UserRole | null>(this.info?.identity?.role ?? null);
  /** The signed-in user's name; `null` for the attached screen and with sign-in off. */
  user = $derived<string | null>(this.info?.identity?.user ?? null);
  /**
   * The sign-in screen asked for while the shell is in use: the attached screen's user menu
   * offers to sign in as someone (or to create the admin on a fresh machine).
   */
  prompt = $state<'sign-in' | 'setup' | null>(null);

  private readonly http: CoreHttp;

  constructor(http: CoreHttp) {
    this.http = http;
  }

  /** Whether the current role may do what needs `need`. */
  can(need: UserRole): boolean {
    return roleAllows(this.role, need);
  }

  /** Why a control is disabled ("Needs the operator role"), or `null` when the role may use it. */
  lock(need: UserRole): string | null {
    return this.can(need) ? null : t(`auth.needs.${need}`);
  }

  /** Asks the core who this browser is. */
  async load(): Promise<Result<SessionInfo>> {
    const r = await this.call(() => this.http.getSession());
    if (r.ok) this.info = r.value;
    return r;
  }

  async signIn(name: string, password: string): Promise<Result<SessionInfo>> {
    const r = await this.call(() => this.http.signIn(name, password));
    if (r.ok) this.info = r.value;
    return r;
  }

  /** Creates the first admin and signs them in. */
  async setup(name: string, password: string): Promise<Result<SessionInfo>> {
    const r = await this.call(() => this.http.setup(name, password));
    if (r.ok) this.info = r.value;
    return r;
  }

  async signOut(): Promise<Result<void>> {
    return this.call(() => this.http.signOut());
  }

  /** Signs out, then asks who this is now: nobody (the sign-in screen), or the attached screen. */
  async signOutAndForget(): Promise<void> {
    await this.signOut();
    await this.load();
  }

  users(): Promise<Result<UserInfo[]>> {
    return this.call(() => this.http.getUsers());
  }

  addUser(name: string, password: string, role: UserRole): Promise<Result<UserInfo>> {
    return this.call(() => this.http.addUser({ name, password, role }));
  }

  removeUser(name: string): Promise<Result<void>> {
    return this.call(() => this.http.removeUser(name));
  }

  setRole(name: string, role: UserRole): Promise<Result<void>> {
    return this.call(() => this.http.setUserRole(name, role));
  }

  /** Your own (`current` given) or, for an admin, anyone's. */
  setPassword(name: string, password: string, current?: string): Promise<Result<void>> {
    return this.call(() =>
      this.http.setPassword(name, current === undefined ? { password } : { password, current }),
    );
  }

  tokens(): Promise<Result<TokenInfo[]>> {
    return this.call(() => this.http.getTokens());
  }

  createToken(name: string, role: UserRole): Promise<Result<TokenCreated>> {
    return this.call(() => this.http.createToken({ name, role }));
  }

  revokeToken(id: string): Promise<Result<void>> {
    return this.call(() => this.http.revokeToken(id));
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
