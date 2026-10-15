import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LongPress } from './longPress';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('LongPress', () => {
  it('fires after 500 ms of holding still, at the start point', () => {
    const lp = new LongPress();
    const fn = vi.fn();
    lp.start(10, 20, fn);
    lp.move(14, 23);
    vi.advanceTimersByTime(499);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledWith(10, 20);
    expect(lp.fired).toBe(true);
  });

  it('is cancelled by moving more than 10 px, or by lifting the finger', () => {
    const lp = new LongPress();
    const fn = vi.fn();
    lp.start(0, 0, fn);
    lp.move(11, 0);
    vi.advanceTimersByTime(600);
    lp.start(0, 0, fn);
    lp.cancel();
    vi.advanceTimersByTime(600);
    expect(fn).not.toHaveBeenCalled();
    expect(lp.fired).toBe(false);
  });
});
