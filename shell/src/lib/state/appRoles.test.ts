import { describe, expect, it } from 'vitest';
import { part, simBase, simDistance } from '../../test/fakeShell';
import {
  allFilled,
  candidates,
  initialPicks,
  picksChanged,
  rolesRequest,
  takesMany,
} from './appRoles';
import type { AppRole } from './types';

const lidar: AppRole = {
  role: 'lidar',
  capability: 'laser-scan@1',
  command: false,
  count: '1',
  parts: [],
};
const base: AppRole = {
  role: 'base',
  capability: 'planar-base@1',
  command: true,
  count: '1',
  parts: [],
};
const ring: AppRole = {
  role: 'ring',
  capability: 'distance@1',
  command: false,
  count: '1..',
  parts: [],
};
const parts = [part('lidar'), simBase('base'), simDistance('front'), simDistance('back')];

describe('app roles', () => {
  it('offers only the parts that provide the role’s capability', () => {
    expect(candidates(lidar, parts)).toEqual(['lidar']);
    expect(candidates(ring, parts)).toEqual(['back', 'front']);
    expect(candidates(base, [part('lidar')])).toEqual([]);
    expect(takesMany(ring)).toBe(true);
    expect(takesMany(lidar)).toBe(false);
  });

  it('starts from the parts picked already, else suggests the only part that fits', () => {
    expect(initialPicks([lidar, base, ring], parts)).toEqual({
      lidar: ['lidar'],
      base: ['base'],
      ring: [],
    });
    const bound = { ...ring, parts: ['front', 'gone'] };
    expect(initialPicks([bound], parts)).toEqual({ ring: ['front'] });
  });

  it('knows when every role is filled', () => {
    expect(allFilled([lidar, ring], { lidar: ['lidar'], ring: ['front', 'back'] })).toBe(true);
    expect(allFilled([lidar, ring], { lidar: ['lidar'], ring: [] })).toBe(false);
    expect(allFilled([lidar], { lidar: ['a', 'b'] })).toBe(false);
    expect(allFilled([], {})).toBe(true);
  });

  it('sends one part for a one-part role and a list for a 1.. role, leaving empty roles out', () => {
    const picks = { lidar: ['lidar'], base: [], ring: ['front'] };
    expect(rolesRequest([lidar, base, ring], picks)).toEqual({ lidar: 'lidar', ring: ['front'] });
  });

  it('knows when the picks differ from what the app has', () => {
    const bound = { ...lidar, parts: ['lidar'] };
    expect(picksChanged([bound], { lidar: ['lidar'] })).toBe(false);
    expect(picksChanged([bound], { lidar: [] })).toBe(true);
    expect(picksChanged([ring], { ring: ['front'] })).toBe(true);
  });
});
