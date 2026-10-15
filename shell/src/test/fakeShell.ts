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
};

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
  return {
    name,
    connector: 'ldrobot-ld19',
    port: 'replay:ld19-sample',
    state: { kind: 'running' },
    signals: [{ address: `${name}/laser-scan`, capability: 'laser-scan@1' }],
    mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
    ...overrides,
  };
}

export const DEFAULT_SETTINGS: Settings = {
  appearance: { theme: 'system', style: 'aluminium', wallpaper: 'aluminium' },
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
    { path: '/api/v0/system', body: { name: 'Faber', version: '0.1.0', timeUs: 1_000_000 } },
    { path: '/api/v0/connectors', body: [LD19] },
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
    filesChanged: (folders: string[]) =>
      ws.receive(bin({ type: 'files-changed', id: subscriptionId('files'), folders })),
  };
}
