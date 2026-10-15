import { describe, expect, it } from 'vitest';
import { RingBuffer } from './ringBuffer';
import { validatePartName } from './partName';
import { portHints, portAvailability } from './portHints';
import { filterLogs, LOG_LEVELS } from './logFilter';
import type { ConnectorInfo, LogEntry, PortInfo } from './types';

describe('RingBuffer', () => {
  it('keeps the newest items up to its capacity, oldest first', () => {
    const r = new RingBuffer<number>(3);
    for (const n of [1, 2, 3, 4, 5]) r.push(n);
    expect(r.toArray()).toEqual([3, 4, 5]);
    expect(r.size).toBe(3);
  });

  it('works below capacity and can be cleared', () => {
    const r = new RingBuffer<string>(1000);
    r.push('a');
    r.push('b');
    expect(r.toArray()).toEqual(['a', 'b']);
    r.clear();
    expect(r.toArray()).toEqual([]);
  });

  it('rejects a capacity below one', () => {
    expect(() => new RingBuffer(0)).toThrow();
  });
});

describe('validatePartName (mirrors the core rule)', () => {
  it('accepts lowercase letters, digits and _, starting with a letter', () => {
    for (const ok of ['lidar', 'front_lidar', 'a1', 'x', 'a'.repeat(32)]) {
      expect(validatePartName(ok)).toBeNull();
    }
  });

  it('explains what is wrong and how to fix it', () => {
    for (const bad of ['', 'Lidar', '1lidar', 'front-lidar', 'front lidar', '_x', 'a'.repeat(33)]) {
      const e = validatePartName(bad);
      expect(e, bad).not.toBeNull();
      expect(e!.hint).toContain('lowercase');
    }
    expect(validatePartName('')!.message).toBe('The part needs a name.');
    expect(validatePartName('a'.repeat(33))!.message).toContain('longer than 32');
  });
});

const ld19: ConnectorInfo = {
  id: 'ldrobot-ld19',
  name: 'LD19 LiDAR',
  version: '0.1.0',
  kind: 'built-in',
  port: { kind: 'serial', baud: 230400 },
  provides: [{ name: 'laser-scan', capability: 'laser-scan@1' }],
};

const replay: PortInfo = {
  id: 'replay:demo-ld19',
  kind: 'replay',
  label: 'Demo recording (LD19)',
  suggestions: [{ connector: 'ldrobot-ld19', reason: 'This is a recording of a LD19 LiDAR.' }],
};

describe('portHints', () => {
  it('names the suggested connector in friendly words', () => {
    expect(portHints(replay, [ld19])).toEqual([
      {
        connector: 'ldrobot-ld19',
        text: 'Often used for: LD19 LiDAR',
        reason: 'This is a recording of a LD19 LiDAR.',
      },
    ]);
  });

  it('falls back to the connector id when the connector is unknown', () => {
    const p = { ...replay, suggestions: [{ connector: 'mystery', reason: 'r' }] };
    expect(portHints(p, [])[0]!.text).toBe('Often used for: mystery');
  });
});

describe('portAvailability', () => {
  it('says who is using a claimed port', () => {
    expect(portAvailability({ ...replay, claimedBy: 'lidar' })).toEqual({
      free: false,
      text: 'Used by the part "lidar"',
    });
    expect(portAvailability({ ...replay, claimedBy: 'tool:serial-monitor' })).toEqual({
      free: false,
      text: 'Used by the serial monitor',
    });
    expect(portAvailability(replay)).toEqual({ free: true, text: 'Free' });
    const nil = { ...replay, claimedBy: null } as unknown as PortInfo;
    expect(portAvailability(nil)).toEqual({ free: true, text: 'Free' });
  });
});

describe('filterLogs', () => {
  const entries: LogEntry[] = [
    { tUs: 1, level: 'debug', source: 'core::ports', message: 'scan' },
    { tUs: 2, level: 'info', source: 'part:lidar', message: 'started' },
    { tUs: 3, level: 'warn', source: 'part:lidar', message: 'checksum mismatch' },
    { tUs: 4, level: 'error', source: 'core::config', message: 'bad parts.toml' },
  ];

  it('lists the levels from most to least severe', () => {
    expect(LOG_LEVELS).toEqual(['error', 'warn', 'info', 'debug']);
  });

  it('shows the chosen level and everything more severe', () => {
    expect(filterLogs(entries, 'warn', '').map((e) => e.tUs)).toEqual([3, 4]);
    expect(filterLogs(entries, 'debug', '')).toHaveLength(4);
  });

  it('matches text in the source or the message, ignoring case', () => {
    expect(filterLogs(entries, 'debug', 'LIDAR').map((e) => e.tUs)).toEqual([2, 3]);
    expect(filterLogs(entries, 'debug', 'toml').map((e) => e.tUs)).toEqual([4]);
  });
});
