import { describe, expect, it } from 'vitest';
import {
  powerUnavailable,
  RECENT_LIMIT,
  addRecent,
  loadRecent,
  powerActions,
  recentWhen,
  renameRecent,
  saveRecent,
  searchLauncher,
  type LaunchProgram,
  type RecentFile,
} from './launcher';

const PROGRAMS: LaunchProgram[] = [
  {
    key: 'tool:parts',
    kind: 'parts',
    title: 'Parts',
    description: 'Add, watch and remove the hardware',
    group: 'Tool',
  },
  {
    key: 'tool:serial-monitor',
    kind: 'serial-monitor',
    title: 'Serial monitor',
    description: 'Read, write and record a serial port',
    group: 'Tool',
  },
  {
    key: 'accessory:notes',
    kind: 'notes',
    title: 'Notes',
    description: 'A plain text editor',
    group: 'Accessory',
  },
];

const file = (name: string, at = 0): RecentFile => ({ path: `home:/Documents/${name}`, name, at });

describe('searchLauncher', () => {
  const sources = {
    programs: PROGRAMS,
    parts: [{ name: 'lidar', viewer: 'lidar/laser-scan' }, { name: 'left_motor' }],
    files: [file('lidar-notes.md'), file('wiring.txt')],
  };

  it('finds nothing for an empty query', () => {
    expect(searchLauncher('  ', sources)).toEqual({ programs: [], parts: [], files: [] });
  });

  it('finds programs, parts and recent files by name, in any case', () => {
    const r = searchLauncher('LID', sources);
    expect(r.parts.map((p) => p.name)).toEqual(['lidar']);
    expect(r.files.map((f) => f.name)).toEqual(['lidar-notes.md']);
    expect(r.programs).toEqual([]);
  });

  it('puts names that start with the query first, then other matches, then descriptions', () => {
    const r = searchLauncher('te', {
      ...sources,
      programs: [
        ...PROGRAMS,
        { key: 'tool:x', kind: 'x', title: 'Test', description: '', group: 'Tool' },
      ],
    });
    expect(r.programs.map((p) => p.title)).toEqual(['Test', 'Notes', 'Serial monitor']);
  });

  it('matches a program on its description', () => {
    expect(searchLauncher('text', sources).programs.map((p) => p.title)).toEqual(['Notes']);
  });
});

describe('recent files', () => {
  it('puts the newest first, each file once, and keeps a few', () => {
    let list: RecentFile[] = [];
    for (let i = 0; i < RECENT_LIMIT + 3; i++) list = addRecent(list, file(`f${i}.txt`, i));
    expect(list).toHaveLength(RECENT_LIMIT);
    expect(list[0]!.name).toBe(`f${RECENT_LIMIT + 2}.txt`);
    list = addRecent(list, file('f5.txt', 99));
    expect(list[0]).toEqual(file('f5.txt', 99));
    expect(list.filter((f) => f.name === 'f5.txt')).toHaveLength(1);
  });

  it('follows a renamed file', () => {
    const list = renameRecent([file('a.txt')], 'home:/Documents/a.txt', 'home:/Desktop/b.txt');
    expect(list[0]).toMatchObject({ path: 'home:/Desktop/b.txt', name: 'b.txt' });
  });

  it('is remembered in the browser, and anything unreadable is an empty list', () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
    saveRecent(storage, [file('a.txt', 5)]);
    expect(loadRecent(storage)).toEqual([file('a.txt', 5)]);
    store.set('faber.recent.v1', '{nope');
    expect(loadRecent(storage)).toEqual([]);
    store.set('faber.recent.v1', '[{"path":1}]');
    expect(loadRecent(storage)).toEqual([]);
    expect(loadRecent(null)).toEqual([]);
  });

  it("says when, as a time today, Yesterday or a date, in the machine's time zone", () => {
    const cph = { timeZone: 'Europe/Copenhagen', clock: '24h' } as const;
    // 11:42 in Copenhagen (UTC+2 in October).
    const now = Date.UTC(2026, 9, 15, 9, 42);
    expect(recentWhen(Date.UTC(2026, 9, 15, 8, 2), now, cph)).toBe('10:02');
    // 23:30 UTC on the 14th is already the 15th in Copenhagen.
    expect(recentWhen(Date.UTC(2026, 9, 14, 23, 30), now, cph)).toBe('01:30');
    expect(recentWhen(Date.UTC(2026, 9, 14, 20, 0), now, cph)).toBe('Yesterday');
    expect(recentWhen(Date.UTC(2026, 8, 1, 12), now, cph)).toBe('1-9-2026');
    expect(recentWhen(Date.UTC(2026, 9, 15, 13, 2), now, { ...cph, clock: '12h' })).toMatch(
      /^3:02\sPM$/i,
    );
  });
});

describe('powerActions', () => {
  it('offers restart and shut down, disabled with the reason until the core can do it', () => {
    expect(powerActions(false)).toEqual([
      { id: 'restart', label: 'Restart machine', disabledReason: powerUnavailable() },
      { id: 'shutdown', label: 'Shut down machine', disabledReason: powerUnavailable() },
    ]);
    expect(powerUnavailable()).toBe('Available on the Faber machine image (Phase 6)');
    expect(powerActions(true).every((a) => a.disabledReason === undefined)).toBe(true);
  });
});
