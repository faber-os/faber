import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, part, simBase } from '../test/fakeShell';
import type { FakeRoute } from '../test/fakeFetch';
import type { AppInfo } from '../lib/state/types';
import AppsWindow from './AppsWindow.svelte';

const APP: AppInfo = {
  id: 'wall-follower',
  name: 'Wall follower',
  version: '0.1.0',
  publisher: 'Faber',
  enabled: true,
  state: { kind: 'running' },
  logic: true,
  tickHz: 20,
  roles: [
    {
      role: 'lidar',
      capability: 'laser-scan@1',
      command: false,
      count: '1',
      parts: ['lidar'],
      drive: 'logic',
    },
    {
      role: 'base',
      capability: 'planar-base@1',
      command: true,
      count: '1',
      parts: ['base'],
      drive: 'logic',
    },
  ],
  publishes: [
    { name: 'wall-distance', capability: 'distance@1', address: 'app:wall-follower/wall-distance' },
  ],
  ui: '/apps/wall-follower/ui/index.html',
  icon: '/apps/wall-follower/icon.svg',
  window: { title: 'Wall follower', width: 420, height: 260, pinned: false },
};

const OFF: AppInfo = {
  ...APP,
  enabled: false,
  state: { kind: 'stopped', message: 'The app is turned off.', hint: 'Turn it on to start it.' },
};

function setup(apps: AppInfo[], routes: FakeRoute[] = []) {
  const shell = fakeShell(routes);
  shell.pushParts([part('lidar'), part('lidar2'), simBase('base')]);
  shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps });
  const id = shell.desktop.open('apps');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(AppsWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

describe('Apps window', () => {
  it('lists every app with its state in words, its icon as an image and its derived signals', async () => {
    setup([OFF]);
    const row = (await screen.findByRole('heading', { name: 'Wall follower' })).closest('li')!;
    expect(row).toHaveTextContent('Stopped');
    expect(row).toHaveTextContent('The app is turned off.');
    expect(row).toHaveTextContent('version 0.1.0');
    const img = row.querySelector('img')!;
    expect(img.getAttribute('src')).toBe('/apps/wall-follower/icon.svg');
    expect(row.querySelector('svg image, svg use')).toBeNull();
    const published = within(row).getByRole('region', { name: 'What Wall follower publishes' });
    expect(published).toHaveTextContent('app:wall-follower/wall-distance');
    expect(within(published).getByText('app')).toBeInTheDocument();
  });

  it('turns an app on and off', async () => {
    const shell = setup(
      [OFF],
      [
        { method: 'POST', path: '/api/v0/apps/wall-follower/enable', body: APP },
        { method: 'POST', path: '/api/v0/apps/wall-follower/disable', body: OFF },
      ],
    );
    const on = await screen.findByRole('switch', { name: 'Turn Wall follower on' });
    expect(on).not.toBeChecked();
    await userEvent.click(on);
    // It shows what the core says, from the apps topic.
    await waitFor(() => expect(on).toBeEnabled());
    expect(on).not.toBeChecked();
    shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps: [APP] });
    await waitFor(() => expect(on).toBeChecked());
    await userEvent.click(on);
    await waitFor(() =>
      expect(shell.fetch.calls.some((c) => c.path.endsWith('/disable'))).toBe(true),
    );
    shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps: [OFF] });
    await waitFor(() => expect(on).not.toBeChecked());
    expect(shell.fetch.calls.filter((c) => c.method === 'POST').map((c) => c.path)).toEqual([
      '/api/v0/apps/wall-follower/enable',
      '/api/v0/apps/wall-follower/disable',
    ]);
  });

  it('refuses to uninstall while the app is on, and says why; once off it uninstalls', async () => {
    const shell = setup(
      [APP],
      [{ method: 'DELETE', path: '/api/v0/packages/wall-follower', status: 204 }],
    );
    const uninstall = await screen.findByRole('button', { name: 'Uninstall Wall follower' });
    expect(uninstall).toBeDisabled();
    expect(screen.getByText(/Turn it off to uninstall it/)).toBeInTheDocument();
    shell.receive({ type: 'apps', id: shell.subscriptionId('apps'), apps: [OFF] });
    await waitFor(() => expect(uninstall).toBeEnabled());
    await userEvent.click(uninstall);
    const sheet = screen.getByRole('alertdialog', { name: 'Uninstall Wall follower?' });
    await userEvent.click(within(sheet).getByRole('button', { name: 'Uninstall' }));
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'DELETE')).toBe(true));
  });

  it('changes the parts of a role and saves them', async () => {
    const shell = setup(
      [APP],
      [{ method: 'PUT', path: '/api/v0/apps/wall-follower/roles', body: APP }],
    );
    const lidar = await screen.findByRole('combobox', { name: 'lidar' });
    expect(lidar).toHaveValue('lidar');
    expect(screen.queryByRole('button', { name: 'Save parts' })).toBeNull();
    await userEvent.selectOptions(lidar, 'lidar2');
    expect(screen.getByText('It restarts with the new parts.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save parts' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.method === 'PUT')?.body).toEqual({
        roles: { lidar: 'lidar2', base: 'base' },
      }),
    );
    expect(screen.getByText(/Wall follower will be able to drive "base"/)).toBeInTheDocument();
  });

  it('opens the app’s window and its derived signal’s viewer', async () => {
    const shell = setup([APP]);
    await userEvent.click(await screen.findByRole('button', { name: 'Open Wall follower' }));
    expect(shell.desktop.state.windows.find((w) => w.kind === 'app')).toMatchObject({
      key: 'wall-follower',
      params: { app: 'wall-follower' },
    });
    await userEvent.click(screen.getByRole('button', { name: /app:wall-follower\/wall-distance/ }));
    expect(shell.desktop.state.windows.find((w) => w.kind === 'distance')).toMatchObject({
      key: 'app:wall-follower/wall-distance',
      params: { address: 'app:wall-follower/wall-distance' },
    });
  });

  it('says when there are no apps', async () => {
    setup([]);
    expect(await screen.findByText(/No apps installed yet/)).toBeInTheDocument();
  });
});
