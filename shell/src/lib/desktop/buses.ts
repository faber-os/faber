/**
 * The I2C scanner's grid and the GPIO panel's header, as plain data the windows draw. Pure: the
 * core's answers come in as plain objects of the shape the API gives.
 */

/** An I²C address as datasheets write it: `0x48`. */
export function hexAddress(address: number): string {
  return `0x${address.toString(16).padStart(2, '0')}`;
}

/** What a scan cell shows, like `i2cdetect`: nothing outside the scanned range. */
export type CellState = 'skipped' | 'empty' | 'found' | 'used';

export interface Cell {
  address: number;
  state: CellState;
  /** The part using it, for `used`. */
  usedBy?: string;
}

export interface ScanLike {
  first: number;
  last: number;
  found: readonly { address: number; usedBy?: string | null }[];
}

/**
 * The scan as `i2cdetect`'s table: 8 rows of 16 addresses (0x00–0x7f). Addresses outside
 * `first`–`last` were never probed (`skipped`); `null` (no scan yet) leaves every cell empty.
 */
export function scanGrid(scan: ScanLike | null): Cell[][] {
  const rows: Cell[][] = [];
  for (let row = 0; row < 8; row++) {
    const cells: Cell[] = [];
    for (let col = 0; col < 16; col++) {
      const address = row * 16 + col;
      if (scan && (address < scan.first || address > scan.last)) {
        cells.push({ address, state: 'skipped' });
        continue;
      }
      const hit = scan?.found.find((f) => f.address === address);
      if (!hit) cells.push({ address, state: 'empty' });
      else if (hit.usedBy) cells.push({ address, state: 'used', usedBy: hit.usedBy });
      else cells.push({ address, state: 'found' });
    }
    rows.push(cells);
  }
  return rows;
}

export interface PinLike {
  pin: number;
  name: string;
  line?: number | null;
  function?: string | null;
}

/** The header as it sits on the board: 20 rows, odd pins on the left, even on the right. */
export function headerRows<P extends PinLike>(pins: readonly P[]): [P, P][] {
  const byPin = new Map(pins.map((p) => [p.pin, p]));
  const rows: [P, P][] = [];
  for (let odd = 1; odd < 40; odd += 2) {
    const left = byPin.get(odd);
    const right = byPin.get(odd + 1);
    if (left && right) rows.push([left, right]);
  }
  return rows;
}

/** What kind of pin it is, for its colour: power, ground or a GPIO. */
export function pinKind(p: PinLike): 'power' | 'ground' | 'gpio' {
  if (p.line != null) return 'gpio';
  return p.name === 'GND' ? 'ground' : 'power';
}

/** The `gpio-<line>` port of a GPIO pin. */
export function gpioPort(line: number): string {
  return `gpio-${line}`;
}
