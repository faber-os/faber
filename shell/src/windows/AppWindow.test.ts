import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, part, simBase } from '../test/fakeShell';
import type { AppInfo } from '../lib/state/types';
import AppWindow from './AppWindow.svelte';

const APP: AppInfo = {
  id: 'wall-follower',
  name: 'Wall follower',
  version: '0.1.0',
  enabled: true,
  state: { kind: 'running' },
  logic: true,
  roles: [
    { role: 'lidar', capability: 'laser-scan@1', command: false, count: '1', parts: ['lidar'] },
    { role: 'base', capability: 'planar-base@1', command: true, count: '1', parts: ['base'] },
  ],
  publishes: [],
  ui: '/apps/wall-follower/ui/index.html',
  icon: '/apps/wall-follower/icon.svg',
  window: { title: 'Wall follower', pinned: false },
};

const GRANT = {
  lease: 'L1',
  part: 'base',
  holder: 'app:wall-follower',
  deadlineUs: 5,
  durationMs: 250,
  locks: [],
};

function setup(app: AppInfo = APP) {
  const shell = fakeShell([
    { method: 'POST', path: '/api/v0/parts/base/lease', status: 201, body: GRANT },
    { method: 'DELETE', path: '/api/v0/parts/base/lease?lease=L1', status: 204 },
  ]);
  shell.pushParts([part('lidar'), simBase('base')]);
  shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps: [app] });
  const id = shell.desktop.open('app', { key: app.id, params: { app: app.id } });
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  const view = render(AppWindow, { props: { win, visible: true }, context: shell.context });
  const frame = view.container.querySelector('iframe')!;
  const posted = vi.spyOn(frame.contentWindow!, 'postMessage');
  /** A message as if the app's page posted it. */
  const fromApp = (data: unknown, source: unknown = frame.contentWindow, origin = 'null') =>
    window.dispatchEvent(new MessageEvent('message', { data, origin, source: source as Window }));
  const replies = () => posted.mock.calls.map((c) => c[0] as Record<string, unknown>);
  return { shell, view, frame, fromApp, replies };
}

describe('App window', () => {
  it('shows the app’s page in a sandboxed frame with scripts only, never same-origin', () => {
    const { frame } = setup();
    expect(frame.getAttribute('sandbox')).toBe('allow-scripts');
    expect(frame.getAttribute('src')).toBe('/apps/wall-follower/ui/index.html');
    expect(frame.getAttribute('referrerpolicy')).toBe('no-referrer');
    expect(frame.title).toBe('Wall follower');
  });

  it('answers its own frame only', async () => {
    const { fromApp, replies } = setup();
    fromApp({ faber: 1, id: 1, type: 'roles' }, window);
    fromApp({ faber: 1, id: 2, type: 'roles' }, undefined, 'http://localhost:3000');
    fromApp({ faber: 1, id: 3, type: 'roles' });
    await waitFor(() => expect(replies()).toHaveLength(1));
    expect(replies()[0]).toMatchObject({ faber: 1, re: 3, ok: true });
  });

  it('asks before the app drives (D46), shows who drives with a Stop the app can’t hide', async () => {
    const { shell, fromApp, replies } = setup();
    fromApp({ faber: 1, id: 1, type: 'drive', role: 'base' });
    const sheet = await screen.findByRole('alertdialog', { name: 'Start driving faber?' });
    expect(sheet).toHaveTextContent('Wall follower wants to drive “base”.');
    expect(shell.fetch.calls.some((c) => c.path.endsWith('/lease'))).toBe(false);
    await userEvent.click(within(sheet).getByRole('button', { name: 'Start' }));
    await waitFor(() =>
      expect(replies()).toContainEqual({ faber: 1, re: 1, ok: true, value: null }),
    );
    const lease = shell.fetch.calls.find((c) => c.path === '/api/v0/parts/base/lease');
    expect(lease?.body).toEqual({ holder: 'app:wall-follower' });
    expect(shell.core.drivingConfirmed).toBe(true);
    const bar = await screen.findByRole('status');
    expect(bar).toHaveTextContent('Wall follower is driving base.');
    await userEvent.click(within(bar).getByRole('button', { name: 'Stop' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'DELETE' && c.path.includes('/lease')),
      ).toBe(true),
    );
    expect(replies().some((m) => m.event === 'lease-ended')).toBe(true);
  });

  it('a "no" drives nothing', async () => {
    const { shell, fromApp, replies } = setup();
    fromApp({ faber: 1, id: 1, type: 'drive', role: 'base' });
    const sheet = await screen.findByRole('alertdialog', { name: 'Start driving faber?' });
    await userEvent.click(within(sheet).getByRole('button', { name: 'Cancel' }));
    await waitFor(() =>
      expect(replies()[0]).toMatchObject({ re: 1, ok: false, error: { code: 'not-confirmed' } }),
    );
    expect(shell.fetch.calls.some((c) => c.path.includes('/lease'))).toBe(false);
  });

  it('closing the window ends its leases', async () => {
    const { shell, view, fromApp, replies } = setup();
    shell.core.confirmDriving();
    fromApp({ faber: 1, id: 1, type: 'drive', role: 'base' });
    await waitFor(() =>
      expect(replies()).toContainEqual({ faber: 1, re: 1, ok: true, value: null }),
    );
    view.unmount();
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'DELETE' && c.path.includes('/lease')),
      ).toBe(true),
    );
  });

  it('says so when the app has gone', async () => {
    const { shell, view } = setup();
    shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps: [] });
    await waitFor(() => expect(view.container.querySelector('iframe')).toBeNull());
    expect(screen.getByText('This app isn’t installed any more.')).toBeInTheDocument();
  });
});
