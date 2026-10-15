import type { AppRole, PartInfo, RoleParts } from './types';

/**
 * Picking the parts for an app's roles (docs/apps.md §1, D04): which parts can fill a role, what
 * to suggest, and what to send. Plain functions, so the Install window and the Apps window share
 * them and every rule is tested without a browser.
 */

/** The parts picked for each role, by role name. */
export type RolePicks = Record<string, string[]>;

/** Whether a role takes several parts (`count = "1.."`). */
export function takesMany(role: AppRole): boolean {
  return role.count !== '1';
}

/** The parts that can fill a role: those that provide its capability, by name. */
export function candidates(role: AppRole, parts: readonly PartInfo[]): string[] {
  return parts
    .filter((p) => p.signals.some((s) => s.capability === role.capability))
    .map((p) => p.name)
    .sort((a, b) => a.localeCompare(b));
}

/**
 * Where the pickers start: the parts picked already, else, when exactly one part can fill a role,
 * that one ("detect, then suggest": the user only confirms). Parts that can't fill the role any
 * more are left out.
 */
export function initialPicks(roles: readonly AppRole[], parts: readonly PartInfo[]): RolePicks {
  const picks: RolePicks = {};
  for (const role of roles) {
    const fit = candidates(role, parts);
    const bound = role.parts.filter((p) => fit.includes(p));
    picks[role.role] = bound.length > 0 ? bound : fit.length === 1 ? [fit[0]!] : [];
  }
  return picks;
}

/** Every role has a part (and a one-part role only one). */
export function allFilled(roles: readonly AppRole[], picks: RolePicks): boolean {
  return roles.every((r) => {
    const n = picks[r.role]?.length ?? 0;
    return n > 0 && (takesMany(r) || n === 1);
  });
}

/** The body of `PUT /apps/{id}/roles`: only roles with parts; one part, or a list for `1..`. */
export function rolesRequest(
  roles: readonly AppRole[],
  picks: RolePicks,
): Record<string, RoleParts> {
  const out: Record<string, RoleParts> = {};
  for (const role of roles) {
    const parts = picks[role.role] ?? [];
    if (parts.length === 0) continue;
    out[role.role] = takesMany(role) ? [...parts] : parts[0]!;
  }
  return out;
}

/** Whether the picks differ from what the app has now. */
export function picksChanged(roles: readonly AppRole[], picks: RolePicks): boolean {
  return roles.some((r) => {
    const now = picks[r.role] ?? [];
    return now.length !== r.parts.length || now.some((p, i) => p !== r.parts[i]);
  });
}
