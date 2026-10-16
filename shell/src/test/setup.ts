import '@testing-library/jest-dom/vitest';

/*
 * jsdom has no layout and no canvas. These stand-ins give components what the browser would:
 * a ResizeObserver that reports a fixed 400×300 box, and a 2D context that draws nothing.
 * The drawing maths is tested on its own in src/lib/viewers.
 */
class FixedResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element) {
    const entry = {
      target,
      contentRect: { width: 400, height: 300 },
    } as unknown as ResizeObserverEntry;
    queueMicrotask(() => this.callback([entry], this as unknown as ResizeObserver));
  }
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= FixedResizeObserver as unknown as typeof ResizeObserver;

const noop = () => {};
const context2d = new Proxy(
  {},
  {
    get: (_t, prop) => (prop === 'canvas' ? undefined : noop),
    set: () => true,
  },
);
HTMLCanvasElement.prototype.getContext = function getContext() {
  return context2d as CanvasRenderingContext2D;
} as unknown as typeof HTMLCanvasElement.prototype.getContext;

/** Path2D, as far as the World view builds paths: records nothing, like the context above. */
class NoPath2D {
  moveTo() {}
  lineTo() {}
  closePath() {}
  rect() {}
  arc() {}
}
globalThis.Path2D ??= NoPath2D as unknown as typeof Path2D;

/** Text ranges have no boxes without layout; CodeMirror (the Arduino editor) measures them. */
const noRects = () => [] as unknown as DOMRectList;
const noRect = () => new DOMRect(0, 0, 0, 0);
Range.prototype.getClientRects ??= noRects;
Range.prototype.getBoundingClientRect ??= noRect;
