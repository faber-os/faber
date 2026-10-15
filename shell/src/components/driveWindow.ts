import { t } from '../lib/i18n';
import type { AppSignal, PartInfo, SignalInfo } from '../lib/state/types';
import type { DesktopController } from './desktopController.svelte';

/** Opens the Drive window (there is one) on `part`, or as it was. */
export function openDrive(desktop: DesktopController, part?: string): string {
  const id = desktop.open('drive');
  if (part) desktop.update(id, { params: { part } });
  return id;
}

/** The viewer window for each capability that has one. */
const VIEWERS: Partial<
  Record<SignalInfo['capability'], { kind: string; title: (name: string) => string }>
> = {
  'laser-scan@1': { kind: 'laser-scan', title: (name) => t('parts.title.viewer', { name }) },
  'distance@1': { kind: 'distance', title: (name) => t('parts.title.distance', { name }) },
  'planar-base@1': { kind: 'planar-base', title: (name) => t('parts.title.base', { name }) },
  'position@1': { kind: 'position', title: (name) => t('parts.title.position', { name }) },
};

/** The signals of a part that Faber can draw. */
export function viewable(p: PartInfo): SignalInfo[] {
  return p.signals.filter((s) => VIEWERS[s.capability]);
}

/** Opens (or brings forward) the viewer of one signal. */
export function openViewer(desktop: DesktopController, p: PartInfo, s: SignalInfo): void {
  const v = VIEWERS[s.capability];
  if (!v) return;
  desktop.open(v.kind, {
    key: s.address,
    title: v.title(p.name),
    params: { address: s.address, part: p.name },
  });
}

/** Whether Faber can draw a signal of this capability (an app's derived signals too). */
export function hasViewer(capability: string): boolean {
  return capability in VIEWERS;
}

/** Opens the viewer of an app's derived signal (D24), titled after the app. */
export function openDerivedViewer(desktop: DesktopController, app: string, s: AppSignal): void {
  const v = VIEWERS[s.capability];
  if (!v) return;
  desktop.open(v.kind, {
    key: s.address,
    title: v.title(`${app} · ${s.name}`),
    params: { address: s.address, part: s.address.split('/')[0] ?? '' },
  });
}
