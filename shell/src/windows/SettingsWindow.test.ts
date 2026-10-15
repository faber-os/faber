import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell } from '../test/fakeShell';
import SettingsWindow from './SettingsWindow.svelte';

function setup(routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  shell.pushSettings();
  const id = shell.desktop.open('settings');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(SettingsWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

const ON = { ...DEFAULT_SETTINGS, developer: { fullMachineFiles: true } };

describe('Settings', () => {
  it('changes the theme for every screen through the core', async () => {
    const dark = {
      ...DEFAULT_SETTINGS,
      appearance: { theme: 'dark' as const, wallpaper: 'aluminium' },
    };
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: dark }]);
    await userEvent.click(await screen.findByRole('radio', { name: 'Dark' }));
    await waitFor(() => expect(shell.core.settings?.appearance.theme).toBe('dark'));
    expect(shell.fetch.calls.find((c) => c.method === 'PUT')?.body).toEqual(dark);
  });

  it('picks a wallpaper', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    await userEvent.click(await screen.findByRole('radio', { name: /Cutting mat/ }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.find((c) => c.method === 'PUT')?.body as typeof DEFAULT_SETTINGS)
          .appearance.wallpaper,
      ).toBe('cutting-mat'),
    );
  });

  it('follows changes made on another screen', async () => {
    const shell = setup();
    await screen.findByRole('radio', { name: 'System' });
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      appearance: { theme: 'light', wallpaper: 'graphite' },
    });
    await waitFor(() => expect(screen.getByRole('radio', { name: 'Light' })).toBeChecked());
  });

  it('turns on whole-machine files only after the warning is confirmed', async () => {
    const shell = setup([{ method: 'POST', path: '/api/v0/settings/developer', body: ON }]);
    const toggle = await screen.findByRole('switch', {
      name: 'Allow Files to see the whole machine',
    });
    await userEvent.click(toggle);
    expect(toggle).not.toBeChecked();
    const dialog = screen.getByRole('alertdialog', { name: 'Let Files see the whole machine?' });
    expect(dialog).toHaveTextContent('no Trash outside the home folder');
    const allow = within(dialog).getByRole('button', { name: 'Allow whole-machine access' });
    expect(allow).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('checkbox'));
    expect(allow).toBeDisabled();
    await userEvent.type(within(dialog).getByRole('textbox'), 'machine');
    expect(allow).toBeEnabled();
    expect(shell.fetch.calls.some((c) => c.method === 'POST')).toBe(false);
    await userEvent.click(allow);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toEqual({
      fullMachineFiles: true,
      acknowledgedRisk: true,
    });
  });

  it('cancelling the warning leaves it off', async () => {
    const shell = setup();
    await userEvent.click(await screen.findByRole('switch'));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(screen.getByRole('switch')).not.toBeChecked();
    expect(shell.fetch.calls.some((c) => c.method === 'POST')).toBe(false);
  });

  it('turns it off without a warning', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/settings/developer', body: DEFAULT_SETTINGS },
    ]);
    shell.pushSettings(ON);
    const toggle = await screen.findByRole('switch');
    await waitFor(() => expect(toggle).toBeChecked());
    await userEvent.click(toggle);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toEqual({
        fullMachineFiles: false,
        acknowledgedRisk: false,
      }),
    );
  });
});
