import { describe, expect, it } from 'vitest';
import {
  combine,
  driveKey,
  keysToStick,
  padToStick,
  scaleStick,
  STILL,
  type Stick,
} from './driveInput';

const keys = (...k: string[]) => keysToStick(new Set(k as never[]));

describe('driveKey', () => {
  it('reads W/A/S/D by where they are on the keyboard, and the arrows, Q, E and Space', () => {
    expect(driveKey({ key: 'z', code: 'KeyW' })).toBe('forward'); // AZERTY's W place
    expect(driveKey({ key: 'ArrowUp', code: 'ArrowUp' })).toBe('forward');
    expect(driveKey({ key: 's', code: 'KeyS' })).toBe('back');
    expect(driveKey({ key: 'ArrowDown', code: 'ArrowDown' })).toBe('back');
    expect(driveKey({ key: 'a', code: 'KeyA' })).toBe('left');
    expect(driveKey({ key: 'ArrowLeft', code: 'ArrowLeft' })).toBe('left');
    expect(driveKey({ key: 'd', code: 'KeyD' })).toBe('right');
    expect(driveKey({ key: 'ArrowRight', code: 'ArrowRight' })).toBe('right');
    expect(driveKey({ key: 'q', code: 'KeyQ' })).toBe('turnLeft');
    expect(driveKey({ key: 'e', code: 'KeyE' })).toBe('turnRight');
    expect(driveKey({ key: ' ', code: 'Space' })).toBe('stop');
    expect(driveKey({ key: 'x', code: 'KeyX' })).toBeNull();
    expect(driveKey({ key: 'Escape', code: 'Escape' })).toBeNull();
  });
});

describe('keysToStick', () => {
  it('drives forward and back, sideways, and turns (left is positive, as in the API)', () => {
    expect(keys('forward')).toEqual({ x: 1, y: 0, turn: 0 });
    expect(keys('back')).toEqual({ x: -1, y: 0, turn: 0 });
    expect(keys('left')).toEqual({ x: 0, y: 1, turn: 0 });
    expect(keys('right')).toEqual({ x: 0, y: -1, turn: 0 });
    expect(keys('turnLeft')).toEqual({ x: 0, y: 0, turn: 1 });
    expect(keys('turnRight')).toEqual({ x: 0, y: 0, turn: -1 });
  });

  it('cancels opposite keys and keeps a diagonal no faster than straight on', () => {
    expect(keys('forward', 'back')).toEqual(STILL);
    const d = keys('forward', 'left');
    expect(Math.hypot(d.x, d.y)).toBeCloseTo(1);
    expect(d.x).toBeCloseTo(d.y);
  });
});

describe('padToStick', () => {
  it('turns a drag on the pad (screen up is forward) into a stick, within the circle', () => {
    expect(padToStick(0, -1, true)).toEqual({ x: 1, y: 0, turn: 0 });
    expect(padToStick(1, 0, true)).toEqual({ x: 0, y: -1, turn: 0 });
    const far = padToStick(2, -2, true);
    expect(Math.hypot(far.x, far.y)).toBeCloseTo(1);
  });

  it('turns instead of sliding sideways when the base is not holonomic', () => {
    expect(padToStick(-1, 0, false)).toEqual({ x: 0, y: 0, turn: 1 });
  });

  it('ignores a tiny drag in the middle', () => {
    expect(padToStick(0.02, -0.03, true)).toEqual(STILL);
  });
});

describe('combine and scale', () => {
  it('adds the pad, keys and turn slider, each axis at most full', () => {
    const a: Stick = { x: 0.8, y: 0, turn: 0 };
    const b: Stick = { x: 0.8, y: 0, turn: -0.5 };
    expect(combine(a, b)).toEqual({ x: 1, y: 0, turn: -0.5 });
  });

  it('scales to the part limits and the speed limit, in m/s and rad/s', () => {
    const max = { vx: 0.6, vy: 0.5, omega: 1.5 };
    expect(scaleStick({ x: 1, y: -1, turn: 0.5 }, max, 0.5, true)).toEqual({
      vx: 0.3,
      vy: -0.25,
      omega: 0.375,
    });
  });

  it('never asks a base that is not holonomic to slide', () => {
    const max = { vx: 0.6, vy: 0.5, omega: 1.5 };
    expect(scaleStick({ x: 0, y: 1, turn: 0 }, max, 1, false).vy).toBe(0);
  });

  it('gives exact zeros when still, so a stop is a stop', () => {
    const t = scaleStick({ x: -0, y: 0, turn: 0 }, { vx: 1, vy: 1, omega: 1 }, 0.5, true);
    expect(Object.is(t.vx, 0) && Object.is(t.vy, 0) && Object.is(t.omega, 0)).toBe(true);
  });
});
