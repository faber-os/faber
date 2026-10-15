import type { WindowDef } from '../components/windowDefs';
import { t } from '../lib/i18n';
import AboutWindow from './AboutWindow.svelte';
import AppWindow from './AppWindow.svelte';
import AppsWindow from './AppsWindow.svelte';
import DistanceWindow from './DistanceWindow.svelte';
import DriveWindow from './DriveWindow.svelte';
import FilesWindow from './FilesWindow.svelte';
import FlasherWindow from './FlasherWindow.svelte';
import InstallPackageWindow from './InstallPackageWindow.svelte';
import LaserScanWindow from './LaserScanWindow.svelte';
import LogsWindow from './LogsWindow.svelte';
import NotesWindow from './NotesWindow.svelte';
import PartsWindow from './PartsWindow.svelte';
import PlanarBaseWindow from './PlanarBaseWindow.svelte';
import PositionWindow from './PositionWindow.svelte';
import PropertiesWindow from './PropertiesWindow.svelte';
import SerialMonitorWindow from './SerialMonitorWindow.svelte';
import SettingsWindow from './SettingsWindow.svelte';
import WorldWindow from './WorldWindow.svelte';

/** Every kind of window the shell can open. Titles and descriptions are getters, so they follow the language. */
export const WINDOW_DEFS: WindowDef[] = [
  {
    kind: 'parts',
    get title() {
      return t('programs.parts.title');
    },
    get description() {
      return t('programs.parts.description');
    },
    icon: 'parts',
    menu: 'parts',
    program: 'tool:parts',
    onDesktop: true,
    colour: 'parts',
    multiple: false,
    defaultSize: { w: 680, h: 520 },
    minSize: { w: 360, h: 300 },
    component: PartsWindow,
  },
  {
    kind: 'laser-scan',
    get title() {
      return t('programs.laserScan.title');
    },
    get description() {
      return t('programs.laserScan.description');
    },
    icon: 'scan',
    menu: null,
    colour: 'viewer',
    multiple: true,
    defaultSize: { w: 560, h: 560 },
    minSize: { w: 300, h: 300 },
    component: LaserScanWindow,
  },
  {
    kind: 'distance',
    get title() {
      return t('programs.distance.title');
    },
    get description() {
      return t('programs.distance.description');
    },
    icon: 'distance',
    menu: null,
    colour: 'viewer',
    multiple: true,
    defaultSize: { w: 440, h: 360 },
    minSize: { w: 300, h: 280 },
    component: DistanceWindow,
  },
  {
    kind: 'position',
    get title() {
      return t('programs.position.title');
    },
    get description() {
      return t('programs.position.description');
    },
    icon: 'position',
    menu: null,
    colour: 'viewer',
    multiple: true,
    defaultSize: { w: 460, h: 380 },
    minSize: { w: 300, h: 260 },
    component: PositionWindow,
  },
  {
    kind: 'planar-base',
    get title() {
      return t('programs.planarBase.title');
    },
    get description() {
      return t('programs.planarBase.description');
    },
    icon: 'base',
    menu: null,
    colour: 'viewer',
    multiple: true,
    defaultSize: { w: 480, h: 340 },
    minSize: { w: 320, h: 280 },
    component: PlanarBaseWindow,
  },
  {
    // Installed apps: on and off, their parts, their windows (docs/apps.md).
    kind: 'apps',
    get title() {
      return t('programs.apps.title');
    },
    get description() {
      return t('programs.apps.description');
    },
    icon: 'grid',
    menu: 'tools',
    program: 'tool:apps',
    onDesktop: true,
    colour: 'parts',
    multiple: false,
    defaultSize: { w: 620, h: 600 },
    minSize: { w: 340, h: 320 },
    component: AppsWindow,
  },
  {
    // One app's own window: its UI in a sandboxed frame (docs/apps.md §9). Opened from Apps;
    // its title, size and icon come from the app.
    kind: 'app',
    get title() {
      return t('programs.app.title');
    },
    get description() {
      return t('programs.app.description');
    },
    icon: 'grid',
    menu: null,
    colour: 'parts',
    multiple: true,
    defaultSize: { w: 480, h: 360 },
    minSize: { w: 240, h: 180 },
    component: AppWindow,
  },
  {
    kind: 'drive',
    get title() {
      return t('programs.drive.title');
    },
    get description() {
      return t('programs.drive.description');
    },
    icon: 'drive',
    menu: 'tools',
    program: 'tool:drive',
    onDesktop: true,
    colour: 'drive',
    multiple: false,
    defaultSize: { w: 600, h: 540 },
    minSize: { w: 340, h: 360 },
    component: DriveWindow,
  },
  {
    // The simulated world from above (docs/simulation.md, section 9).
    kind: 'world',
    get title() {
      return t('programs.world.title');
    },
    get description() {
      return t('programs.world.description');
    },
    icon: 'world',
    menu: 'tools',
    program: 'tool:world',
    colour: 'world',
    multiple: false,
    defaultSize: { w: 720, h: 560 },
    minSize: { w: 360, h: 340 },
    component: WorldWindow,
  },
  {
    kind: 'serial-monitor',
    get title() {
      return t('programs.serialMonitor.title');
    },
    get description() {
      return t('programs.serialMonitor.description');
    },
    icon: 'serial',
    menu: 'tools',
    program: 'tool:serial-monitor',
    onDesktop: true,
    colour: 'serial',
    multiple: false,
    defaultSize: { w: 680, h: 500 },
    minSize: { w: 380, h: 320 },
    component: SerialMonitorWindow,
  },
  {
    // Puts a program on an Arduino (docs/api.md, "The Arduino flasher").
    kind: 'arduino-flasher',
    get title() {
      return t('programs.flasher.title');
    },
    get description() {
      return t('programs.flasher.description');
    },
    icon: 'chip',
    menu: 'tools',
    program: 'tool:arduino-flasher',
    colour: 'flasher',
    multiple: false,
    defaultSize: { w: 560, h: 600 },
    minSize: { w: 340, h: 380 },
    component: FlasherWindow,
  },
  {
    kind: 'logs',
    get title() {
      return t('programs.logs.title');
    },
    get description() {
      return t('programs.logs.description');
    },
    icon: 'logs',
    menu: 'tools',
    program: 'tool:logs',
    colour: 'logs',
    multiple: false,
    defaultSize: { w: 760, h: 460 },
    minSize: { w: 380, h: 260 },
    component: LogsWindow,
  },
  {
    kind: 'files',
    get title() {
      return t('programs.files.title');
    },
    get description() {
      return t('programs.files.description');
    },
    icon: 'folder',
    menu: 'accessories',
    program: 'accessory:files',
    onDesktop: true,
    colour: 'files',
    multiple: true,
    defaultSize: { w: 780, h: 520 },
    minSize: { w: 360, h: 300 },
    component: FilesWindow,
  },
  {
    kind: 'notes',
    get title() {
      return t('programs.notes.title');
    },
    get description() {
      return t('programs.notes.description');
    },
    icon: 'notes',
    menu: 'accessories',
    program: 'accessory:notes',
    onDesktop: true,
    colour: 'notes',
    multiple: true,
    defaultSize: { w: 680, h: 520 },
    minSize: { w: 340, h: 260 },
    component: NotesWindow,
  },
  {
    kind: 'settings',
    get title() {
      return t('programs.settings.title');
    },
    get description() {
      return t('programs.settings.description');
    },
    icon: 'settings',
    menu: 'accessories',
    program: 'accessory:settings',
    onDesktop: true,
    colour: 'settings',
    multiple: false,
    defaultSize: { w: 620, h: 560 },
    minSize: { w: 340, h: 320 },
    component: SettingsWindow,
  },
  {
    // A connector package to install (docs/packages.md §1), opened from Files or a drop.
    kind: 'install-package',
    get title() {
      return t('programs.install.title');
    },
    get description() {
      return t('programs.install.description');
    },
    icon: 'package',
    menu: null,
    colour: 'parts',
    multiple: true,
    defaultSize: { w: 560, h: 620 },
    minSize: { w: 340, h: 360 },
    component: InstallPackageWindow,
  },
  {
    kind: 'properties',
    get title() {
      return t('programs.properties.title');
    },
    get description() {
      return t('programs.properties.description');
    },
    icon: 'info',
    menu: null,
    colour: 'about',
    multiple: true,
    defaultSize: { w: 440, h: 400 },
    minSize: { w: 320, h: 280 },
    component: PropertiesWindow,
  },
  {
    kind: 'about',
    get title() {
      return t('programs.about.title');
    },
    get description() {
      return t('programs.about.description');
    },
    icon: 'info',
    menu: 'faber',
    colour: 'about',
    multiple: false,
    defaultSize: { w: 440, h: 440 },
    minSize: { w: 320, h: 300 },
    component: AboutWindow,
  },
];

export const WINDOW_KINDS = Object.fromEntries(WINDOW_DEFS.map((d) => [d.kind, d]));
