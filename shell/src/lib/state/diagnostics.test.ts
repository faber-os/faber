import { describe, expect, it } from 'vitest';
import { diagnosticsText, logsText, redact } from './diagnostics';
import type { AboutInfo, LogEntry, PartInfo, PortInfo } from './types';

const ABOUT: AboutInfo = {
  version: '0.1.0',
  api: '5499d40f4fdcb9d4',
  dataFolder: 'C:\\Users\\mikke\\faber\\data',
  uptimeS: 3725,
  os: 'windows (x86_64)',
  timeZone: 'Europe/Copenhagen',
};

const PORTS: PortInfo[] = [
  {
    id: 'usb:10c4:ea60',
    kind: 'usb-serial',
    label: 'CP2102 USB to UART Bridge (COM3)',
    claimedBy: 'lidar',
    suggestions: [],
  },
  { id: 'serial:COM7', kind: 'bluetooth', label: "Mikkel's phone (COM7)", suggestions: [] },
];

const PARTS: PartInfo[] = [
  {
    name: 'lidar',
    connector: 'ldrobot-ld19',
    port: 'usb:10c4:ea60',
    state: { kind: 'failed', message: 'No data for 2 s.', hint: 'Check the cable.' },
    signals: [],
    mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
  },
];

function log(i: number, message = `line ${i}`): LogEntry {
  return { tUs: i, level: 'info', source: 'core::ports', message };
}

const time = (e: LogEntry) => `t${e.tUs}`;

describe('diagnostics', () => {
  it('says which Faber, which computer, and what the parts and ports are doing', () => {
    const text = diagnosticsText({
      about: ABOUT,
      ports: PORTS,
      parts: PARTS,
      logs: [],
      logTime: time,
    });
    expect(text).toContain('Faber 0.1.0 (API 5499d40f4fdcb9d4)');
    expect(text).toContain('OS: windows (x86_64)');
    expect(text).toContain('Time zone: Europe/Copenhagen');
    expect(text).toContain('Uptime: 1 h 2 min');
    expect(text).toContain(
      'usb:10c4:ea60 (usb-serial) CP2102 USB to UART Bridge (COM3), used by lidar',
    );
    expect(text).toContain('lidar: ldrobot-ld19 on usb:10c4:ea60, failed: No data for 2 s.');
  });

  it('holds no personal data: no data folder, no user name, no Bluetooth device names', () => {
    const logs = [
      log(1, 'saved C:\\Users\\mikke\\faber\\data\\config\\parts.toml'),
      log(2, 'read /home/pi/x'),
    ];
    const text = diagnosticsText({ about: ABOUT, ports: PORTS, parts: PARTS, logs, logTime: time });
    expect(text).not.toContain('mikke');
    expect(text).not.toContain('/home/pi');
    expect(text).not.toContain("Mikkel's phone");
    expect(text).toContain('serial:COM7 (bluetooth) Bluetooth device');
    expect(text).toContain('saved <data folder>\\config\\parts.toml');
    expect(text).toContain('read <home>/x');
  });

  it('keeps only the last 200 log lines', () => {
    const logs = Array.from({ length: 250 }, (_, i) => log(i));
    const text = diagnosticsText({ about: ABOUT, ports: [], parts: [], logs, logTime: time });
    expect(text).toContain('t249 INFO core::ports line 249');
    expect(text).toContain('t50 INFO');
    expect(text).not.toContain('t49 INFO');
  });

  it('works before the core has said anything about itself', () => {
    const text = diagnosticsText({ about: null, ports: [], parts: [], logs: [], logTime: time });
    expect(text).toContain('Faber (version unknown)');
  });

  it('writes the whole log as text, one line per entry', () => {
    expect(logsText([log(1), log(2)], time, null)).toBe(
      't1 INFO core::ports line 1\nt2 INFO core::ports line 2\n',
    );
    expect(redact('C:\\Users\\anna\\x', null)).toBe('<home>\\x');
    expect(redact('/Users/anna/x', null)).toBe('<home>/x');
  });
});
