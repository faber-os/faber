# Circuits

Status: **D61 proposed** (built 2026-10-17, waiting for Mikkel). Circuits is the wiring diagram of
D60 grown into a program: draw how the parts are wired, switch the power on, and see what
happens, down to which resistor is missing and how big it must be. The aim is Faber's own:
someone who has never held a soldering iron should learn electronics by doing it here first.

It runs wherever the shell runs. Faber doesn't need a Raspberry Pi: in a browser on a PC with an
Arduino on USB, Circuits draws "this computer" where a Pi would be.

## 1. Where it lives

| What | Where |
|---|---|
| The program | Start menu → Tools → **Circuits** (`tool:circuit`), one drawing per window |
| A drawing | a `.fbcircuit` file in Files (JSON: `{faberCircuit: 1, doc, custom}`); double-click opens it |
| Logic | `shell/src/lib/circuit/`: parts, nets, routing, checks, advice, fixes, the circuit simulation, the emulator. Pure TypeScript; imports nothing else of the shell |
| UI | `shell/src/windows/CircuitWindow.svelte` (files, the core) and `shell/src/windows/circuit/` (the editor) |
| Tests | `shell/src/lib/circuit/*.test.ts`; real compiled sketches in `lib/circuit/fixtures/` |

The standalone lab (`tools/wiring-lab`) was its first home and is gone; its history is in git.

**Texts.** Everything the editor says is in the catalogue (`circuit.*`). The parts catalogue
(names, notes) and the electronics the checks explain are English data for now, like the Arduino
editor's API catalogue (`noRawText` skips `lib/circuit/`). Translating them needs message ids per
part and per check: not done.

## 2. The circuit simulation (power mode)

`lib/circuit/circuit.ts` turns the drawing into a network and solves it with nodal analysis:
volts at every pin, amps in every wire.

| Becomes | What |
|---|---|
| A wire | copper: its gauge (Dupont jumper, 22, 16 or 10 AWG, the wires in SENNEP's drawer) × its real length (default 0.25 m) |
| A battery, an adapter | a source behind its internal resistance (25 mΩ per 18650 cell) |
| A regulator (buck, 7805, LD1117, a board's VIN and 3V3) | an output that is on while its input is in range; its input draws what the output gives (a buck at 88 %) |
| A part's power pin | a load that runs inside its range and draws its rated current |
| A board pin set by code | a source through about 25 Ω (Arduino) or 40 Ω (Pi); INPUT_PULLUP is 35 kΩ |
| A resistor, LED, diode, TVS, capacitor | itself (capacitors are open at DC; their voltage and polarity are checked) |
| A fuse, BMS | a few milliohms that open above their trip current, and the circuit is solved again |
| A relay | a coil load; 30 meets 87 while the coil has 8 V or more |
| A motor driver + motor | off, driving (2 A per SENNEP motor) or held still (an estimated 20 A, not measured) |

It reports, in words a beginner follows: too much or too little voltage, a supply or pin asked
for more current than it gives, a linear regulator burning more than 1 W, a resistor over its
watts, an LED over its current, a wire over what it carries, 5 V on a 3.3 V pin, a capacitor
over its voltage or backwards.

## 3. Where a resistor belongs (advice)

`lib/circuit/advice.ts` says where a resistor or diode is missing, how big, and why, and its fix
adds the real part with its wires (one click, previewed on hover, undoable):

| Advice | Value |
|---|---|
| An LED needs a series resistor | (supply − LED volts) ÷ 10 mA, rounded up to the next E12 value |
| A switch or open-collector input needs a pull-up | "INPUT_PULLUP in the code" (no part) or 10 kΩ to 5 V |
| An enable or gate needs a pull-down while the board boots | 10 kΩ to GND |
| A voltage above a pin's logic needs a divider | 10 kΩ below, the E12 value above that keeps it under 90 % of the logic |
| 5 V into a 3.3 V input | a divider (1.8 kΩ / 3.3 kΩ) or a level shifter |
| A relay coil needs a diode | 1N4007, stripe to 86 |

Resistors come from the E12 series; the resistor kit's actual values aren't known yet.

## 4. Faber's parts on the drawing

Add ▾ → *The parts in Faber* draws what Faber runs: a FaberLink board as its Arduino (the type
read from its name, since the core doesn't report it), the parts on it as their modules, wired to
the pins the board reports (`PINS`); the LD19 with its USB adapter; everything on USB hanging off
the Pi (when Faber runs on real buses) or "this computer". A part on the drawing with the same name
as a Faber part shows that part's live state (`FABER: running`).

## 5. Running a sketch

Code ▾ → *Run a sketch on <board>…* compiles the sketch with Faber's Arduino tools
(`POST /api/v0/arduino/compile` with `hex: true` returns the program) and runs it in the window
on avr8js (MIT): ATmega328P for the Uno and Nano, ATmega2560 for the Mega, 16 MHz, with timers
(millis, PWM), all USARTs and the ADC. Ten times a second its pins go into the circuit (a PWM pin
as its average) and the circuit's voltages come back as its inputs, so a sketch that lights an
LED lights it on the drawing, and a button pressed in power mode is read by the sketch. Serial
shows under the toolbar, and text can be sent to it.

The 2560's peripherals are avr8js's blocks at the 2560's addresses and vectors, written for
Circuits: the test sketch runs on both, but less of the Mega is proven. Not emulated: I²C and
SPI devices (a sketch waiting for a sensor waits), EEPROM, the watchdog.

## 6. Open

- The whole-machine layout: SENNEP V2 has a hand layout by job; 12 of its 203 wires find no route
  and are drawn straight. Arrange packs a machine that size too tightly.
- Texts of parts and checks are English (§1).
- Simulated sensors: a sonar doesn't answer the emulated sketch yet.
- The live view shows each Faber part's state, not its readings.
