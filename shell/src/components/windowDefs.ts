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
  /** Its desktop icon key (`tool:<id>` or `accessory:<id>`, docs/api.md), if it has one. */
  pin?: string;
  /** One line for the start menu. */
  description: string;
}
