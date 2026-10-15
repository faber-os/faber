import { describe, expect, it } from 'vitest';
import {
  buildStatic,
  drawWorld,
  machineOnScreen,
  type PathLike,
  type WorldPalette,
  type WorldScene,
} from './worldDraw';
import type { Camera } from './worldView';

class PathRecorder implements PathLike {
  ops: string[] = [];
  moveTo(x: number, y: number) {
    this.ops.push(`M${x},${y}`);
  }
  lineTo(x: number, y: number) {
    this.ops.push(`L${x},${y}`);
  }
  closePath() {
    this.ops.push('Z');
  }
  rect(x: number, y: number, w: number, h: number) {
    this.ops.push(`R${x},${y},${w},${h}`);
  }
}

/** Records what is filled and stroked, in which colour; enough to check layers, not pixels. */
function recorder() {
  const log: string[] = [];
  let fill = '';
  let stroke = '';
  let rects = 0;
  const ctx = {
    set fillStyle(v: string) {
      fill = v;
    },
    set strokeStyle(v: string) {
      stroke = v;
    },
    set lineWidth(_v: number) {},
    set globalAlpha(_v: number) {},
    set lineJoin(_v: string) {},
    set lineCap(_v: string) {},
    set font(_v: string) {},
    set textAlign(_v: string) {},
    set textBaseline(_v: string) {},
    setLineDash() {},
    setTransform() {},
    fillRect() {
      rects++;
    },
    beginPath() {},
    arc() {},
    moveTo() {},
    lineTo() {},
    closePath() {},
    fill(path?: PathRecorder) {
      log.push(`fill ${fill}${path ? ` ${path.ops[0]}` : ''}`);
    },
    stroke(path?: PathRecorder) {
      log.push(`stroke ${stroke}${path ? ` ${path.ops[0]}` : ''}`);
    },
    fillText(text: string) {
      log.push(`text ${text}`);
    },
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, log, rects: () => rects };
}

const palette: WorldPalette = {
  background: 'bg',
  floor: 'floor',
  grid: 'grid',
  wall: 'wall',
  shape: 'shape',
  overhead: 'overhead',
  label: 'label',
  machine: 'machine',
  heading: 'heading',
  trail: 'trail',
  mover: 'mover',
  scan: 'scan',
  bumped: 'bumped',
  cone: { ok: 'cone-ok', near: 'cone-near', none: 'cone-none', bad: 'cone-bad' },
};

const geometry = {
  bounds: { min: [0, 0], max: [4, 3] } as { min: [number, number]; max: [number, number] },
  walls: [{ a: [0, 0], b: [4, 0] } as { a: [number, number]; b: [number, number] }],
  shapes: [
    {
      points: [
        [1, 1],
        [2, 1],
        [2, 2],
      ] as [number, number][],
    },
  ],
  overhead: [],
};

const scene = (over: Partial<WorldScene> = {}): WorldScene => ({
  pose: { x: 2, y: 1.5, yaw: 0 },
  footprint: { length: 0.8, width: 0.65 },
  bumped: false,
  movers: [{ x: 3, y: 2, radius: 0.25 }],
  trail: [],
  scans: [],
  cones: [],
  ...over,
});

const cam: Camera = { scale: 100, cx: 2, cy: 1.5 };
const size = { width: 400, height: 300 };

describe('the static layer', () => {
  it('turns walls, shapes and the floor into world-coordinate paths once', () => {
    const layer = buildStatic(geometry, () => new PathRecorder());
    expect(layer.floor.ops).toEqual(['R0,0,4,3']);
    expect(layer.walls.ops).toEqual(['M0,0', 'L4,0']);
    expect(layer.shapes.ops).toEqual(['M1,1', 'L2,1', 'L2,2', 'Z']);
    expect(layer.overhead.ops).toEqual([]);
  });
});

describe('drawWorld', () => {
  const layer = buildStatic(geometry, () => new PathRecorder());

  it('draws the floor, then shapes and walls, then what moves, the machine on top', () => {
    const r = recorder();
    drawWorld(r.ctx, layer, scene(), cam, size, 1, palette);
    const order = [
      'fill floor R0,0,4,3',
      'fill shape M1,1',
      'stroke wall M0,0',
      'fill mover',
      'fill machine',
    ];
    const at = order.map((o) => r.log.indexOf(o));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(r.log).toContain('fill heading');
    expect(r.log).toContain('text 50 cm');
    expect(r.log).not.toContain('stroke bumped');
  });

  it('outlines the machine in red while it is bumped', () => {
    const r = recorder();
    drawWorld(r.ctx, layer, scene({ bumped: true }), cam, size, 1, palette);
    expect(r.log).toContain('stroke bumped');
  });

  it('colours each distance cone by its status and draws every scan point in view', () => {
    const r = recorder();
    const scans = [{ x: new Float32Array([2, 2.5, 99]), y: new Float32Array([1.5, 1.5, 99]) }];
    const cones = [
      {
        polygon: [
          [2, 1.5],
          [3, 1],
          [3, 2],
        ] as [number, number][],
        tone: 'near' as const,
      },
    ];
    drawWorld(r.ctx, layer, scene({ scans, cones }), cam, size, 1, palette);
    expect(r.log).toContain('fill cone-near');
    // The background, two points in view (the third is far off-screen).
    expect(r.rects()).toBe(3);
  });

  it('shows a marker where the machine stands when there is no base', () => {
    const r = recorder();
    drawWorld(r.ctx, null, scene({ footprint: null }), cam, size, 1, palette, 'imperial');
    expect(r.log).toContain('fill machine');
    expect(r.log.some((l) => l.endsWith(' in') || l.endsWith(' ft'))).toBe(true);
  });

  it('knows where the machine is on screen', () => {
    expect(machineOnScreen({ x: 2, y: 1.5, yaw: 0 }, cam, size)).toEqual({ x: 200, y: 150 });
    expect(machineOnScreen({ x: 50, y: 1.5, yaw: 0 }, cam, size)).toBeNull();
    expect(machineOnScreen(null, cam, size)).toBeNull();
  });
});
