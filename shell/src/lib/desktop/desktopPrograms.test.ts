import { describe, expect, it } from 'vitest';
import { addToDesktop, onDesktop, removeFromDesktop } from './desktopPrograms';

const PROGRAMS = ['tool:parts', 'tool:apps', 'tool:world', 'accessory:files'];

describe('programs on the desktop', () => {
  it('shows every program the user has not removed, also ones the layout never heard of', () => {
    // A layout saved before World existed still gets World's icon.
    expect(
      onDesktop(PROGRAMS, { icons: [{ key: 'tool:parts', x: 12, y: 12 }], removed: [] }),
    ).toEqual(PROGRAMS);
    expect(onDesktop(PROGRAMS, { icons: [], removed: ['tool:apps'] })).toEqual([
      'tool:parts',
      'tool:world',
      'accessory:files',
    ]);
  });

  it('removes an icon for good, and forgets where it was', () => {
    const layout = {
      icons: [
        { key: 'tool:apps', x: 12, y: 108 },
        { key: 'home:/Desktop/a.txt', x: 104, y: 12 },
      ],
      removed: ['tool:logs'],
    };
    const out = removeFromDesktop(layout, 'tool:apps');
    expect(out).toEqual({
      icons: [{ key: 'home:/Desktop/a.txt', x: 104, y: 12 }],
      removed: ['tool:logs', 'tool:apps'],
    });
    expect(removeFromDesktop(out, 'tool:apps')).toEqual(out);
  });

  it('adds a removed icon back', () => {
    const out = addToDesktop({ icons: [], removed: ['tool:logs', 'tool:apps'] }, 'tool:apps');
    expect(out).toEqual({ icons: [], removed: ['tool:logs'] });
    expect(onDesktop(PROGRAMS, out)).toContain('tool:apps');
  });
});
