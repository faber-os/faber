import { describe, expect, it } from 'vitest';
import {
  isPinned,
  pin,
  taskClick,
  taskbarHeight,
  taskbarItems,
  togglePin,
  unpin,
  type Program,
  type TaskWindow,
} from './taskbar';

const PROGRAMS: Program[] = [
  { key: 'tool:parts', kind: 'parts' },
  { key: 'tool:serial-monitor', kind: 'serial-monitor' },
  { key: 'tool:logs', kind: 'logs' },
  { key: 'accessory:files', kind: 'files' },
  { key: 'accessory:notes', kind: 'notes' },
];

const win = (id: string, kind: string, minimised = false): TaskWindow => ({ id, kind, minimised });

describe('taskbarItems', () => {
  it('shows every pinned program in pinned order, even with nothing open', () => {
    const items = taskbarItems(['accessory:files', 'tool:parts'], PROGRAMS, [], null);
    expect(items).toEqual([
      { type: 'program', key: 'accessory:files', kind: 'files', windows: [], active: false },
      { type: 'program', key: 'tool:parts', kind: 'parts', windows: [], active: false },
    ]);
  });

  it('marks a pinned program open and active through its windows', () => {
    const items = taskbarItems(
      ['tool:parts', 'accessory:notes'],
      PROGRAMS,
      [win('w1', 'notes'), win('w2', 'parts'), win('w3', 'notes')],
      'w3',
    );
    expect(items).toEqual([
      { type: 'program', key: 'tool:parts', kind: 'parts', windows: ['w2'], active: false },
      {
        type: 'program',
        key: 'accessory:notes',
        kind: 'notes',
        windows: ['w1', 'w3'],
        active: true,
      },
    ]);
  });

  it('puts open windows of unpinned programs after the pinned ones, in opening order', () => {
    const items = taskbarItems(
      ['tool:parts'],
      PROGRAMS,
      [win('w1', 'laser-scan'), win('w2', 'parts'), win('w3', 'logs')],
      'w1',
    );
    expect(items.map((i) => (i.type === 'program' ? i.key : i.id))).toEqual([
      'tool:parts',
      'w1',
      'w3',
    ]);
    expect(items[1]).toEqual({ type: 'window', id: 'w1', kind: 'laser-scan', active: true });
  });

  it('does not call a minimised window active', () => {
    const items = taskbarItems([], PROGRAMS, [win('w1', 'logs', true)], 'w1');
    expect(items[0]).toMatchObject({ active: false });
  });

  it('gives each pinned app its own windows only, though every app window is of one kind', () => {
    const programs: Program[] = [
      ...PROGRAMS,
      { key: 'app:wall-follower', kind: 'app', windowKey: 'wall-follower' },
      { key: 'app:gauges', kind: 'app', windowKey: 'gauges' },
    ];
    const windows: TaskWindow[] = [
      { id: 'w1', kind: 'app', key: 'gauges', minimised: false },
      { id: 'w2', kind: 'app', key: 'wall-follower', minimised: false },
    ];
    const items = taskbarItems(['app:wall-follower'], programs, windows, 'w1');
    expect(items).toEqual([
      { type: 'program', key: 'app:wall-follower', kind: 'app', windows: ['w2'], active: false },
      { type: 'window', id: 'w1', kind: 'app', active: true },
    ]);
    expect(
      taskClick(
        { ...(items[0] as Extract<(typeof items)[0], { type: 'program' }>), windows: [] },
        null,
      ),
    ).toEqual({
      do: 'open',
      kind: 'app',
      key: 'app:wall-follower',
    });
  });

  it('skips pinned keys this shell does not know, and duplicates', () => {
    const items = taskbarItems(['app:weather', 'tool:parts', 'tool:parts'], PROGRAMS, [], null);
    expect(items.map((i) => (i.type === 'program' ? i.key : ''))).toEqual(['tool:parts']);
  });
});

describe('taskClick', () => {
  const program = (windows: string[], active = false) =>
    ({ type: 'program', key: 'accessory:notes', kind: 'notes', windows, active }) as const;

  it('starts a pinned program that is not open', () => {
    expect(taskClick(program([]), null)).toEqual({
      do: 'open',
      kind: 'notes',
      key: 'accessory:notes',
    });
  });

  it('activates its one window', () => {
    expect(taskClick(program(['w1']), null)).toEqual({ do: 'activate', id: 'w1' });
  });

  it('brings the newest of several windows forward, then steps through them', () => {
    expect(taskClick(program(['w1', 'w2', 'w3']), 'w9')).toEqual({ do: 'activate', id: 'w3' });
    expect(taskClick(program(['w1', 'w2', 'w3'], true), 'w3')).toEqual({
      do: 'activate',
      id: 'w1',
    });
    expect(taskClick(program(['w1', 'w2', 'w3'], true), 'w1')).toEqual({
      do: 'activate',
      id: 'w2',
    });
  });

  it('activates an unpinned window', () => {
    expect(taskClick({ type: 'window', id: 'w4', kind: 'logs', active: false }, null)).toEqual({
      do: 'activate',
      id: 'w4',
    });
  });
});

describe('pinning', () => {
  it('adds at the end, once', () => {
    expect(pin(['tool:parts'], 'accessory:files')).toEqual(['tool:parts', 'accessory:files']);
    expect(pin(['tool:parts'], 'tool:parts')).toEqual(['tool:parts']);
  });

  it('removes, and toggles', () => {
    expect(unpin(['tool:parts', 'accessory:files'], 'tool:parts')).toEqual(['accessory:files']);
    expect(togglePin(['tool:parts'], 'tool:parts')).toEqual([]);
    expect(togglePin([], 'tool:parts')).toEqual(['tool:parts']);
    expect(isPinned(['tool:parts'], 'tool:parts')).toBe(true);
    expect(isPinned([], 'tool:parts')).toBe(false);
  });
});

describe('taskbarHeight', () => {
  it('is taller when large, and small for anything unknown', () => {
    expect(taskbarHeight('small')).toBe(52);
    expect(taskbarHeight('large')).toBe(64);
    expect(taskbarHeight(undefined)).toBe(52);
  });
});
