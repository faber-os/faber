import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, part } from '../test/fakeShell';
import type { Settings } from '../lib/state/types';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';

function setup() {
  const shell = fakeShell([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
  shell.pushSettings();
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
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
      'Drive',
      'World',
      'Serial monitor',
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
});
