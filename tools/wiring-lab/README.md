# Wiring lab

The wiring diagram (docs/wiring-diagram.md, D60), built on its own before it moves into the shell.

```bash
npm install --offline   # the packages are the shell's, so the npm cache has them
npm run dev             # http://localhost:5180
npm test                # the routing, checks, layout and connect logic
npm run check           # types
```

- `src/lib/`: no UI. Parts (`parts.ts`), geometry, nets (wires + breadboard strips + plugged
  pins), the router, the checks, "Connect to…" (`connect.ts`), Arrange (`layout.ts`) and the
  build steps. All tested in `lab.test.ts`.
- `src/ui/`: Svelte 5. `lab.svelte.ts` holds the state; everything worked out from the drawing is
  `$derived` from a plain snapshot of it.

The drawing is saved in the browser (localStorage). "Example" brings back SENNEP's front-left
corner with the pins from its firmware.
