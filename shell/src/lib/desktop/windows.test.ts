import { describe, expect, it } from 'vitest';
import {
  clampRect,
  closeWindow,
  emptyDesktop,
  focusWindow,
  isCompactScreen,
  minimiseWindow,
  moveRect,
  openWindow,
  refit,
  resizeRect,
  setRect,
  stackOrder,
  toggleMaximise,
  topWindow,
  updateWindow,
  type Area,
  type WindowKind,
} from './windows';

const area: Area = { width: 1600, height: 900 };
const parts: WindowKind = {
  kind: 'parts',
  title: 'Parts',
  multiple: false,
  defaultSize: { w: 600, h: 400 },
  minSize: { w: 320, h: 240 },
};
const viewer: WindowKind = {
  kind: 'laser-scan',
  title: 'Viewer',
  multiple: true,
  defaultSize: { w: 500, h: 500 },
  minSize: { w: 300, h: 300 },
};

describe('isCompactScreen', () => {
  it('treats the 7" 1024×600 screen and anything smaller as compact', () => {
    expect(isCompactScreen({ width: 1024, height: 600 })).toBe(true);
    expect(isCompactScreen({ width: 390, height: 844 })).toBe(true);
    expect(isCompactScreen({ width: 1280, height: 600 })).toBe(true);
    expect(isCompactScreen({ width: 1440, height: 900 })).toBe(false);
  });
});

describe('clampRect', () => {
  it('keeps a window inside the area and at least its minimum size', () => {
    expect(clampRect({ x: -50, y: -10, w: 100, h: 100 }, { w: 200, h: 150 }, area)).toEqual({
      x: 0,
      y: 0,
      w: 200,
      h: 150,
    });
    expect(clampRect({ x: 1500, y: 850, w: 400, h: 300 }, parts.minSize, area)).toEqual({
      x: 1200,
      y: 600,
      w: 400,
      h: 300,
    });
  });

  it('shrinks a window that is bigger than the area, even below its minimum', () => {
    const small: Area = { width: 300, height: 200 };
    expect(clampRect({ x: 10, y: 10, w: 800, h: 600 }, parts.minSize, small)).toEqual({
      x: 0,
      y: 0,
      w: 300,
      h: 200,
    });
  });
});

describe('moveRect and resizeRect', () => {
  const start = { x: 100, y: 100, w: 400, h: 300 };

  it('moves by the drag distance, clamped to the area', () => {
    expect(moveRect(start, 50, -20, area)).toEqual({ x: 150, y: 80, w: 400, h: 300 });
    expect(moveRect(start, -500, -500, area)).toEqual({ x: 0, y: 0, w: 400, h: 300 });
    expect(moveRect(start, 5000, 5000, area)).toEqual({ x: 1200, y: 600, w: 400, h: 300 });
  });

  it('resizes from the bottom-right edge', () => {
    expect(resizeRect(start, 'se', 100, 50, parts.minSize, area)).toEqual({
      x: 100,
      y: 100,
      w: 500,
      h: 350,
    });
  });

  it('resizes from the top-left edge, keeping the opposite corner fixed', () => {
    expect(resizeRect(start, 'nw', -40, -30, parts.minSize, area)).toEqual({
      x: 60,
      y: 70,
      w: 440,
      h: 330,
    });
  });

  it('stops at the minimum size without moving the fixed edge', () => {
    expect(resizeRect(start, 'w', 300, 0, parts.minSize, area)).toEqual({
      x: 180,
      y: 100,
      w: 320,
      h: 300,
    });
    expect(resizeRect(start, 'n', 0, 999, parts.minSize, area)).toEqual({
      x: 100,
      y: 160,
      w: 400,
      h: 240,
    });
  });

  it('stops at the area edges', () => {
    expect(resizeRect(start, 'e', 5000, 0, parts.minSize, area).w).toBe(1500);
    expect(resizeRect(start, 'n', 0, -5000, parts.minSize, area)).toEqual({
      x: 100,
      y: 0,
      w: 400,
      h: 400,
    });
  });
});

describe('openWindow', () => {
  it('opens a window at its default size, focused and on top', () => {
    const { state, id } = openWindow(emptyDesktop(), { kind: parts }, area);
    const w = state.windows[0]!;
    expect(w.id).toBe(id);
    expect(w.rect.w).toBe(600);
    expect(w.rect.h).toBe(400);
    expect(w.title).toBe('Parts');
    expect(state.focusedId).toBe(id);
    expect(w.maximised).toBe(false);
  });

  it('only ever opens one window of a single-instance kind', () => {
    let { state } = openWindow(emptyDesktop(), { kind: parts }, area);
    const first = state.windows[0]!.id;
    state = minimiseWindow(state, first);
    const again = openWindow(state, { kind: parts }, area);
    expect(again.state.windows).toHaveLength(1);
    expect(again.id).toBe(first);
    expect(again.state.windows[0]!.minimised).toBe(false);
    expect(again.state.focusedId).toBe(first);
  });

  it('opens one window per key for a multi-instance kind', () => {
    let s = openWindow(emptyDesktop(), { kind: viewer, key: 'a/laser-scan', title: 'a' }, area);
    s = openWindow(s.state, { kind: viewer, key: 'b/laser-scan', title: 'b' }, area);
    expect(s.state.windows).toHaveLength(2);
    const again = openWindow(s.state, { kind: viewer, key: 'a/laser-scan' }, area);
    expect(again.state.windows).toHaveLength(2);
    expect(again.state.focusedId).toBe(s.state.windows[0]!.id);
  });

  it('cascades new windows so they do not cover each other exactly', () => {
    let s = openWindow(emptyDesktop(), { kind: viewer, key: 'a' }, area);
    s = openWindow(s.state, { kind: viewer, key: 'b' }, area);
    const [a, b] = s.state.windows;
    expect(b!.rect.x).toBeGreaterThan(a!.rect.x);
    expect(b!.rect.y).toBeGreaterThan(a!.rect.y);
  });

  it('uses saved geometry when it is given, clamped to the area', () => {
    const { state } = openWindow(
      emptyDesktop(),
      { kind: parts, saved: { x: 1500, y: 20, w: 700, h: 500 } },
      area,
    );
    expect(state.windows[0]!.rect).toEqual({ x: 900, y: 20, w: 700, h: 500 });
  });

  it('opens maximised on a compact screen', () => {
    const { state } = openWindow(
      emptyDesktop(),
      { kind: parts },
      { width: 1024, height: 552 },
      {
        compact: true,
      },
    );
    expect(state.windows[0]!.maximised).toBe(true);
  });
});

describe('focus, minimise, maximise, close', () => {
  function three() {
    let s = openWindow(emptyDesktop(), { kind: viewer, key: 'a' }, area);
    s = openWindow(s.state, { kind: viewer, key: 'b' }, area);
    s = openWindow(s.state, { kind: viewer, key: 'c' }, area);
    const [a, b, c] = s.state.windows.map((w) => w.id) as [string, string, string];
    return { state: s.state, a, b, c };
  }

  it('brings a focused window to the top', () => {
    const { state, a, b, c } = three();
    const s = focusWindow(state, a);
    expect(stackOrder(s).map((w) => w.id)).toEqual([b, c, a]);
    expect(s.focusedId).toBe(a);
  });

  it('gives focus to the next window down when one is minimised', () => {
    const { state, b, c } = three();
    const s = minimiseWindow(state, c);
    expect(s.focusedId).toBe(b);
    expect(topWindow(s)?.id).toBe(b);
  });

  it('leaves nothing focused when every window is minimised', () => {
    const { state, a, b, c } = three();
    const s = minimiseWindow(minimiseWindow(minimiseWindow(state, a), b), c);
    expect(s.focusedId).toBeNull();
  });

  it('maximises and restores to the previous rect', () => {
    const { state, a } = three();
    const before = state.windows[0]!.rect;
    const max = toggleMaximise(state, a);
    expect(max.windows[0]!.maximised).toBe(true);
    expect(max.focusedId).toBe(a);
    const back = toggleMaximise(max, a);
    expect(back.windows[0]!.maximised).toBe(false);
    expect(back.windows[0]!.rect).toEqual(before);
  });

  it('closes a window and focuses the next one down', () => {
    const { state, b, c } = three();
    const s = closeWindow(state, c);
    expect(s.windows.map((w) => w.id)).not.toContain(c);
    expect(s.focusedId).toBe(b);
  });

  it('ignores unknown ids', () => {
    const { state } = three();
    expect(focusWindow(state, 'nope')).toBe(state);
    expect(closeWindow(state, 'nope')).toBe(state);
  });

  it('sets a new rect', () => {
    const { state, a } = three();
    const s = setRect(state, a, { x: 1, y: 2, w: 400, h: 400 });
    expect(s.windows[0]!.rect).toEqual({ x: 1, y: 2, w: 400, h: 400 });
  });
});

describe('refit', () => {
  it('pulls windows back inside when the screen gets smaller', () => {
    let s = openWindow(
      emptyDesktop(),
      { kind: parts, saved: { x: 1000, y: 500, w: 600, h: 400 } },
      area,
    );
    const smaller: Area = { width: 1200, height: 700 };
    s = { ...s, state: refit(s.state, smaller, () => parts.minSize) };
    expect(s.state.windows[0]!.rect).toEqual({ x: 600, y: 300, w: 600, h: 400 });
  });
});

describe('updateWindow', () => {
  it('changes the title, key and params, e.g. after Save as', () => {
    const { state, id } = openWindow(emptyDesktop(), { kind: viewer, key: 'a' }, area);
    const s = updateWindow(state, id, { title: '*b.txt', key: 'home:/b.txt', params: { p: '1' } });
    expect(s.windows[0]).toMatchObject({ title: '*b.txt', key: 'home:/b.txt', params: { p: '1' } });
    expect(updateWindow(s, 'nope', { title: 'x' })).toBe(s);
  });

  it('then finds the window by its new key', () => {
    const { state, id } = openWindow(emptyDesktop(), { kind: viewer, key: 'a' }, area);
    const s = updateWindow(state, id, { key: 'b' });
    expect(openWindow(s, { kind: viewer, key: 'b' }, area).id).toBe(id);
  });
});
