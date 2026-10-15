import { describe, expect, it } from 'vitest';
import { DesktopController } from './desktopController.svelte';
import { TASKBAR_HEIGHTS } from '../lib/desktop/taskbar';

const TASKBAR_HEIGHT = TASKBAR_HEIGHTS.small;
import type { WindowKind } from '../lib/desktop/windows';
import type { StorageLike } from '../lib/desktop/geometryStore';

const kinds: Record<string, WindowKind> = {
  parts: {
    kind: 'parts',
    title: 'Parts',
    multiple: false,
    defaultSize: { w: 600, h: 400 },
    minSize: { w: 320, h: 240 },
  },
  'laser-scan': {
    kind: 'laser-scan',
    title: 'Scan',
    multiple: true,
    defaultSize: { w: 500, h: 500 },
    minSize: { w: 300, h: 300 },
  },
};

function memory(): StorageLike & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) };
}

describe('DesktopController', () => {
  it('leaves room for the taskbar', () => {
    const d = new DesktopController(kinds, null, { width: 1440, height: 900 });
    expect(d.area).toEqual({ width: 1440, height: 900 - TASKBAR_HEIGHT });
  });

  it('opens windows maximised on the 1024×600 screen', () => {
    const d = new DesktopController(kinds, null, { width: 1024, height: 600 });
    const id = d.open('parts');
    expect(d.state.windows.find((w) => w.id === id)?.maximised).toBe(true);
  });

  it('opens windows normally on a desktop screen', () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    expect(d.state.windows.find((w) => w.id === id)?.maximised).toBe(false);
  });

  it('remembers where a window was, per viewer address', () => {
    const storage = memory();
    const d = new DesktopController(kinds, storage, { width: 1600, height: 1000 });
    const id = d.open('laser-scan', { key: 'demo/laser-scan' });
    d.setRect(id, { x: 40, y: 50, w: 420, h: 380 });
    d.flush();
    d.close(id);

    const again = new DesktopController(kinds, storage, { width: 1600, height: 1000 });
    const id2 = again.open('laser-scan', { key: 'demo/laser-scan' });
    expect(again.state.windows.find((w) => w.id === id2)?.rect).toEqual({
      x: 40,
      y: 50,
      w: 420,
      h: 380,
    });
  });

  it('toggles a window from the taskbar', () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    d.activate(id);
    expect(d.state.windows[0]!.minimised).toBe(true);
    d.activate(id);
    expect(d.state.windows[0]!.minimised).toBe(false);
    expect(d.state.focusedId).toBe(id);
  });

  it('pulls windows back on screen when the viewport shrinks', () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    d.setRect(id, { x: 1000, y: 500, w: 600, h: 400 });
    d.setViewport({ width: 1200, height: 800 });
    const r = d.state.windows[0]!.rect;
    expect(r.x + r.w).toBeLessThanOrEqual(1200);
    expect(r.y + r.h).toBeLessThanOrEqual(800 - TASKBAR_HEIGHT);
  });

  it('lets a window with unsaved work refuse to close, and close later', () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    let allow = false;
    const asked: string[] = [];
    d.setCloseGuard(id, () => {
      asked.push(id);
      return allow;
    });
    d.requestClose(id);
    expect(asked).toEqual([id]);
    expect(d.state.windows).toHaveLength(1);
    allow = true;
    d.requestClose(id);
    expect(d.state.windows).toHaveLength(0);
  });

  it('closes without asking once the guard is removed', () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    const remove = d.setCloseGuard(id, () => false);
    remove();
    d.requestClose(id);
    expect(d.state.windows).toHaveLength(0);
  });

  it('gives the windows less room with the large taskbar, and refits them', () => {
    const d = new DesktopController(kinds, null, { width: 1200, height: 700 });
    const id = d.open('parts');
    d.setRect(id, { x: 0, y: 300, w: 400, h: 340 });
    d.setTaskbarHeight(64);
    expect(d.area.height).toBe(700 - 64);
    const r = d.state.windows.find((w) => w.id === id)!.rect;
    expect(r.y + r.h).toBeLessThanOrEqual(700 - 64);
  });

  it("forgets a window's tabs when it closes", () => {
    const d = new DesktopController(kinds, null, { width: 1600, height: 1000 });
    const id = d.open('parts');
    d.setTabs(id, {
      state: { tabs: [], activeId: null, nextId: 1 },
      activate: () => {},
      close: () => {},
    });
    expect(d.tabs[id]).toBeDefined();
    d.close(id);
    expect(d.tabs[id]).toBeUndefined();
  });
});

describe('DesktopController titles', () => {
  it('shows a window in the language of the moment, unless it named itself', async () => {
    const { WINDOW_KINDS } = await import('../windows/registry');
    const { locale } = await import('../lib/i18n/locale.svelte');
    const d = new DesktopController(WINDOW_KINDS, memory(), { width: 1440, height: 900 });
    const parts = d.open('parts');
    const doc = d.open('notes');
    d.update(doc, { title: 'todo.txt' });
    const win = (id: string) => d.state.windows.find((w) => w.id === id)!;
    expect(d.titleOf(win(parts))).toBe('Parts');
    try {
      locale.use('xx', { 'programs.parts.title': 'Partz' });
      expect(d.titleOf(win(parts))).toBe('Partz');
      expect(d.titleOf(win(doc))).toBe('todo.txt');
    } finally {
      locale.use('en', null);
    }
  });
});
