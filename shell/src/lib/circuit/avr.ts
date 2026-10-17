/**
 * Runs an Arduino sketch in the browser (avr8js, MIT): the compiled program on a simulated
 * ATmega328P (Uno, Nano) or ATmega2560 (Mega) at 16 MHz, with its ports, timers (PWM, millis),
 * serial and ADC. Each pin's state goes to the circuit simulation, and the circuit's voltages
 * come back as the pins' inputs, so a sketch that lights an LED lights the LED on the drawing.
 *
 * The 328P's peripherals are avr8js's own. The 2560's are the same blocks at the 2560's
 * addresses and interrupt vectors (ATmega2560 datasheet, "Interrupts" and "Register Summary"),
 * written for this file: the Mega runs, but less of it is proven than the Uno.
 */
import {
  ADCMuxInputType,
  AVRADC,
  AVRIOPort,
  AVRTimer,
  AVRUSART,
  adcConfig,
  avrInstruction,
  CPU,
  PinState,
  portAConfig,
  portBConfig,
  portCConfig,
  portDConfig,
  portEConfig,
  portFConfig,
  portGConfig,
  portHConfig,
  portJConfig,
  portKConfig,
  portLConfig,
  timer0Config,
  timer1Config,
  timer2Config,
  usart0Config,
  type ADCConfig,
  type AVRPortConfig,
  type AVRTimerConfig,
} from 'avr8js';
import type { PinState as CircuitPin } from './circuit';

export type Chip = 'atmega328p' | 'atmega2560';

const HZ = 16_000_000;

const PORTS: Record<string, AVRPortConfig> = {
  A: portAConfig,
  B: portBConfig,
  C: portCConfig,
  D: portDConfig,
  E: portEConfig,
  F: portFConfig,
  G: portGConfig,
  H: portHConfig,
  J: portJConfig,
  K: portKConfig,
  L: portLConfig,
};

/** Arduino pin (as the drawing names it) → port and bit, from the Arduino AVR core's variants. */
const UNO_PINS: Record<string, [string, number]> = {
  ...Object.fromEntries(
    [0, 1, 2, 3, 4, 5, 6, 7].map((n) => [`D${n}`, ['D', n] as [string, number]]),
  ),
  ...Object.fromEntries(
    [8, 9, 10, 11, 12, 13].map((n) => [`D${n}`, ['B', n - 8] as [string, number]]),
  ),
  ...Object.fromEntries([0, 1, 2, 3, 4, 5].map((n) => [`A${n}`, ['C', n] as [string, number]])),
  SDA: ['C', 4],
  SCL: ['C', 5],
};

const MEGA_DIGITAL: [string, number][] = [
  ['E', 0],
  ['E', 1],
  ['E', 4],
  ['E', 5],
  ['G', 5],
  ['E', 3],
  ['H', 3],
  ['H', 4],
  ['H', 5],
  ['H', 6], // D0–9
  ['B', 4],
  ['B', 5],
  ['B', 6],
  ['B', 7],
  ['J', 1],
  ['J', 0],
  ['H', 1],
  ['H', 0],
  ['D', 3],
  ['D', 2], // D10–19
  ['D', 1],
  ['D', 0],
  ['A', 0],
  ['A', 1],
  ['A', 2],
  ['A', 3],
  ['A', 4],
  ['A', 5],
  ['A', 6],
  ['A', 7], // D20–29
  ['C', 7],
  ['C', 6],
  ['C', 5],
  ['C', 4],
  ['C', 3],
  ['C', 2],
  ['C', 1],
  ['C', 0],
  ['D', 7],
  ['G', 2], // D30–39
  ['G', 1],
  ['G', 0],
  ['L', 7],
  ['L', 6],
  ['L', 5],
  ['L', 4],
  ['L', 3],
  ['L', 2],
  ['L', 1],
  ['L', 0], // D40–49
  ['B', 3],
  ['B', 2],
  ['B', 1],
  ['B', 0], // D50–53
];
const MEGA_PINS: Record<string, [string, number]> = {
  ...Object.fromEntries(MEGA_DIGITAL.map((p, n) => [`D${n}`, p])),
  ...Object.fromEntries(
    [0, 1, 2, 3, 4, 5, 6, 7].map((n) => [`A${n}`, ['F', n] as [string, number]]),
  ),
  ...Object.fromEntries(
    [0, 1, 2, 3, 4, 5, 6, 7].map((n) => [`A${n + 8}`, ['K', n] as [string, number]]),
  ),
  SDA: ['D', 1],
  SCL: ['D', 0],
};

/** Interrupt vector `n` (1 = RESET, as the datasheet numbers them) as a program address. */
const vec = (n: number) => (n - 1) * 2;

function megaTimer(
  base: AVRTimerConfig,
  r: {
    A: number;
    B: number;
    C: number;
    ICR: number;
    TCNT: number;
    TCCRA: number;
    TIMSK: number;
    TIFR: number;
  },
  v: number,
  oc: [string, number][],
): AVRTimerConfig {
  return {
    ...base,
    bits: 16,
    captureInterrupt: vec(v),
    compAInterrupt: vec(v + 1),
    compBInterrupt: vec(v + 2),
    compCInterrupt: vec(v + 3),
    ovfInterrupt: vec(v + 4),
    TIFR: r.TIFR,
    OCRA: r.A,
    OCRB: r.B,
    OCRC: r.C,
    ICR: r.ICR,
    TCNT: r.TCNT,
    TCCRA: r.TCCRA,
    TCCRB: r.TCCRA + 1,
    TCCRC: r.TCCRA + 2,
    TIMSK: r.TIMSK,
    compPortA: PORTS[oc[0]![0]]!.PORT,
    compPinA: oc[0]![1],
    compPortB: PORTS[oc[1]![0]]!.PORT,
    compPinB: oc[1]![1],
    compPortC: PORTS[oc[2]![0]]!.PORT,
    compPinC: oc[2]![1],
    externalClockPort: 0,
    externalClockPin: 0,
  };
}

function megaTimers(): AVRTimerConfig[] {
  return [
    {
      ...timer0Config,
      compAInterrupt: vec(22),
      compBInterrupt: vec(23),
      ovfInterrupt: vec(24),
      compPortA: portBConfig.PORT,
      compPinA: 7,
      compPortB: portGConfig.PORT,
      compPinB: 5,
      externalClockPort: 0,
      externalClockPin: 0,
    },
    megaTimer(
      timer1Config,
      { A: 0x88, B: 0x8a, C: 0x8c, ICR: 0x86, TCNT: 0x84, TCCRA: 0x80, TIMSK: 0x6f, TIFR: 0x36 },
      17,
      [
        ['B', 5],
        ['B', 6],
        ['B', 7],
      ],
    ),
    {
      ...timer2Config,
      compAInterrupt: vec(14),
      compBInterrupt: vec(15),
      ovfInterrupt: vec(16),
      compPortA: portBConfig.PORT,
      compPinA: 4,
      compPortB: portHConfig.PORT,
      compPinB: 6,
    },
    megaTimer(
      timer1Config,
      { A: 0x98, B: 0x9a, C: 0x9c, ICR: 0x96, TCNT: 0x94, TCCRA: 0x90, TIMSK: 0x71, TIFR: 0x38 },
      32,
      [
        ['E', 3],
        ['E', 4],
        ['E', 5],
      ],
    ),
    megaTimer(
      timer1Config,
      { A: 0xa8, B: 0xaa, C: 0xac, ICR: 0xa6, TCNT: 0xa4, TCCRA: 0xa0, TIMSK: 0x72, TIFR: 0x39 },
      42,
      [
        ['H', 3],
        ['H', 4],
        ['H', 5],
      ],
    ),
    megaTimer(
      timer1Config,
      {
        A: 0x128,
        B: 0x12a,
        C: 0x12c,
        ICR: 0x126,
        TCNT: 0x124,
        TCCRA: 0x120,
        TIMSK: 0x73,
        TIFR: 0x3a,
      },
      47,
      [
        ['L', 3],
        ['L', 4],
        ['L', 5],
      ],
    ),
  ];
}

const usart = (rx: number, regs: number) => ({
  rxCompleteInterrupt: vec(rx),
  dataRegisterEmptyInterrupt: vec(rx + 1),
  txCompleteInterrupt: vec(rx + 2),
  UCSRA: regs,
  UCSRB: regs + 1,
  UCSRC: regs + 2,
  UBRRL: regs + 4,
  UBRRH: regs + 5,
  UDR: regs + 6,
});

function megaAdc(): ADCConfig {
  const channels: ADCConfig['muxChannels'] = {};
  for (let i = 0; i < 8; i++) {
    channels[i] = { type: ADCMuxInputType.SingleEnded, channel: i };
    channels[0x20 + i] = { type: ADCMuxInputType.SingleEnded, channel: 8 + i };
  }
  channels[0x1e] = { type: ADCMuxInputType.Constant, voltage: 1.1 };
  channels[0x1f] = { type: ADCMuxInputType.Constant, voltage: 0 };
  return {
    ...adcConfig,
    adcInterrupt: vec(30),
    numChannels: 16,
    muxInputMask: 0x3f,
    muxChannels: channels,
  };
}

/** Reads Intel HEX into program memory (words). */
export function loadHex(hex: string, flashBytes: number): Uint16Array {
  const bytes = new Uint8Array(flashBytes);
  let base = 0;
  for (const raw of hex.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line.startsWith(':')) continue;
    const n = parseInt(line.slice(1, 3), 16);
    const addr = parseInt(line.slice(3, 7), 16);
    const type = parseInt(line.slice(7, 9), 16);
    if (type === 0) {
      for (let i = 0; i < n; i++) {
        const at = base + addr + i;
        if (at < flashBytes) bytes[at] = parseInt(line.slice(9 + i * 2, 11 + i * 2), 16);
      }
    } else if (type === 2) base = parseInt(line.slice(9, 13), 16) << 4;
    else if (type === 4) base = parseInt(line.slice(9, 13), 16) << 16;
    else if (type === 1) break;
  }
  return new Uint16Array(bytes.buffer);
}

export interface Emulator {
  readonly chip: Chip;
  /** Simulated time, in seconds. */
  readonly seconds: number;
  /** Runs for `ms` of simulated time (stops early past `budgetMs` of real time); returns ms run. */
  run(ms: number, budgetMs?: number): number;
  /** Each pin's state since the last call: outputs with their average level (PWM), inputs, pull-ups. */
  pinStates(): Map<string, CircuitPin>;
  /** What the circuit puts on the pins: volts, by pin id. */
  setInputs(volts: Map<string, number>): void;
  /** Bytes the sketch writes to Serial (USB). */
  onSerial: ((text: string) => void) | null;
  /** Sends text to the sketch's Serial. */
  write(text: string): void;
}

export function emulate(board: 'uno' | 'nano' | 'mega', hex: string): Emulator {
  const mega = board === 'mega';
  const chip: Chip = mega ? 'atmega2560' : 'atmega328p';
  const program = loadHex(hex, mega ? 256 * 1024 : 32 * 1024);
  // The 2560's extended I/O reaches 0x1FF, so its data space is 0x2200 bytes.
  const cpu = new CPU(program, mega ? 0x2100 : 0x800);
  const map = mega ? MEGA_PINS : UNO_PINS;
  const letters = mega ? ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L'] : ['B', 'C', 'D'];
  const ports = new Map(letters.map((l) => [l, new AVRIOPort(cpu, PORTS[l]!)]));
  for (const t of mega ? megaTimers() : [timer0Config, timer1Config, timer2Config])
    new AVRTimer(cpu, t);
  const serial = new AVRUSART(cpu, mega ? usart(26, 0xc0) : usart0Config, HZ);
  if (mega) {
    new AVRUSART(cpu, usart(37, 0xc8), HZ);
    new AVRUSART(cpu, usart(52, 0xd0), HZ);
    new AVRUSART(cpu, usart(55, 0x130), HZ);
  }
  const adc = new AVRADC(cpu, mega ? megaAdc() : adcConfig);
  const analog = new Map<string, number>(
    Object.keys(map)
      .filter((k) => /^A\d+$/.test(k))
      .map((k) => [k, Number(k.slice(1))]),
  );

  // Time each pin spends high, to turn PWM into an average level.
  const high = new Map<string, number>();
  const since = new Map<string, number>();
  let windowStart = 0;
  const byPort = new Map<string, [string, number][]>();
  for (const [pin, [l, bit]] of Object.entries(map))
    byPort.set(l, [...(byPort.get(l) ?? []), [pin, bit]]);
  for (const [l, port] of ports) {
    const pins = byPort.get(l) ?? [];
    port.addListener((value, old) => {
      for (const [pin, bit] of pins) {
        const was = (old >> bit) & 1;
        const now = (value >> bit) & 1;
        if (was === now) continue;
        if (was) high.set(pin, (high.get(pin) ?? 0) + cpu.cycles - (since.get(pin) ?? windowStart));
        else since.set(pin, cpu.cycles);
      }
    });
  }

  const emu: Emulator = {
    chip,
    get seconds() {
      return cpu.cycles / HZ;
    },
    onSerial: null,
    run(ms, budgetMs = 30) {
      const until = cpu.cycles + Math.round((ms / 1000) * HZ);
      const start = performance.now();
      while (cpu.cycles < until) {
        for (let i = 0; i < 20000 && cpu.cycles < until; i++) {
          avrInstruction(cpu);
          cpu.tick();
        }
        if (performance.now() - start > budgetMs) break;
      }
      return ((cpu.cycles - (until - Math.round((ms / 1000) * HZ))) / HZ) * 1000;
    },
    pinStates() {
      const out = new Map<string, CircuitPin>();
      const span = Math.max(1, cpu.cycles - windowStart);
      for (const [pin, [l, bit]] of Object.entries(map)) {
        const st = ports.get(l)!.pinState(bit);
        if (st === PinState.Input) continue;
        if (st === PinState.InputPullUp) {
          out.set(pin, { mode: 'pullup' });
          continue;
        }
        let h = high.get(pin) ?? 0;
        if (st === PinState.High)
          h += cpu.cycles - Math.max(since.get(pin) ?? windowStart, windowStart);
        const level = Math.min(1, Math.max(0, h / span));
        out.set(pin, { mode: 'out', level: Math.round(level * 100) / 100 });
      }
      high.clear();
      windowStart = cpu.cycles;
      for (const k of [...since.keys()]) since.set(k, windowStart);
      return out;
    },
    setInputs(volts) {
      for (const [pin, v] of volts) {
        const at = map[pin];
        if (!at) continue;
        ports.get(at[0])?.setPin(at[1], v > 2.5);
        const ch = analog.get(pin);
        if (ch !== undefined) adc.channelValues[ch] = Math.max(0, Math.min(5, v));
      }
    },
    write(text) {
      for (const c of new TextEncoder().encode(text)) serial.writeByte(c);
    },
  };
  serial.onByteTransmit = (b) => emu.onSerial?.(String.fromCharCode(b));
  return emu;
}

/** The Arduino board an emulator runs for a drawing's board part. */
export function emulatedBoard(defId: string): 'uno' | 'nano' | 'mega' | null {
  return defId === 'mega' ? 'mega' : defId === 'uno' ? 'uno' : defId === 'nano' ? 'nano' : null;
}
