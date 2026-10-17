/**
 * The wiring diagram's data (docs/wiring-diagram.md). Positions are in grid units: 1 unit =
 * 0.05 in = 1.27 mm, half a header pitch, so a wire can pass between two pins.
 */

/** The way a wire leaves a pin: up, right, down, left. */
export type Dir = 0 | 1 | 2 | 3;

/** What a board pin can do. */
export type Cap = 'io' | 'pwm' | 'analog' | 'sda' | 'scl' | 'int' | 'tx' | 'rx';

export type PinKind =
  /** A board's general pin (see `caps`). */
  | 'io'
  /** Power out of a board or a part (a 5 V pin, a battery's +). */
  | 'supply'
  | 'gnd'
  /** A board's own power input with a range (VIN). */
  | 'vin'
  /** Not for wiring (NC, AREF, RESET). */
  | 'other'
  /** A part's logic power in, with a range. */
  | 'vcc'
  /** A part's signal pin (see `needs`). */
  | 'sig'
  /** A part's main power in, with a range (a driver's B+). */
  | 'power-in'
  | 'motor-out'
  | 'motor-in'
  | 'usb-host'
  | 'usb-device'
  /** A breadboard hole. */
  | 'hole';

export interface PinDef {
  id: string;
  label: string;
  x: number;
  y: number;
  d: Dir;
  kind: PinKind;
  /** Board pins: what the pin can do. */
  caps?: Cap[];
  /** Board pins: logic level in volts. */
  logic?: number;
  /** `supply` pins: volts out. */
  volts?: number;
  /** `vcc`, `vin`, `power-in`: the range the pin accepts. */
  min?: number;
  max?: number;
  /** Part signal pins: what the board pin must be able to do. */
  needs?: Cap[];
  /** Part signal pins that drive the line, and at what level. */
  drives?: number;
  /** Part signal pins that output (two outputs on one line fight). */
  out?: boolean;
  optional?: boolean;
  /** The pin is the board's USB serial (uploads and the Faber link). */
  serial?: boolean;
  /** An inner header row: the wire first steps this way, between the outer row's pins. */
  jog?: [number, number];
  /** Where the pin is, in words, for the build steps. */
  place?: string;
  hideLabel?: boolean;
  pairLabel?: string;
  /** A screw terminal rather than a header pin. */
  terminal?: boolean;
  /** Part of a power inlet (the Pi's USB-C): only for powering the board, never a pin to share. */
  inlet?: boolean;
}

export type PartKind = 'board' | 'part' | 'breadboard';

export interface PartDef {
  id: string;
  name: string;
  covers: string;
  kind: PartKind;
  /** Which drawing (see ui/looks.ts); `generic` draws a plain board in `color`. */
  look: string;
  w: number;
  h: number;
  title?: string;
  /** Header strips drawn under the pins: [x0, y0, x1, y1]. */
  strips?: [number, number, number, number][];
  pins: PinDef[];
  color?: string;
  /** A picture of the top view (a data URL), for parts people add. */
  art?: string | null;
  /** Pins joined inside the part: a breadboard's strips. */
  buses?: string[][];
  /** Pins that pass a signal through: a level shifter's LVn and HVn. */
  channels?: [string, string][];
}

export interface Part {
  id: string;
  def: string;
  name: string;
  x: number;
  y: number;
  rot: Dir;
}

/** A wire between two pins, each `partId.pinId`. */
export interface Wire {
  id: string;
  a: string;
  b: string;
  color?: string;
}

export interface Diagram {
  name: string;
  parts: Part[];
  wires: Wire[];
  /** Build steps ticked off, by wire id (and `plug:partId` for parts plugged in). */
  done: Record<string, boolean>;
  seq: number;
}

export type DefLookup = (id: string) => PartDef | undefined;

export function splitKey(key: string): [string, string] {
  const i = key.indexOf('.');
  return [key.slice(0, i), key.slice(i + 1)];
}
