import { encode } from '@msgpack/msgpack';
import { decode } from '@msgpack/msgpack';
import { createHttp } from '../lib/api/http';
import { CoreSocket } from '../lib/api/socket';
import { CoreState } from '../lib/state/core.svelte';
import { CORE_CONTEXT_KEY } from '../lib/state/context';
import type { ConnectorInfo, PartInfo, PortInfo, Settings } from '../lib/state/types';
import { MENU_CONTEXT_KEY, MenuController } from '../components/menuController.svelte';
import { DESKTOP_CONTEXT_KEY, DesktopController } from '../components/desktopController.svelte';
import { THEME_CONTEXT_KEY, ThemeController } from '../components/theme.svelte';
import { WINDOW_KINDS } from '../windows/registry';
import { fakeFetch, type FakeRoute } from './fakeFetch';
import { FakeWebSocket } from './fakeWebSocket';

export const LD19: ConnectorInfo = {
  id: 'ldrobot-ld19',
  name: 'LD19 LiDAR',
  version: '0.1.0',
  kind: 'built-in',
  port: { kind: 'serial', baud: 230400 },
  provides: [{ name: 'laser-scan', capability: 'laser-scan@1' }],
  settings: [],
};

const RATE = {
  key: 'rate-hz',
  label: 'Rate',
  description: 'How often a simulated part sends a new value.',
  unit: 'Hz',
  advanced: true,
  kind: 'number',
  min: 0,
  minExclusive: true,
} as const;

/** A number setting above 0, as the core describes one. */
function positive(key: string, label: string, unit: string, def: number, suggested?: number) {
  return {
    key,
    label,
    description: `About ${label}.`,
    unit,
    advanced: false,
    kind: 'number' as const,
    default: def,
    min: 0,
    minExclusive: true,
    ...(suggested === undefined ? {} : { suggested }),
  };
}

export const GENERIC_BASE: ConnectorInfo = {
  id: 'generic-planar-base',
  name: 'Generic drive base',
  version: '0.1.0',
  kind: 'built-in',
  port: { kind: 'bridge' },
  provides: [
    {
      name: 'planar-base',
      capability: 'planar-base@1',
      info: {
        capability: 'planar-base@1',
        info: {
          holonomic: true,
          maxVelocity: { vx: 0.5, vy: 0.5, omega: 1.5 },
          maxAcceleration: { vx: 1, vy: 1, omega: 1.5 },
          builtFrom: [],
        },
      },
    },
  ],
  // As the core describes it (core/src/connectors/generic.rs).
  settings: [
    positive('length', 'Length', 'm', 0.5, 0.8),
    positive('width', 'Width', 'm', 0.4, 0.65),
    {
      key: 'holonomic',
      label: 'Can drive sideways (mecanum or omni wheels)',
      description: 'Off for a tank.',
      advanced: false,
      kind: 'bool',
      default: true,
    },
    positive('max-vx', 'Top speed forward', 'm/s', 0.5, 0.6),
    positive('max-vy', 'Top speed sideways', 'm/s', 0.5, 0.6),
    positive('max-omega', 'Top turning speed', 'rad/s', 1.5),
    positive('max-accel', 'Acceleration', 'm/s²', 1),
    { ...RATE, default: 20 },
    {
      key: 'built-from',
      label: 'Built from motors',
      description: 'The motor parts it drives with.',
      advanced: true,
      kind: 'part-list',
    },
  ],
};

export const GENERIC_DISTANCE: ConnectorInfo = {
  id: 'generic-distance',
  name: 'Generic distance sensor',
  version: '0.1.0',
  kind: 'built-in',
  port: { kind: 'bridge' },
  provides: [
    {
      name: 'distance',
      capability: 'distance@1',
      info: {
        capability: 'distance@1',
        info: { minM: 0.02, maxM: 4, fieldOfViewRad: 0.26, kind: 'ultrasonic' },
      },
    },
  ],
  settings: [
    {
      key: 'min-m',
      label: 'Closest it can measure',
      description: 'Anything nearer reads as too close.',
      unit: 'm',
      advanced: false,
      kind: 'number',
      default: 0.02,
      min: 0,
    },
    positive('max-m', 'Farthest it can measure', 'm', 4),
    positive('fov-rad', 'Field of view', 'rad', 0.26),
    {
      key: 'kind',
      label: 'Kind of sensor',
      description: 'How it measures.',
      advanced: false,
      kind: 'choice',
      default: 'ultrasonic',
      choices: [
        { value: 'ultrasonic', label: 'Ultrasonic' },
        { value: 'laser-tof', label: 'Laser time-of-flight' },
        { value: 'infrared', label: 'Infrared' },
        { value: 'radar', label: 'Radar' },
      ],
    },
    { ...RATE, default: 10 },
  ],
};

/** A simulated drive base, as the parts topic shows it. */
export function simBase(name = 'base', overrides: Partial<PartInfo> = {}): PartInfo {
  return part(name, {
    connector: 'generic-planar-base',
    port: undefined,
    simulate: 'manual',
    signals: [
      {
        address: `${name}/planar-base`,
        capability: 'planar-base@1',
        info: GENERIC_BASE.provides[0]!.info!,
      },
    ],
    ...overrides,
  });
}

/** A manual simulated distance sensor. */
export function simDistance(name = 'front', overrides: Partial<PartInfo> = {}): PartInfo {
  return part(name, {
    connector: 'generic-distance',
    port: undefined,
    simulate: 'manual',
    signals: [
      {
        address: `${name}/distance`,
        capability: 'distance@1',
        info: GENERIC_DISTANCE.provides[0]!.info!,
      },
    ],
    ...overrides,
  });
}

export const SAMPLE_PORT: PortInfo = {
  id: 'replay:ld19-sample',
  kind: 'replay',
  label: 'Recording ld19-sample',
  suggestions: [{ connector: 'ldrobot-ld19', reason: 'This is a recording of a LD19 LiDAR.' }],
};

export const USB_PORT: PortInfo = {
  id: 'usb:10c4:ea60',
  kind: 'usb-serial',
  label: 'CP2102 USB to UART Bridge (COM3)',
  usb: { vid: 0x10c4, pid: 0xea60 },
  claimedBy: 'lidar',
  suggestions: [],
};

export function part(name: string, overrides: Partial<PartInfo> = {}): PartInfo {
  const p: PartInfo = {
    name,
    connector: 'ldrobot-ld19',
    port: 'replay:ld19-sample',
    state: { kind: 'running' },
    signals: [{ address: `${name}/laser-scan`, capability: 'laser-scan@1' }],
    mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
    ...overrides,
  };
  // A simulated part has no port (absent, never undefined: that is how the wire has it).
  if (p.port === undefined) delete p.port;
  return p;
}

export const DEFAULT_SETTINGS: Settings = {
  machine: { name: 'faber' },
  appearance: { theme: 'system', style: 'aluminium', wallpaper: 'aluminium', language: 'en' },
  accessibility: { reduceMotion: 'system', higherContrast: 'system', textSize: '100' },
  units: { length: 'metric' },
  time: { timeZone: '', clock: '24h' },
  desktop: { icons: [] },
  taskbar: {
    pinned: ['tool:parts', 'accessory:files', 'accessory:notes', 'tool:serial-monitor'],
    size: 'small',
  },
  developer: { fullMachineFiles: false },
};

/** A folder listing answer for the fake fetch. */
export function listing(path: string, entries: Partial<import('../lib/state/types').FileEntry>[]) {
  return {
    path: `/api/v0/files/list?path=${encodeURIComponent(path)}`,
    body: {
      path,
      entries: entries.map((e) => ({
        kind: 'file',
        size: 0,
        path: `${path}/${e.name}`,
        ...e,
      })),
    },
  };
}

function bin(msg: unknown): ArrayBuffer {
  const b = encode(msg);
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
}

/**
 * A whole shell wired to fakes: real CoreState, real CoreSocket over a FakeWebSocket, real HTTP
 * client over a fake fetch. Components rendered with `context` behave as in the browser.
 */
export function fakeShell(routes: FakeRoute[] = [], viewport = { width: 1600, height: 1000 }) {
  FakeWebSocket.reset();
  const fetch = fakeFetch([
    {
      path: '/api/v0/system',
      body: { name: 'Faber', version: '0.1.0', timeUs: 1_000_000, timeZone: 'Europe/Copenhagen' },
    },
    { path: '/api/v0/connectors', body: [LD19] },
    { path: '/api/v0/connectors/problems', body: [] },
    { path: '/api/v0/packages', body: [] },
    {
      path: '/api/v0/about',
      body: {
        version: '0.1.0',
        api: 'test',
        dataFolder: '/data',
        uptimeS: 1,
        os: 'test',
        timeZone: 'Europe/Copenhagen',
      },
    },
    {
      path: '/api/v0/storage',
      body: {
        recordingsBytes: 0,
        documentsBytes: 0,
        desktopBytes: 0,
        backupsBytes: 0,
        freeBytes: 1,
        totalBytes: 2,
      },
    },
    listing('home:/Backups', []),
    { path: '/api/v0/translations', body: [] },
    ...routes,
  ]);
  const socket = new CoreSocket({
    url: 'ws://test/api/v0/ws?format=msgpack',
    format: 'msgpack',
    WebSocketImpl: FakeWebSocket,
  });
  const core = new CoreState({
    http: createHttp(fetch.fetch),
    socket,
    format: 'msgpack',
    schedule: (fn) => fn(),
  });
  core.start();
  const ws = FakeWebSocket.latest();
  ws.open();
  const desktop = new DesktopController(WINDOW_KINDS, null, viewport);
  const theme = new ThemeController(null, null);
  const menu = new MenuController();
  const context = new Map<unknown, unknown>([
    [CORE_CONTEXT_KEY, core],
    [DESKTOP_CONTEXT_KEY, desktop],
    [THEME_CONTEXT_KEY, theme],
    [MENU_CONTEXT_KEY, menu],
  ]);

  type Sent = { type: string; id: number; topic?: { kind: string; address?: string } };
  const sent = () => ws.sent.map((d) => decode(d as Uint8Array) as Sent);

  function subscriptionId(kind: string, address?: string): number {
    const m = sent()
      .filter(
        (s) =>
          s.type === 'subscribe' &&
          s.topic?.kind === kind &&
          (!address || s.topic.address === address),
      )
      .at(-1);
    if (!m) throw new Error(`No subscription to ${kind} ${address ?? ''}`);
    return m.id;
  }

  return {
    core,
    ws,
    fetch,
    desktop,
    theme,
    menu,
    context,
    sent,
    subscriptionId,
    receive: (msg: unknown) => ws.receive(bin(msg)),
    pushParts: (parts: PartInfo[]) =>
      ws.receive(bin({ type: 'parts', id: subscriptionId('parts'), parts })),
    pushPorts: (ports: PortInfo[]) =>
      ws.receive(bin({ type: 'ports', id: subscriptionId('ports'), ports })),
    pushSettings: (settings: Settings = DEFAULT_SETTINGS) =>
      ws.receive(bin({ type: 'settings', id: subscriptionId('settings'), settings })),
    pushLeases: (
      leases: { part: string; holder: string; deadlineUs?: number; locks?: string[] }[],
    ) =>
      ws.receive(
        bin({
          type: 'leases',
          id: subscriptionId('leases'),
          leases: leases.map((l) => ({ deadlineUs: 1, locks: [], ...l })),
        }),
      ),
    pushEstop: (engaged: boolean, reason = 'Stop button') =>
      ws.receive(
        bin({
          type: 'estop',
          id: subscriptionId('estop'),
          status: engaged
            ? { engaged, engagedBy: { tUs: 1, engaged, source: 'shell', reason } }
            : { engaged },
        }),
      ),
    /** A reading of `address` to every subscription to it. */
    pushReading: (address: string, reading: unknown) =>
      ws.receive(bin({ type: 'signal', id: subscriptionId('signal', address), address, reading })),
    filesChanged: (folders: string[]) =>
      ws.receive(bin({ type: 'files-changed', id: subscriptionId('files'), folders })),
  };
}
