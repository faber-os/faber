# The wiring diagram

Status: **proposed (D60)**. Prototype: `docs/prototypes/wiring-canvas.html`, published as an artifact
for Mikkel on 2026-10-17. Nothing of this is in the shell yet.

## 1. The idea

A guided board (docs/wiring.md, D57) already knows its board, its parts, which pin each part's roles
use and how each part is powered. That is all a wiring diagram needs. So Faber **draws** the
diagram from the plan instead of asking anyone to draw it:

- every part appears around the board, turned so its pins face the board;
- every wire finds its own path: around parts, never on top of another wire, hopping where two
  cross, sharing track only with wires of the same net (a bus, with a junction dot where it splits);
- colours follow what a wire carries (red 5 V, orange 3.3 V, crimson battery, black ground,
  distinct colours for signals), and a person can pick the colour of the wire in their hand;
- the same checks the plan already runs are shown on the drawing: shorts, two supplies joined,
  voltage out of a part's range, a PWM job on a pin that can't, 5 V logic into a 3.3 V pin,
  a signal on a power pin, a motor on a board pin, a battery whose minus never meets the board's
  ground, a USB-serial pin in use;
- **Build** turns it into a tick list (ground, then power, then signals, part by part), with where
  each wire goes ("right double header") and when several wires meet one pin ("use a Wago or a
  breadboard rail"). Ticking a part done is where the part test plugs in later: the sonar reads a
  distance, the motor turns a little with the wheels off the ground.

Code-mode boards (no plan) can use the same canvas by hand: drop parts, click pin to pin.

## 2. What a part needs to be drawn

Two levels, so anyone can add a part:

1. **Generic** (nothing but numbers): the part's size in mm and which edge each pin is on. Faber
   draws a board in a chosen colour with the pins 2.54 mm apart, centred on their edge.
2. **A drawing**: an SVG (or PNG) of the top view, shipped beside the part file. Pins are still
   placed by the numbers, so a drawing can never move a pin.

In `parts/*.toml` that is one new table (what the prototype's "New part" dialog writes out):

```toml
[drawing]
size-mm = [45.0, 20.0]
art = "hc-sr04.svg"   # optional; without it, `colour` and a plain board
pins = [
  { id = "VCC", edge = "bottom", at-mm = 19.05 },
  { id = "TRIG", edge = "bottom", at-mm = 21.59 },
  { id = "ECHO", edge = "bottom", at-mm = 24.13 },
  { id = "GND", edge = "bottom", at-mm = 26.67 },
]
```

What a pin **is** (power in with a range, ground, a signal that needs PWM, a 5 V output) already
lives in the part file's roles and `[power]`; the drawing only says where it is. Boards already
have `[[headers]]` (pin order along each edge); they gain positions in mm and the inner/outer row
of a double header.

## 3. How it is laid out and routed (as in the prototype)

- **Grid:** 1.27 mm (half a header pitch), so a wire from an inner header row can leave between
  two pins of the outer row, as a real jumper does.
- **Arrange:** the board in the middle; each part on the side where its board pins are (signal
  pins decide, not power), in the order of those pins; a top or bottom row much wider than the
  board spills its outermost parts to the sides; parts that only meet other parts (a motor, a
  battery) go beyond their busiest partner, facing it. Then every part is nudged until it keeps
  8 grid units (1 cm) from every other.
- **Routing:** A* over (cell, direction) with costs for length, bends and crossings and a small
  cost for running right next to another wire. Walls: every part plus one unit. Each pin first
  gets a fixed stub straight out (stepping sideways first in an inner header row). Signals route
  first, shortest first; power and ground last, so they gather into buses. A wire with no free
  path is drawn dashed red and listed in Checks. 27 wires take about 17 ms in a browser.
- **Drawing:** rounded corners, a hop on the horizontal wire wherever another net's vertical wire
  crosses, a casing in the mat's colour under each wire so crossings read clearly.

## 4. Where it would live in Faber

A **Wiring** tab on a guided board (next to Pins and Settings) and a window of its own for a whole
machine (several boards, the Pi in the middle). The core serves the plan; the shell does layout and
routing (it is drawing, not machine state), so nothing new crosses the API except the `[drawing]`
tables. A person's moves and colour picks are saved with the plan.

## 5. Not decided (for D60)

1. **Ship the drawings or make them?** Recommended: Faber ships simple SVGs for its own parts
   (from the parts' datasheet sizes) and accepts SVGs in community packages. Uploading pictures in
   the shell is only for a person's own parts.
2. **One diagram per board or per machine?** Recommended: per board first (that is what the guided
   plan knows); a machine view with the Pi, power and every board after the automations work.
3. **Breadboard view?** Fritzing draws wires into breadboard holes. Recommended: no. Faber's
   wires go pin to pin, and a shared pin says "Wago or rail" in the build steps.
4. **Physical layout (where parts sit on the robot)?** Out of scope: that is CAD. The diagram is
   about which pin goes where.

## 6. What the prototype leaves out

Real board drawings (the boards are drawn from rectangles), the Pi's power and the Mega's USB link
as wires, rip-up-and-retry when a wire can't find a path, touch pinch-zoom, and saving anywhere
but the browser.
