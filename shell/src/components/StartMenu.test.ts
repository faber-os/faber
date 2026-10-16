import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, WALL_FOLLOWER, fakeShell, part } from '../test/fakeShell';
import type { Settings } from '../lib/state/types';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';

function setup() {
  const shell = fakeShell([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
  shell.pushSettings();
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
}

/** Opens the start menu again, after something closed it. */
async function openStartAgain() {
  if (!screen.queryByRole('menu', { name: 'Start' }))
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
  return screen.getByRole('menu', { name: 'Start' });
}

async function openStart() {
  await userEvent.click(screen.getByRole('button', { name: 'Start' }));
  return screen.getByRole('menu', { name: 'Start' });
}

describe('Start menu', () => {
  it('shows every program as a tile, and the search field has focus', async () => {
    setup();
    const menu = await openStart();
    expect(screen.getByRole('searchbox', { name: 'Search Faber' })).toHaveFocus();
    const programs = within(menu).getByRole('group', { name: 'Programs' });
    expect(
      within(programs)
        .getAllByRole('menuitem')
        .map((m) => m.textContent?.trim()),
    ).toEqual([
      'Parts',
      'Apps',
      'Drive',
      'World',
      'Serial monitor',
      'Arduino flasher',
      'Arduino',
      'Logs',
      'Files',
      'Notes',
      'Settings',
      'About Faber',
    ]);
  });

  it('searches programs and opens the first result with Enter', async () => {
    const shell = setup();
    await openStart();
    await userEvent.keyboard('seri');
    const menu = screen.getByRole('menu', { name: 'Start' });
    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((m) => m.textContent),
    ).toEqual([expect.stringContaining('Serial monitor')]);
    await userEvent.keyboard('{Enter}');
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['serial-monitor']);
    expect(screen.queryByRole('menu', { name: 'Start' })).not.toBeInTheDocument();
  });

  it('finds parts, and opens a part with a scan in its viewer', async () => {
    const shell = setup();
    shell.pushParts([part('front_lidar')]);
    await openStart();
    await userEvent.keyboard('front');
    await userEvent.click(screen.getByRole('menuitem', { name: /front_lidar/ }));
    expect(await screen.findByRole('dialog', { name: 'front_lidar · scan' })).toBeInTheDocument();
  });

  it('lists recent files and finds them', async () => {
    const shell = setup();
    shell.desktop.noteRecent('home:/Documents/wiring.txt', Date.now());
    shell.desktop.noteRecent('home:/Documents/drive-test.md', Date.now());
    const menu = await openStart();
    const recent = within(menu).getByRole('group', { name: 'Recent files' });
    expect(
      within(recent)
        .getAllByRole('menuitem')
        .map((m) => m.textContent),
    ).toEqual([expect.stringContaining('drive-test.md'), expect.stringContaining('wiring.txt')]);
    await userEvent.keyboard('wir');
    expect(within(menu).getAllByRole('menuitem')).toHaveLength(1);
    expect(within(menu).getByRole('menuitem')).toHaveTextContent('wiring.txt');
  });

  it('says so when nothing matches', async () => {
    setup();
    await openStart();
    await userEvent.keyboard('zzz');
    expect(screen.getByText('Nothing called “zzz”.')).toBeInTheDocument();
  });

  it('has one power button whose restart and shut down say why they are not there yet', async () => {
    setup();
    await openStart();
    await userEvent.click(screen.getByRole('button', { name: 'Power options' }));
    const menu = await screen.findByRole('menu', { name: 'Power' });
    for (const name of ['Restart machine', 'Shut down machine']) {
      const it = within(menu).getByRole('menuitem', { name });
      expect(it).toHaveAttribute('aria-disabled', 'true');
      expect(it).toHaveAccessibleDescription('Available on the Faber machine image (Phase 6)');
    }
  });

  it('adds a program removed from the desktop back from its tile', async () => {
    const shell = setup();
    shell.pushSettings({ ...DEFAULT_SETTINGS, desktop: { icons: [], removed: ['tool:logs'] } });
    const start = await openStart();
    const tile = within(start).getByRole('menuitem', { name: /^Logs/ });
    await fireEvent.contextMenu(tile, { clientX: 50, clientY: 50 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Add to desktop' }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.find((c) => c.method === 'PUT')?.body as Settings).desktop.removed,
      ).toEqual([]),
    );
  });

  it('offers Remove from desktop for a program that is on it', async () => {
    setup();
    const start = await openStart();
    await fireEvent.contextMenu(within(start).getByRole('menuitem', { name: /^Logs/ }), {
      clientX: 50,
      clientY: 50,
    });
    expect(
      await screen.findByRole('menuitem', { name: 'Remove from desktop' }),
    ).toBeInTheDocument();
  });

  it('pins a program to the taskbar from its tile', async () => {
    const shell = setup();
    const menu = await openStart();
    const logs = within(menu).getByRole('menuitem', { name: 'Logs' });
    await fireEvent.contextMenu(logs, { clientX: 100, clientY: 300 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin to taskbar' }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.find((c) => c.method === 'PUT')?.body as Settings).taskbar.pinned,
      ).toContain('tool:logs'),
    );
  });

  it('lists the apps that have a window, opens one, and pins it from its menu', async () => {
    const shell = setup();
    shell.pushApps([
      WALL_FOLLOWER,
      { ...WALL_FOLLOWER, id: 'logger', name: 'Logger', ui: undefined },
    ]);
    const menu = await openStart();
    const apps = within(menu).getByRole('group', { name: 'Apps' });
    const tiles = within(apps).getAllByRole('menuitem');
    expect(tiles.map((m) => m.textContent?.trim())).toEqual(['Wall follower']);
    expect(tiles[0]!.querySelector('img')).toHaveAttribute('src', '/apps/wall-follower/icon.svg');
    await fireEvent.contextMenu(tiles[0]!, { clientX: 100, clientY: 300 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin to taskbar' }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body as Settings).taskbar
          .pinned,
      ).toContain('app:wall-follower'),
    );
    await userEvent.click(
      within(await openStartAgain()).getByRole('menuitem', { name: 'Wall follower' }),
    );
    const w = shell.desktop.state.windows.at(-1)!;
    expect([w.kind, w.key, w.params.app]).toEqual(['app', 'wall-follower', 'wall-follower']);
  });

  it('finds apps by name', async () => {
    const shell = setup();
    shell.pushApps([WALL_FOLLOWER]);
    await openStart();
    await userEvent.keyboard('wall');
    const found = screen.getByRole('group', { name: 'Apps' });
    expect(within(found).getByRole('menuitem')).toHaveTextContent('App, version 0.1.0');
  });
});
