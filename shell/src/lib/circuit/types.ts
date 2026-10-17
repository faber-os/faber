/**
 * The wiring diagram's data (docs/wiring-diagram.md). Positions are in grid units: 1 unit =
 * 0.05 in = 1.27 mm, half a header pitch, so a wire can pass between two pins.
 */

/** The way a wire leaves a pin: up, right, down, left. */
export type Dir = 0 | 1 | 2 | 3;

/** What a board pin can do. */
export type Cap =
  'io' | 'pwm' | 'analog' | 'sda' | 'scl' | 'int' | 'tx' | 'rx' | 'sck' | 'mosi' | 'miso' | 'audio';

/** A socket a cable plugs into. `usb` carries data and 5 V from the host; `usbc-power` only power. */
export type PortType = 'usb' | 'usbc-power' | 'hdmi' | 'csi';

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
  /** A socket (see `port` and `host`). */
  | 'port'
  /** A breadboard hole. */
  | 'hole'
  /** Only conducts, through the part to its partner (see `passes`): a fuse's ends, a BMS's B+ and P+. */
  | 'pass'
  /** Out to a speaker, and the speaker's own pins. */
  | 'speaker-out'
  | 'speaker-in';

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
  /** Part of a power inlet: only for powering the board, never a pin to share. */
  inlet?: boolean;
  /** `port` pins: the kind of socket, and whether it is the host end (gives power). */
  port?: PortType;
  host?: boolean;
  /** Current drawn (power-in pins) or the most a supply pin gives, in mA. */
  ma?: number;
  maxMa?: number;
  /** The highest voltage that does no harm (above `max` only warns). */
  limit?: number;
  /** Something to know the moment this pin is wired (a MOSFET that isn't logic level). */
  caution?: string;
  /** A pin that must be held low (an enable, a gate) while its board boots: advice adds a pull-down. */
  pull?: 'down';
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
  /** Pins joined through the part while it is closed (a fuse, a switch, a connector). */
  passes?: [string, string][];
  /** Whether a click in power mode opens and closes it, and if it is open by default. */
  switch?: 'toggle' | 'push';
  /** A board with legs: it plugs into a breadboard (a Nano). */
  pluggable?: boolean;
  /** Settings that change the part (a battery's cells): see parts.ts `variant`. */
  params?: ParamDef[];
  /** Notes shown in the inspector. */
  notes?: string[];
  /** How it behaves electrically beyond its pin kinds (see circuit.ts). */
  elec?: Elec;
  /** A fuse's or BMS's trip current, in A: above it the part opens. */
  trip?: number;
}

/**
 * The electrical model of a two-legged part. Values are datasheet or typical figures (the part's
 * notes say which).
 */
export type Elec =
  | { t: 'resistor'; a: string; b: string; ohms: number; watts: number }
  /** A diode or LED: `a` anode (+), `k` cathode (−). LEDs carry their colour. */
  | {
      t: 'diode';
      a: string;
      k: string;
      vf: number;
      rs: number;
      maxMa: number;
      led?: string;
      ma?: number;
    }
  /** A TVS diode: open until the voltage reaches `vbr`, then it clamps. */
  | { t: 'tvs'; a: string; k: string; vbr: number }
  | { t: 'cap'; a: string; b: string; uf: number; volts: number; polar: boolean }
  /** A brushed motor: its current while running freely and with the shaft held, at `volts`. */
  | { t: 'motor'; a: string; b: string; volts: number; runA: number; stallA: number };

export interface ParamDef {
  id: string;
  label: string;
  unit?: string;
  min?: number;
  max?: number;
  step?: number;
  default: number;
}

export interface Part {
  id: string;
  /** The part type; a parametric one carries its settings: `bat?s=3&p=6&cell=4.2`. */
  def: string;
  name: string;
  x: number;
  y: number;
  rot: Dir;
  /** A switch or button that is open (power mode). */
  open?: boolean;
}

/** A wire between two pins, each `partId.pinId`. */
export interface Wire {
  id: string;
  a: string;
  b: string;
  color?: string;
  /** The wire's thickness; without one it is a Dupont jumper between pins, 22 AWG on a terminal. */
  gauge?: Gauge;
  /** Its real length in metres (default 0.25 m). */
  len?: number;
}

/** The wires in SENNEP's drawer: Dupont jumpers and 22, 16 and 10 AWG cable. */
export type Gauge = 'dupont' | '22' | '16' | '10';

export interface Diagram {
  name: string;
  parts: Part[];
  wires: Wire[];
  /** Build steps ticked off, by wire id (and `plug:partId` for parts plugged in). */
  done: Record<string, boolean>;
  seq: number;
  /** Board pins the code sets to INPUT_PULLUP (no pull-up resistor needed). */
  pullups?: string[];
  /** Faber parts that couldn't be drawn (from Faber's parts: no drawing for them yet). */
  skipped?: string[];
}

export type DefLookup = (id: string) => PartDef | undefined;

export function splitKey(key: string): [string, string] {
  const i = key.indexOf('.');
  return [key.slice(0, i), key.slice(i + 1)];
}
