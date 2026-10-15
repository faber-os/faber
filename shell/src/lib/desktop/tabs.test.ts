import { describe, expect, it } from 'vitest';
import {
  activateTab,
  activeTab,
  addTab,
  closeTab,
  dirtyTabs,
  emptyTabs,
  findTab,
  stepTab,
  tabTarget,
  updateTab,
  type TabsState,
} from './tabs';

function three(): TabsState {
  let s = emptyTabs();
  for (const k of ['a', 'b', 'c']) s = addTab(s, { title: k, key: k }).state;
  return s;
}

describe('tabs', () => {
  it('adds a tab after the active one and makes it active', () => {
    let s = three();
    s = activateTab(s, s.tabs[0]!.id);
    const r = addTab(s, { title: 'new', key: 'n' });
    expect(r.state.tabs.map((t) => t.title)).toEqual(['a', 'new', 'b', 'c']);
    expect(r.state.activeId).toBe(r.id);
    expect(activeTab(r.state)?.dirty).toBe(false);
  });

  it('gives ids that are never reused', () => {
    let s = three();
    const last = s.tabs.at(-1)!.id;
    s = closeTab(s, last);
    expect(addTab(s, { title: 'd', key: 'd' }).id).not.toBe(last);
  });

  it('closing the active tab activates its right neighbour, or the left one at the end', () => {
    const s = three();
    const [a, b, c] = s.tabs.map((t) => t.id);
    const mid = closeTab(activateTab(s, b!), b!);
    expect(mid.activeId).toBe(c);
    const end = closeTab(s, c!);
    expect(end.activeId).toBe(b);
    const other = closeTab(activateTab(s, a!), c!);
    expect(other.activeId).toBe(a);
  });

  it('closing the last tab leaves none active', () => {
    const r = addTab(emptyTabs(), { title: 'x', key: 'x' });
    expect(closeTab(r.state, r.id)).toMatchObject({ tabs: [], activeId: null });
  });

  it('updates a tab only when something changed', () => {
    const s = three();
    const id = s.tabs[0]!.id;
    expect(updateTab(s, id, { title: 'a' })).toBe(s);
    const next = updateTab(s, id, { title: 'A', dirty: true, key: 'k' });
    expect(next.tabs[0]).toMatchObject({ title: 'A', dirty: true, key: 'k' });
    expect(dirtyTabs(next).map((t) => t.id)).toEqual([id]);
  });

  it('finds a tab by what it shows', () => {
    expect(findTab(three(), 'b')?.title).toBe('b');
    expect(findTab(three(), 'zz')).toBeUndefined();
  });

  it('steps through the tabs with wrap-round', () => {
    const s = three();
    const [a, , c] = s.tabs.map((t) => t.id);
    expect(stepTab(s, 1).activeId).toBe(a);
    expect(stepTab(activateTab(s, a!), -1).activeId).toBe(c);
  });
});

describe('tabTarget', () => {
  const windows = [
    { id: 'w1', kind: 'notes', z: 3, minimised: false },
    { id: 'w2', kind: 'notes', z: 7, minimised: false },
    { id: 'w3', kind: 'files', z: 9, minimised: false },
  ];
  const tabs: Record<string, TabsState> = {
    w1: addTab(emptyTabs(), { title: 'wiring.txt', key: 'home:/Desktop/wiring.txt' }).state,
    w2: addTab(emptyTabs(), { title: 'Untitled 1', key: 'new:1' }).state,
  };

  it('goes to the tab that already shows the file', () => {
    expect(tabTarget(windows, 'notes', (id) => tabs[id], 'home:/Desktop/wiring.txt')).toEqual({
      windowId: 'w1',
      tabId: tabs.w1!.tabs[0]!.id,
    });
  });

  it("otherwise to the program's frontmost window", () => {
    expect(tabTarget(windows, 'notes', (id) => tabs[id], 'home:/x.txt')).toEqual({
      windowId: 'w2',
    });
  });

  it('or nowhere when the program has no window', () => {
    expect(tabTarget(windows, 'logs', (id) => tabs[id], 'x')).toBeNull();
  });
});
