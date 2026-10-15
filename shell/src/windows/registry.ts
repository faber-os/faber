import type { WindowDef } from '../components/windowDefs';
import AboutWindow from './AboutWindow.svelte';
import LaserScanWindow from './LaserScanWindow.svelte';
import LogsWindow from './LogsWindow.svelte';
import PartsWindow from './PartsWindow.svelte';
import SerialMonitorWindow from './SerialMonitorWindow.svelte';

/** Every kind of window the shell can open. */
export const WINDOW_DEFS: WindowDef[] = [
  {
    kind: 'parts',
    title: 'Parts',
    description: 'Add, watch and remove the hardware on this machine',
    icon: 'parts',
    menu: 'parts',
    multiple: false,
    defaultSize: { w: 680, h: 520 },
    minSize: { w: 360, h: 300 },
    component: PartsWindow,
  },
  {
    kind: 'laser-scan',
    title: 'Scan viewer',
    description: 'Live top view of a laser-scan signal',
    icon: 'scan',
    menu: null,
    multiple: true,
    defaultSize: { w: 560, h: 560 },
    minSize: { w: 300, h: 300 },
    component: LaserScanWindow,
  },
  {
    kind: 'serial-monitor',
    title: 'Serial monitor',
    description: 'Read and write a serial port directly',
    icon: 'serial',
    menu: 'tools',
    multiple: false,
    defaultSize: { w: 680, h: 480 },
    minSize: { w: 380, h: 300 },
    component: SerialMonitorWindow,
  },
  {
    kind: 'logs',
    title: 'Logs',
    description: 'Everything the core reports, live',
    icon: 'logs',
    menu: 'tools',
    multiple: false,
    defaultSize: { w: 760, h: 460 },
    minSize: { w: 380, h: 260 },
    component: LogsWindow,
  },
  {
    kind: 'about',
    title: 'About Faber',
    description: 'Versions and connection details',
    icon: 'info',
    menu: 'faber',
    multiple: false,
    defaultSize: { w: 440, h: 420 },
    minSize: { w: 320, h: 300 },
    component: AboutWindow,
  },
];

export const WINDOW_KINDS = Object.fromEntries(WINDOW_DEFS.map((d) => [d.kind, d]));
