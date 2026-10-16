/**
 * The drawing of a board's headers for the pin picker (docs/wiring.md §3): where each header
 * position goes, from the pin map's `headers` (the order along each header, not measured
 * positions). Faber draws the board itself from this data, with the USB connector on the left,
 * so any board with a pin map can be drawn without artwork.
 *
 * Units are header pitches (0.1"); the component scales them.
 */

/** One header as the pin map gives it (`PinHeader`). */
export interface HeaderData {
  edge: 'top' | 'bottom' | 'right';
  rows: number;
  positions: string[];
}

/** One position of the drawing. */
export interface DrawnPosition {
  /** A pin's name (`D7`), or a label (`GND`). */
  name: string;
  /** Its centre. */
  x: number;
  y: number;
  /** Where its name is written, from the pin: towards the middle of the board, or outside. */
  side: 'below' | 'above' | 'left' | 'right';
  /** It is one of the map's pins (it can be picked), not a label like GND. */
  pin: boolean;
}

export interface BoardDrawing {
  /** The board's outline, from (0, 0). */
  width: number;
  height: number;
  /** What to show: the board, the USB connector sticking out and names written outside it. */
  view: { x: number; y: number; width: number; height: number };
  positions: DrawnPosition[];
  /** Where the board's name and the USB connector go. */
  middle: { x: number; y: number; width: number; height: number };
  usb: { x: number; y: number; width: number; height: number };
  /** The pins in keyboard order: along the top, down the right, along the bottom. */
  order: string[];
}

/** Room left of the headers for the USB connector, a gap between headers on one edge, … */
const LEFT = 5;
const GAP = 1;
const EDGE = 1;
/** Rotated names under a top pin or over a bottom pin take this much height. */
const LABELS = 3.2;
const MIDDLE = 3;
/** Names written beside a pin on the right header (`D22`). */
const SIDE_LABELS = 2;

/**
 * Lays out the headers. Positions are spaced one pitch apart; headers on one edge follow each
 * other with a gap; a double header puts its edge row nearest the board's edge.
 */
export function layoutBoard(headers: HeaderData[], isPin: (name: string) => boolean): BoardDrawing {
  const top = headers.filter((h) => h.edge === 'top');
  const bottom = headers.filter((h) => h.edge === 'bottom');
  const right = headers.filter((h) => h.edge === 'right');

  const rowsOf = (hs: HeaderData[]) => Math.max(1, ...hs.map((h) => h.rows));
  const length = (hs: HeaderData[]) =>
    hs.reduce((n, h, i) => n + h.positions.length / h.rows + (i > 0 ? GAP : 0), 0);
  const topRows = top.length ? rowsOf(top) : 0;
  const bottomRows = bottom.length ? rowsOf(bottom) : 0;
  const rightWidth = right.reduce((n, h) => n + h.rows, 0);

  // Top: the edge row at EDGE, a second (inner) row below it; names below the inner row.
  const topBand = top.length ? EDGE + topRows + LABELS : EDGE;
  const bottomBand = bottom.length ? LABELS + bottomRows + EDGE : EDGE;
  const rightLength = right.reduce((n, h) => Math.max(n, h.positions.length / h.rows), 0);
  const height = Math.max(topBand + MIDDLE + bottomBand, rightLength + 2 * EDGE + 1);
  const across = Math.max(length(top), length(bottom));
  const rightRoom = right.length ? GAP * 2 + rightWidth + SIDE_LABELS : 0;
  const width = LEFT + across + EDGE + rightRoom;

  const positions: DrawnPosition[] = [];
  const order: string[] = [];
  const add = (p: Omit<DrawnPosition, 'pin'>) => {
    const pin = isPin(p.name);
    positions.push({ ...p, pin });
  };

  // Along the top, left to right.
  let x = LEFT;
  for (const h of top) {
    const pairs = h.positions.length / h.rows;
    for (let i = 0; i < pairs; i++) {
      for (let r = 0; r < h.rows; r++) {
        const name = h.positions[i * h.rows + r]!;
        // Pair order: inner row first, then the one at the edge, named outside the board.
        const edgeRow = h.rows === 2 && r === 1;
        add({
          name,
          x: x + i + 0.5,
          y: EDGE + (h.rows === 2 && !edgeRow ? 1 : 0) + 0.5,
          side: edgeRow ? 'above' : 'below',
        });
      }
    }
    x += pairs + GAP;
  }
  // Down the right, top to bottom; the edge row is the rightmost.
  let rx = width - EDGE - rightWidth;
  for (const h of right) {
    const pairs = h.positions.length / h.rows;
    const y0 = (height - pairs) / 2;
    for (let i = 0; i < pairs; i++) {
      for (let r = 0; r < h.rows; r++) {
        const name = h.positions[i * h.rows + r]!;
        const col = h.rows === 2 ? r : 0;
        add({ name, x: rx + col + 0.5, y: y0 + i + 0.5, side: 'left' });
      }
    }
    rx += h.rows;
  }
  // Along the bottom, left to right; the edge row is the lowest.
  x = LEFT;
  for (const h of bottom) {
    const pairs = h.positions.length / h.rows;
    for (let i = 0; i < pairs; i++) {
      for (let r = 0; r < h.rows; r++) {
        const name = h.positions[i * h.rows + r]!;
        const edgeRow = h.rows === 2 && r === 1;
        add({
          name,
          x: x + i + 0.5,
          y: height - EDGE - (h.rows === 2 && !edgeRow ? 1 : 0) - 0.5,
          side: edgeRow ? 'below' : 'above',
        });
      }
    }
    x += pairs + GAP;
  }

  // A double header on the right: its inner column is named on the left, its edge column on
  // the right, outside the board.
  for (const p of positions) {
    if (p.side === 'left' && right.some((h) => h.rows === 2) && p.x > width - EDGE - 1)
      p.side = 'right';
  }
  for (const p of positions) if (p.pin) order.push(p.name);

  const middleTop = topBand;
  const middleHeight = height - topBand - bottomBand;
  const outside = (edge: HeaderData[]) => (edge.some((h) => h.rows === 2) ? LABELS : 0);
  const usb = { x: -0.6, y: height * 0.22, width: 3.6, height: Math.min(4, height * 0.3) };
  const view = {
    x: usb.x - 0.2,
    y: -outside(top) - 0.2,
    width: width - usb.x + 0.4 + (right.some((h) => h.rows === 2) ? SIDE_LABELS : 0),
    height: height + outside(top) + outside(bottom) + 0.4,
  };
  return {
    width,
    height,
    view,
    positions,
    order,
    middle: {
      x: LEFT,
      y: middleTop,
      width: Math.max(1, across),
      height: Math.max(1, middleHeight),
    },
    usb,
  };
}

/**
 * The pin a key moves to from `current` (keyboard order: along the top, down the right, along
 * the bottom). Null for a key that doesn't move.
 */
export function stepPin(order: string[], current: string, key: string): string | null {
  if (order.length === 0) return null;
  const i = Math.max(0, order.indexOf(current));
  switch (key) {
    case 'ArrowRight':
    case 'ArrowDown':
      return order[Math.min(order.length - 1, i + 1)]!;
    case 'ArrowLeft':
    case 'ArrowUp':
      return order[Math.max(0, i - 1)]!;
    case 'Home':
      return order[0]!;
    case 'End':
      return order[order.length - 1]!;
    default:
      return null;
  }
}
