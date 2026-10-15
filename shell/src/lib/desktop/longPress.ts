/**
 * Touch has no right button: holding a finger still for ~500 ms opens the context menu instead.
 * Moving more than a few pixels first means the finger is scrolling or dragging, not pressing.
 */
export const LONG_PRESS_MS = 500;
export const LONG_PRESS_SLOP_PX = 10;

export interface Timers {
  set(fn: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

const realTimers: Timers = {
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

export class LongPress {
  private handle: unknown = null;
  private x = 0;
  private y = 0;
  /** True from the moment the press fired until the next start: the click that follows is eaten. */
  fired = false;

  constructor(
    private readonly timers: Timers = realTimers,
    private readonly ms = LONG_PRESS_MS,
    private readonly slop = LONG_PRESS_SLOP_PX,
  ) {}

  start(x: number, y: number, onPress: (x: number, y: number) => void): void {
    this.cancel();
    this.fired = false;
    this.x = x;
    this.y = y;
    this.handle = this.timers.set(() => {
      this.handle = null;
      this.fired = true;
      onPress(this.x, this.y);
    }, this.ms);
  }

  move(x: number, y: number): void {
    if (this.handle === null) return;
    if (Math.hypot(x - this.x, y - this.y) > this.slop) this.cancel();
  }

  cancel(): void {
    if (this.handle !== null) this.timers.clear(this.handle);
    this.handle = null;
  }
}
