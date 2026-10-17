/**
 * A sketch running on a board of the drawing (docs/circuits.md §5): the emulator steps in time
 * with the screen, its pins go to the circuit (power mode switches on), and the voltages the
 * circuit puts on the board's pins go back in as its inputs, ten times a second.
 */
import { emulate, type Emulator } from '../../lib/circuit/avr';
import type { PinState } from '../../lib/circuit/circuit';
import type { CircuitStore } from './circuit.svelte';

const FEED_MS = 100;
const SERIAL_KEEP = 20_000;

export class SketchRunner {
  /** The board part it runs on, the sketch's name. */
  part = $state<string | null>(null);
  sketch = $state('');
  running = $state(false);
  /** Simulated seconds, and how fast against real time (1 = real time). */
  seconds = $state(0);
  speed = $state(1);
  serial = $state('');
  private emu: Emulator | null = null;
  private frame = 0;
  private lastFeed = 0;

  constructor(private lab: CircuitStore) {}

  start(part: string, board: 'uno' | 'nano' | 'mega', hex: string, sketch: string): void {
    this.stop();
    this.part = part;
    this.sketch = sketch;
    this.serial = '';
    this.emu = emulate(board, hex);
    this.emu.onSerial = (c) => {
      const next = this.serial + c;
      this.serial = next.length > SERIAL_KEEP ? next.slice(-SERIAL_KEEP) : next;
    };
    this.running = true;
    this.lab.power = true;
    let last = performance.now();
    const step = (now: number) => {
      if (!this.running || !this.emu) return;
      const want = Math.min(50, now - last);
      last = now;
      const ran = this.emu.run(want, 12);
      this.speed = want > 0 ? Math.min(1, ran / want) : 1;
      this.seconds = this.emu.seconds;
      if (now - this.lastFeed > FEED_MS) {
        this.lastFeed = now;
        this.feed();
      }
      this.frame = requestAnimationFrame(step);
    };
    this.frame = requestAnimationFrame(step);
  }

  /** Pins out to the circuit, the circuit's voltages back in. */
  private feed(): void {
    const emu = this.emu;
    const part = this.part;
    if (!emu || !part) return;
    const pins = new Map<string, PinState>();
    const pullups: string[] = [];
    for (const [pin, st] of emu.pinStates()) {
      pins.set(`${part}.${pin}`, st);
      if (st.mode === 'pullup') pullups.push(pin);
    }
    this.lab.pins = pins;
    const volts = new Map<string, number>();
    // A pulled-up pin with nothing on it reads HIGH, like on the real board.
    for (const pin of pullups) volts.set(pin, 5);
    const sim = this.lab.sim;
    if (!sim) return emu.setInputs(volts);
    const prefix = `${part}.`;
    for (const [key, v] of sim.volts)
      if (key.startsWith(prefix)) volts.set(key.slice(prefix.length), v);
    emu.setInputs(volts);
  }

  write(text: string): void {
    this.emu?.write(text);
  }

  stop(): void {
    this.running = false;
    cancelAnimationFrame(this.frame);
    this.emu = null;
    this.lab.pins = new Map();
  }
}
