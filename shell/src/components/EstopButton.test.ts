import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, part, simBase } from '../test/fakeShell';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';

function setup() {
  const shell = fakeShell([
    { method: 'POST', path: '/api/v0/estop', body: { engaged: true } },
    { method: 'POST', path: '/api/v0/estop/release', body: { engaged: false } },
  ]);
  shell.pushSettings(DEFAULT_SETTINGS);
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
}

const nav = () => screen.getByRole('navigation', { name: 'Taskbar' });
const posts = (shell: ReturnType<typeof setup>, path: string) =>
  shell.fetch.calls.filter((c) => c.method === 'POST' && c.path === path);

describe('E-stop on the taskbar', () => {
  it('shows a Stop button only while a part can move', async () => {
    const shell = setup();
    shell.pushParts([part('lidar')]);
    expect(within(nav()).queryByRole('button', { name: /^Stop/ })).not.toBeInTheDocument();
    shell.pushParts([part('lidar'), simBase('base')]);
    expect(await within(nav()).findByRole('button', { name: /^Stop/ })).toBeInTheDocument();
  });

  it('engages at once, without asking, and then shows Stopped', async () => {
    const shell = setup();
    shell.pushParts([simBase('base')]);
    await userEvent.click(await within(nav()).findByRole('button', { name: /^Stop/ }));
    expect(within(nav()).getByText('Stopped')).toBeInTheDocument();
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    await waitFor(() =>
      expect(posts(shell, '/api/v0/estop')[0]?.body).toEqual({
        reason: 'Stop button on the taskbar',
        source: 'shell',
      }),
    );
  });

  it('shows Stopped and Release when another screen or program engaged it', async () => {
    const shell = setup();
    shell.pushParts([part('lidar')]);
    shell.pushEstop(true, 'drive-square.py lost its way');
    expect(await within(nav()).findByText('Stopped')).toBeInTheDocument();
    expect(within(nav()).getByRole('button', { name: 'Release' })).toBeInTheDocument();
  });

  it('engages from anywhere with the Pause/Break key, but not with Escape', async () => {
    const shell = setup();
    shell.pushParts([simBase('base')]);
    await fireEvent.keyDown(document.body, { key: 'Escape', code: 'Escape' });
    expect(posts(shell, '/api/v0/estop')).toEqual([]);
    await fireEvent.keyDown(document.body, { key: 'Pause', code: 'Pause' });
    await waitFor(() =>
      expect(posts(shell, '/api/v0/estop')[0]?.body).toEqual({
        reason: 'Pause/Break key',
        source: 'shell',
      }),
    );
  });

  it('engages with Pause/Break even when a window stops the key from going further', async () => {
    const shell = setup();
    shell.pushParts([simBase('base')]);
    const input = document.createElement('input');
    input.addEventListener('keydown', (e) => e.stopPropagation());
    document.body.append(input);
    await fireEvent.keyDown(input, { key: 'Pause', code: 'Pause' });
    await waitFor(() => expect(posts(shell, '/api/v0/estop')).toHaveLength(1));
    input.remove();
  });

  it('asks before releasing, and can keep it stopped', async () => {
    const shell = setup();
    shell.pushParts([simBase('base')]);
    shell.pushEstop(true);
    await userEvent.click(await within(nav()).findByRole('button', { name: 'Release' }));
    const ask = screen.getByRole('alertdialog', { name: 'Release the e-stop?' });
    await userEvent.click(within(ask).getByRole('button', { name: 'Keep stopped' }));
    expect(posts(shell, '/api/v0/estop/release')).toEqual([]);

    await userEvent.click(within(nav()).getByRole('button', { name: 'Release' }));
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Release' }),
    );
    await waitFor(() => expect(posts(shell, '/api/v0/estop/release')).toHaveLength(1));
    expect(await within(nav()).findByRole('button', { name: /^Stop/ })).toBeInTheDocument();
  });
});
