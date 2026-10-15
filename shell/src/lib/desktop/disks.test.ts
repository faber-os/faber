import { describe, expect, it } from 'vitest';
import { diskRoots, isOnDisk, rootsChanged } from './disks';

const HOME = { id: 'home', label: 'Home', path: 'home:/' };
const SD = { id: 'disk-e', label: 'SENNEP SD (E:)', path: 'disk-e:/' };
const STICK = { id: 'disk-f', label: 'USB (F:)', path: 'disk-f:/' };
const MACHINE = { id: 'machine', label: 'This computer', path: 'machine:/' };

describe('disks', () => {
  it('lists only the disk roots, in the order the core sent them', () => {
    expect(diskRoots([HOME, SD, STICK, MACHINE])).toEqual([SD, STICK]);
    expect(diskRoots([HOME, MACHINE])).toEqual([]);
  });

  it('knows a path on a disk', () => {
    expect(isOnDisk('disk-e:/logs/run.txt')).toBe(true);
    expect(isOnDisk('home:/Desktop')).toBe(false);
    expect(isOnDisk('machine:/C:')).toBe(false);
  });

  it('asks for the roots again when a root other than home changed', () => {
    expect(rootsChanged(['disk-e:/'])).toBe(true);
    expect(rootsChanged(['home:/'])).toBe(false);
    expect(rootsChanged(['home:/Desktop', 'disk-e:/logs'])).toBe(false);
  });
});
