import { describe, expect, it } from 'vitest';
import { gpioPort, headerRows, hexAddress, pinKind, scanGrid } from './buses';

describe('the I2C scanner grid', () => {
  it('lays a scan out like i2cdetect, skipping what was never probed', () => {
    const grid = scanGrid({
      first: 0x08,
      last: 0x77,
      found: [{ address: 0x48 }, { address: 0x4a, usedBy: 'cabin_temp' }],
    });
    expect(grid).toHaveLength(8);
    expect(grid.every((r) => r.length === 16)).toBe(true);
    expect(grid[0]![7]!.state).toBe('skipped');
    expect(grid[0]![8]!.state).toBe('empty');
    expect(grid[4]![8]).toEqual({ address: 0x48, state: 'found' });
    expect(grid[4]![10]).toEqual({ address: 0x4a, state: 'used', usedBy: 'cabin_temp' });
    expect(grid[7]![8]!.state).toBe('skipped');
    expect(scanGrid(null)[0]![0]!.state).toBe('empty');
    expect(hexAddress(0x48)).toBe('0x48');
    expect(hexAddress(8)).toBe('0x08');
  });
});

describe('the GPIO header', () => {
  it('pairs the pins as they sit on the board', () => {
    const pins = Array.from({ length: 40 }, (_, i) => ({ pin: i + 1, name: `P${i + 1}` }));
    const rows = headerRows(pins);
    expect(rows).toHaveLength(20);
    expect(rows[0]!.map((p) => p.pin)).toEqual([1, 2]);
    expect(rows[19]!.map((p) => p.pin)).toEqual([39, 40]);
    expect(pinKind({ pin: 6, name: 'GND' })).toBe('ground');
    expect(pinKind({ pin: 1, name: '3V3' })).toBe('power');
    expect(pinKind({ pin: 11, name: 'GPIO17', line: 17 })).toBe('gpio');
    expect(gpioPort(17)).toBe('gpio-17');
  });
});
