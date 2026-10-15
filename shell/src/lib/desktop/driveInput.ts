/**
 * The Drive window's controls as plain numbers: keys, the on-screen pad and the turn slider make a
 * *stick* (each axis −1..1, in the machine frame: x forward, y left, turn counter-clockwise), and
 * the stick scales to a velocity with the part's limits and the speed limit.
 */

export interface Stick {
  /** Forward (+) and back (−). */
  x: number;
  /** Left (+) and right (−). */
  y: number;
  /** Counter-clockwise (+) and clockwise (−). */
  turn: number;
}

export interface Velocity {
  vx: number;
  vy: number;
  omega: number;
}

export const STILL: Stick = { x: 0, y: 0, turn: 0 };

export type DriveKey = 'forward' | 'back' | 'left' | 'right' | 'turnLeft' | 'turnRight' | 'stop';

/**
 * Letters by where they sit on the keyboard (`code`), so W/A/S/D are the same keys on AZERTY;
 * arrows and Space by name.
 */
export function driveKey(e: { key: string; code: string }): DriveKey | null {
  switch (e.code) {
    case 'KeyW':
    case 'ArrowUp':
      return 'forward';
    case 'KeyS':
    case 'ArrowDown':
      return 'back';
    case 'KeyA':
    case 'ArrowLeft':
      return 'left';
    case 'KeyD':
    case 'ArrowRight':
      return 'right';
    case 'KeyQ':
      return 'turnLeft';
    case 'KeyE':
      return 'turnRight';
    case 'Space':
      return 'stop';
  }
  return null;
}

/** The keys held down now. A diagonal is no faster than straight on. */
export function keysToStick(held: ReadonlySet<DriveKey>): Stick {
  const axis = (plus: DriveKey, minus: DriveKey) =>
    (held.has(plus) ? 1 : 0) - (held.has(minus) ? 1 : 0);
  const x = axis('forward', 'back');
  const y = axis('left', 'right');
  const turn = axis('turnLeft', 'turnRight');
  const len = Math.hypot(x, y);
  return len > 1 ? { x: x / len, y: y / len, turn } : { x, y, turn };
}

/** Below this, a drag on the pad is a resting finger, not a command. */
const DEAD_ZONE = 0.06;

/**
 * A point on the pad, from its centre, as a fraction of its radius (screen x right, y down). On a
 * base that can't slide, left and right turn it instead.
 */
export function padToStick(px: number, py: number, holonomic: boolean): Stick {
  let x = -py;
  let y = -px;
  const len = Math.hypot(x, y);
  if (len < DEAD_ZONE) return STILL;
  if (len > 1) {
    x /= len;
    y /= len;
  }
  return holonomic ? { x: z(x), y: z(y), turn: 0 } : { x: z(x), y: 0, turn: z(y) };
}

/** Several controls at once: added, each axis at most full. */
export function combine(...sticks: Stick[]): Stick {
  const sum = (k: keyof Stick) => clamp(sticks.reduce((a, s) => a + s[k], 0));
  return { x: sum('x'), y: sum('y'), turn: sum('turn') };
}

/** The velocity a stick asks for: `max` are the part's limits, `speed` the speed limit (0..1). */
export function scaleStick(s: Stick, max: Velocity, speed: number, holonomic: boolean): Velocity {
  const k = clamp(speed, 0, 1);
  return {
    vx: round(s.x * max.vx * k),
    vy: holonomic ? round(s.y * max.vy * k) : 0,
    omega: round(s.turn * max.omega * k),
  };
}

function clamp(v: number, lo = -1, hi = 1): number {
  return Math.min(hi, Math.max(lo, v));
}

/** Millimetres per second are plenty, and a stop is an exact 0 (never −0). */
function round(v: number): number {
  return z(Math.round(v * 1000) / 1000);
}

function z(v: number): number {
  return v === 0 ? 0 : v;
}
