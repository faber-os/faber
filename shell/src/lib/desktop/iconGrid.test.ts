import { describe, expect, it } from 'vitest';
import {
  cellToPoint,
  gridSize,
  iconsInRect,
  moveIcon,
  moveIcons,
  placeIcons,
  pointToCell,
  positionsForSettings,
  type GridSpec,
} from './iconGrid';

const spec: GridSpec = { cellW: 100, cellH: 100, padX: 10, padY: 10 };
const area = { width: 420, height: 320 }; // 4 columns, 3 rows

describe('grid geometry', () => {
  it('fits whole cells into the area, at least one', () => {
    expect(gridSize(area, spec)).toEqual({ cols: 4, rows: 3 });
    expect(gridSize({ width: 10, height: 10 }, spec)).toEqual({ cols: 1, rows: 1 });
  });

  it('converts between cells and pixels', () => {
    expect(cellToPoint({ col: 2, row: 1 }, spec)).toEqual({ x: 210, y: 110 });
    expect(pointToCell({ x: 210, y: 110 }, spec, { cols: 4, rows: 3 })).toEqual({
      col: 2,
      row: 1,
    });
  });

  it('snaps a point to the nearest cell and keeps it inside the grid', () => {
    const grid = { cols: 4, rows: 3 };
    expect(pointToCell({ x: 258, y: 170 }, spec, grid)).toEqual({ col: 2, row: 2 });
    expect(pointToCell({ x: 9000, y: -50 }, spec, grid)).toEqual({ col: 3, row: 0 });
  });
});

describe('placeIcons', () => {
  it('puts new icons in the next free cell, top to bottom then the next column', () => {
    const p = placeIcons(['a', 'b', 'c', 'd'], [], area, spec);
    expect(p.get('a')).toEqual({ col: 0, row: 0 });
    expect(p.get('c')).toEqual({ col: 0, row: 2 });
    expect(p.get('d')).toEqual({ col: 1, row: 0 });
  });

  it('keeps saved positions, snapped to the grid', () => {
    const p = placeIcons(['a', 'b'], [{ key: 'b', x: 305, y: 212 }], area, spec);
    expect(p.get('b')).toEqual({ col: 3, row: 2 });
    expect(p.get('a')).toEqual({ col: 0, row: 0 });
  });

  it('auto-placed icons never land on a saved one', () => {
    const p = placeIcons(['new', 'old'], [{ key: 'old', x: 10, y: 10 }], area, spec);
    expect(p.get('old')).toEqual({ col: 0, row: 0 });
    expect(p.get('new')).toEqual({ col: 0, row: 1 });
  });

  it('moves the second of two icons saved on the same cell to the nearest free one', () => {
    const p = placeIcons(
      ['a', 'b'],
      [
        { key: 'a', x: 110, y: 110 },
        { key: 'b', x: 110, y: 110 },
      ],
      area,
      spec,
    );
    expect(p.get('a')).toEqual({ col: 1, row: 1 });
    const b = p.get('b')!;
    expect(Math.abs(b.col - 1) + Math.abs(b.row - 1)).toBe(1);
  });

  it('pulls icons saved on a bigger screen back inside a smaller one', () => {
    const p = placeIcons(['a'], [{ key: 'a', x: 1800, y: 900 }], area, spec);
    expect(p.get('a')).toEqual({ col: 3, row: 2 });
  });

  it('ignores saved positions of icons that no longer exist', () => {
    const p = placeIcons(['a'], [{ key: 'gone', x: 10, y: 10 }], area, spec);
    expect([...p.keys()]).toEqual(['a']);
    expect(p.get('a')).toEqual({ col: 0, row: 0 });
  });
});

describe('moveIcon', () => {
  it('drops an icon on the cell under the pointer', () => {
    const p = placeIcons(['a', 'b'], [], area, spec);
    const next = moveIcon(p, 'a', { x: 330, y: 240 }, area, spec);
    expect(next.get('a')).toEqual({ col: 3, row: 2 });
    expect(next.get('b')).toEqual({ col: 0, row: 1 });
    expect(p.get('a')).toEqual({ col: 0, row: 0 }); // the old placement is left alone
  });

  it('dropped on another icon, it takes the nearest free cell instead', () => {
    const p = placeIcons(['a', 'b'], [], area, spec);
    const next = moveIcon(p, 'a', { x: 10, y: 110 }, area, spec);
    expect(next.get('b')).toEqual({ col: 0, row: 1 });
    expect(next.get('a')).not.toEqual({ col: 0, row: 1 });
  });

  it('dropping an icon on its own cell changes nothing', () => {
    const p = placeIcons(['a'], [], area, spec);
    expect(moveIcon(p, 'a', { x: 12, y: 15 }, area, spec).get('a')).toEqual({ col: 0, row: 0 });
  });
});

describe('moveIcons', () => {
  it('moves several icons by the same amount, keeping their shape', () => {
    const p = placeIcons(['a', 'b', 'c'], [], area, spec); // a 0,0  b 0,1  c 0,2
    const next = moveIcons(p, ['a', 'b'], { dx: 200, dy: 0 }, area, spec);
    expect(next.get('a')).toEqual({ col: 2, row: 0 });
    expect(next.get('b')).toEqual({ col: 2, row: 1 });
    expect(next.get('c')).toEqual({ col: 0, row: 2 });
  });

  it('can move down into cells the group itself leaves', () => {
    const p = placeIcons(['a', 'b'], [], area, spec); // a 0,0  b 0,1
    const next = moveIcons(p, ['a', 'b'], { dx: 0, dy: 100 }, area, spec);
    expect(next.get('a')).toEqual({ col: 0, row: 1 });
    expect(next.get('b')).toEqual({ col: 0, row: 2 });
  });

  it('never stacks icons: one landing on another takes a free cell', () => {
    const p = placeIcons(['a', 'b', 'c'], [], area, spec);
    const next = moveIcons(p, ['a'], { dx: 0, dy: 100 }, area, spec);
    const cells = [...next.values()].map((c) => `${c.col},${c.row}`);
    expect(new Set(cells).size).toBe(3);
  });
});

describe('iconsInRect', () => {
  it('finds the icons a selection rectangle touches, drawn in any direction', () => {
    const p = placeIcons(['a', 'b', 'c', 'd'], [], area, spec); // column 0, then d at 1,0
    expect(iconsInRect(p, { x: 0, y: 0, w: 50, h: 150 }, spec)).toEqual(['a', 'b']);
    expect(iconsInRect(p, { x: 150, y: 50, w: -100, h: -20 }, spec)).toEqual(['a', 'd']);
    expect(iconsInRect(p, { x: 400, y: 300, w: 5, h: 5 }, spec)).toEqual([]);
  });
});

describe('positionsForSettings', () => {
  it('stores every placed icon in pixels, in a stable order', () => {
    const p = placeIcons(['b', 'a'], [], area, spec);
    expect(positionsForSettings(p, spec)).toEqual([
      { key: 'a', x: 10, y: 110 },
      { key: 'b', x: 10, y: 10 },
    ]);
  });
});
