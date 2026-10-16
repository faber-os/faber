import { describe, expect, it } from 'vitest';
import { layoutBoard, stepPin, type HeaderData } from './boardDrawing';

const UNO: HeaderData[] = [
  {
    edge: 'top',
    rows: 1,
    positions: ['SCL', 'SDA', 'AREF', 'GND', 'D13', 'D12', 'D11', 'D10', 'D9', 'D8'],
  },
  { edge: 'top', rows: 1, positions: ['D7', 'D6', 'D5', 'D4', 'D3', 'D2', 'D1', 'D0'] },
  {
    edge: 'bottom',
    rows: 1,
    positions: ['NC', 'IOREF', 'RESET', '3V3', '5V', 'GND', 'GND', 'VIN'],
  },
  { edge: 'bottom', rows: 1, positions: ['A0', 'A1', 'A2', 'A3', 'A4', 'A5'] },
];

const isPin = (n: string) => /^[DA]\d+$/.test(n);

describe('drawing a board from its headers', () => {
  it('puts the headers along their edges in order, with a gap between headers', () => {
    const d = layoutBoard(UNO, isPin);
    const at = (name: string) => d.positions.find((p) => p.name === name)!;
    expect(at('D8').x + 1 + 1).toBe(at('D7').x);
    expect(at('D7').y).toBe(at('D8').y);
    expect(at('A0').y).toBeGreaterThan(at('D7').y);
    expect(at('A0').x).toBeLessThan(at('A5').x);
    // Labels aren't pins; every pin is in the keyboard order, top first.
    expect(at('GND').pin).toBe(false);
    expect(d.order[0]).toBe('D13');
    expect(d.order.at(-1)).toBe('A5');
    expect(d.order).toHaveLength(20);
    // Everything fits on the board.
    for (const p of d.positions) {
      expect(p.x).toBeGreaterThan(0);
      expect(p.x).toBeLessThan(d.width);
      expect(p.y).toBeGreaterThan(0);
      expect(p.y).toBeLessThan(d.height);
    }
  });

  it('draws a double header on the right with its edge column outermost', () => {
    const right: HeaderData = {
      edge: 'right',
      rows: 2,
      positions: ['5V', '5V', 'D22', 'D23', 'D24', 'D25', 'GND', 'GND'],
    };
    const d = layoutBoard([...UNO, right], isPin);
    const at = (name: string) => d.positions.find((p) => p.name === name)!;
    expect(at('D23').x).toBeGreaterThan(at('D22').x);
    expect(at('D23').y).toBe(at('D22').y);
    expect(at('D24').y).toBeGreaterThan(at('D22').y);
    expect([at('D22').side, at('D23').side]).toEqual(['left', 'right']);
    expect(d.view.x + d.view.width).toBeGreaterThan(d.width);
    // Keyboard order: the top, then down the right, then the bottom.
    expect(d.order.indexOf('D22')).toBeGreaterThan(d.order.indexOf('D0'));
    expect(d.order.indexOf('D22')).toBeLessThan(d.order.indexOf('A0'));
  });

  it('moves along the headers with the arrow keys', () => {
    const order = ['D13', 'D12', 'D11'];
    expect(stepPin(order, 'D12', 'ArrowRight')).toBe('D11');
    expect(stepPin(order, 'D12', 'ArrowUp')).toBe('D13');
    expect(stepPin(order, 'D11', 'ArrowDown')).toBe('D11');
    expect(stepPin(order, 'D12', 'Home')).toBe('D13');
    expect(stepPin(order, 'D13', 'End')).toBe('D11');
    expect(stepPin(order, 'D13', 'a')).toBeNull();
  });
});
