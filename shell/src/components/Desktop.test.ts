import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell } from '../test/fakeShell';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';

function setup(viewport?: { width: number; height: number }) {
  const shell = fakeShell([], viewport);
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
}

describe('Desktop', () => {
  it('opens windows from the start menu', async () => {
    const shell = setup();
    const start = screen.getByRole('button', { name: 'Start' });
    await userEvent.click(start);
    expect(start).toHaveAttribute('aria-expanded', 'true');
    const menu = screen.getByRole('menu', { name: 'Start' });
    await userEvent.click(within(menu).getByRole('menuitem', { name: /Logs/ }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(await screen.findByRole('dialog', { name: 'Logs' })).toBeInTheDocument();
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['logs']);
  });

  it('closes the start menu with Escape and gives focus back to the start button', async () => {
    setup();
    const start = screen.getByRole('button', { name: 'Start' });
    await userEvent.click(start);
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(start).toHaveFocus();
  });

  it('minimises, restores from the taskbar, maximises and closes a window', async () => {
    const shell = setup();
    shell.desktop.open('about');
    const dialog = await screen.findByRole('dialog', { name: 'About Faber' });

    await userEvent.click(within(dialog).getByRole('button', { name: 'Minimise About Faber' }));
    expect(dialog).not.toBeVisible();

    const task = within(screen.getByRole('list', { name: 'Programs and windows' })).getByRole(
      'button',
      {
        name: /About Faber/,
      },
    );
    await userEvent.click(task);
    expect(dialog).toBeVisible();
    expect(task).toHaveAttribute('aria-pressed', 'true');

    await userEvent.click(within(dialog).getByRole('button', { name: 'Maximise About Faber' }));
    expect(within(dialog).getByRole('button', { name: 'Restore About Faber' })).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole('button', { name: 'Close About Faber' }));
    expect(screen.queryByRole('dialog', { name: 'About Faber' })).not.toBeInTheDocument();
  });

  it('moves a window with the keyboard from its title bar', async () => {
    const shell = setup();
    shell.desktop.open('about');
    const dialog = await screen.findByRole('dialog', { name: 'About Faber' });
    const before = shell.desktop.state.windows[0]!.rect;
    within(dialog).getByRole('toolbar').focus();
    await userEvent.keyboard('{ArrowRight}{ArrowDown}');
    const after = shell.desktop.state.windows[0]!.rect;
    expect(after.x).toBe(before.x + 16);
    expect(after.y).toBe(before.y + 16);
  });

  it('shows the connection state in the taskbar', async () => {
    const shell = setup();
    expect(screen.getByText('Connected')).toBeInTheDocument();
    shell.ws.drop();
    expect(await screen.findByText('Reconnecting…')).toBeInTheDocument();
  });

  it('lists each part viewer in the start menu', async () => {
    const shell = setup();
    shell.pushParts([
      {
        name: 'demo',
        connector: 'ldrobot-ld19',
        port: 'replay:ld19-sample',
        state: { kind: 'running' },
        signals: [{ address: 'demo/laser-scan', capability: 'laser-scan@1' }],
        mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
      },
    ]);
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await userEvent.click(screen.getByRole('menuitem', { name: /demo\/laser-scan/ }));
    expect(await screen.findByRole('dialog', { name: 'demo · scan' })).toBeInTheDocument();
  });

  it('draws the wallpaper from the settings', async () => {
    const shell = setup();
    const wall = document.querySelector('.wall')!;
    expect(wall.getAttribute('data-wallpaper')).toBe('aluminium');
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      appearance: { theme: 'dark', style: 'aluminium', wallpaper: 'cutting-mat', language: 'en' },
    });
    await expect.poll(() => wall.getAttribute('data-wallpaper')).toBe('cutting-mat');
  });
});
