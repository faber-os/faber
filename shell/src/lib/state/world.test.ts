import { describe, expect, it } from 'vitest';
import { simBase, simDistance, part } from '../../test/fakeShell';
import { worldBase, worldDistances, worldParts, worldScanners } from './world';

const mount = { x: 0.3, y: 0, z: 0.2, roll: 0, pitch: 0, yaw: 0 };

describe('world parts', () => {
  const parts = [
    simBase('base', { simulate: 'world' }),
    simBase('spare'),
    simDistance('front', { simulate: 'world', mount }),
    simDistance('hand'),
    part('lidar', { port: undefined, simulate: 'world', mount }),
    part('real'),
  ];

  it('are the parts simulated in the world', () => {
    expect(worldParts(parts).map((p) => p.name)).toEqual(['base', 'front', 'lidar']);
    expect(worldParts(null)).toEqual([]);
  });

  it('have one base: the machine', () => {
    expect(worldBase(parts)?.name).toBe('base');
    expect(worldBase([simBase('b', { simulate: 'world', state: { kind: 'invalid' } })])).toBeNull();
    expect(worldBase([simBase('manual')])).toBeNull();
  });

  it('list the sensors to draw, with their mounts and their own limits', () => {
    expect(worldScanners(parts)).toEqual([
      { part: 'lidar', address: 'lidar/laser-scan', mount, info: null },
    ]);
    const [front] = worldDistances(parts);
    expect(front).toMatchObject({ part: 'front', address: 'front/distance', mount });
    expect(front!.info.maxM).toBe(4);
  });
});
