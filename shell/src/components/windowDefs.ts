import type { Component } from 'svelte';
import type { WindowKind, WindowState } from '../lib/desktop/windows';
import type { IconName } from './Icon.svelte';

export interface WindowProps {
  win: WindowState;
  /** False while minimised: windows stay mounted (keeping their state) but stop drawing. */
  visible: boolean;
}

export interface WindowDef extends WindowKind {
  icon: IconName;
  component: Component<WindowProps>;
  /** Where the start menu lists it; null for windows opened from elsewhere (viewers). */
  menu: 'parts' | 'tools' | 'accessories' | 'faber' | null;
  /**
   * Its program key (`tool:<id>` or `accessory:<id>`, docs/api.md), for programs that can be
   * pinned to the taskbar and have a desktop icon. Viewers and dialogs have none.
   */
  program?: string;
  /** Shown as an icon on the desktop. */
  onDesktop?: boolean;
  /** Its icon colour: `--app-<colour>` (src/lib/desktop/styles.ts, APP_COLOURS). */
  colour: string;
  /** One line for the start menu. */
  description: string;
}
