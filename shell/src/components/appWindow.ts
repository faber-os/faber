import type { AppInfo } from '../lib/state/types';
import type { CoreState } from '../lib/state/core.svelte';
import type { WindowState } from '../lib/desktop/windows';
import type { DesktopController } from './desktopController.svelte';

/**
 * Opening an app's window (docs/apps.md §9): its UI in a sandboxed frame, with the title and size
 * from its `[window]`. One window per app.
 */
export function openAppWindow(desktop: DesktopController, app: AppInfo): string | null {
  if (!app.ui) return null;
  const w = app.window;
  return desktop.open('app', {
    key: app.id,
    title: w?.title ?? app.name,
    params: { app: app.id },
    size: w?.width && w.height ? { w: w.width, h: w.height } : undefined,
  });
}

/** The image icon of a window: an app window shows its app's icon (a URL for `<img>` only). */
export function windowIcon(
  core: CoreState,
  win: Pick<WindowState, 'kind' | 'params'>,
): string | null {
  if (win.kind !== 'app') return null;
  return core.apps?.find((a) => a.id === win.params.app)?.icon ?? null;
}

/** The tokens an app may use to look like the shell: colours, type, sizes. Nothing else. */
export const APP_THEME_TOKENS = [
  '--surface',
  '--surface-2',
  '--surface-3',
  '--surface-sunken',
  '--text',
  '--text-muted',
  '--text-faint',
  '--line',
  '--line-strong',
  '--hover',
  '--selected',
  '--accent',
  '--accent-hover',
  '--accent-ink',
  '--accent-soft',
  '--accent-text',
  '--focus-ring',
  '--ok',
  '--ok-soft',
  '--ok-text',
  '--warn',
  '--warn-soft',
  '--warn-text',
  '--danger',
  '--danger-soft',
  '--danger-text',
  '--info',
  '--info-soft',
  '--info-text',
  '--font-ui',
  '--font-data',
  '--text-xs',
  '--text-sm',
  '--text-md',
  '--text-lg',
  '--text-xl',
  '--text-2xl',
  '--space-1',
  '--space-2',
  '--space-3',
  '--space-4',
  '--space-5',
  '--space-6',
  '--radius-sm',
  '--radius-md',
  '--radius-lg',
] as const;

/** The shell's look as it is drawn now, for an app window (`theme` in the app protocol). */
export function themeForApps(
  scheme: 'light' | 'dark',
  root: Element = document.documentElement,
): { scheme: 'light' | 'dark'; tokens: Record<string, string> } {
  const style = getComputedStyle(root);
  const tokens: Record<string, string> = {};
  for (const name of APP_THEME_TOKENS) {
    const value = style.getPropertyValue(name).trim();
    if (value) tokens[name] = value;
  }
  return { scheme, tokens };
}
