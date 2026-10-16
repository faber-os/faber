import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell } from '../../test/fakeShell';
import SettingsWindow from '../SettingsWindow.svelte';
import type { UserRole } from '../../lib/state/types';

function setup(role: UserRole = 'admin', routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  shell.signInAs(role);
  shell.pushSettings();
  const id = shell.desktop.open('settings');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(SettingsWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

const users = () => screen.getByRole('region', { name: 'Users' });

describe('Settings → Users, as an admin', () => {
  it('lists the users and adds one', async () => {
    const shell = setup('admin', [
      { method: 'POST', path: '/api/v0/users', status: 201, body: { name: 'vic', role: 'viewer' } },
    ]);
    expect(await within(users()).findByText('mikkel')).toBeInTheDocument();
    const form = within(users()).getByRole('form', { name: 'Add a user' });
    await userEvent.type(within(form).getByLabelText('Name'), 'Vic');
    await userEvent.type(within(form).getByLabelText('Password'), 'password1');
    await userEvent.click(within(form).getByRole('button', { name: 'Add user' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.find((c) => c.path === '/api/v0/users' && c.method === 'POST')?.body,
      ).toEqual({
        name: 'vic',
        password: 'password1',
        role: 'viewer',
      }),
    );
    expect(await within(users()).findByText('Added vic.')).toBeInTheDocument();
  });

  it('can’t remove itself, and says why', async () => {
    setup('admin');
    const remove = await within(users()).findByRole('button', { name: 'Remove mikkel' });
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute('title', expect.stringMatching(/can’t remove yourself/));
  });

  it('makes an API token and shows it once', async () => {
    const shell = setup('admin', [
      {
        method: 'POST',
        path: '/api/v0/tokens',
        status: 201,
        body: {
          token: {
            id: 'ab12',
            name: 'drive-square',
            user: 'mikkel',
            role: 'operator',
            created: '2026-10-16T10:00:00Z',
          },
          secret: 'faber_secret',
        },
      },
    ]);
    const form = await within(users()).findByRole('form', { name: 'New API token' });
    await userEvent.type(within(form).getByLabelText('What it is for'), 'drive-square');
    await userEvent.selectOptions(within(form).getByLabelText('Role'), 'operator');
    await userEvent.click(within(form).getByRole('button', { name: 'Make token' }));
    const secret = await within(users()).findByRole('textbox', { name: 'The new API token' });
    expect(secret).toHaveValue('faber_secret');
    expect(
      shell.fetch.calls.find((c) => c.path === '/api/v0/tokens' && c.method === 'POST')?.body,
    ).toEqual({
      name: 'drive-square',
      role: 'operator',
    });
    await userEvent.click(within(users()).getByRole('button', { name: 'Done' }));
    expect(within(users()).queryByRole('textbox', { name: 'The new API token' })).toBeNull();
  });
});

describe('Settings, as a viewer', () => {
  it('shows what a viewer can’t change as disabled, with the reason', async () => {
    setup('viewer');
    const add = await within(users()).findByRole('button', { name: 'Add user' });
    expect(add).toBeDisabled();
    expect(add).toHaveAttribute('title', 'Needs the admin role');
    expect(within(users()).getByText('Needs the admin role')).toBeInTheDocument();
    // The look needs an operator; the machine's name an admin.
    expect(screen.getByRole('radio', { name: 'Dark' })).toBeDisabled();
    expect(screen.getAllByText('Needs the operator role').length).toBeGreaterThan(0);
    expect(screen.getByLabelText('Machine name')).toBeDisabled();
    // A viewer may still make tokens for themselves (viewer tokens only).
    const role = within(
      within(users()).getByRole('form', { name: 'New API token' }),
    ).getByLabelText('Role');
    expect(
      within(role)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['Viewer']);
  });

  it('lets an operator change the look but not the machine', async () => {
    setup('operator');
    expect(await screen.findByRole('radio', { name: 'Dark' })).toBeEnabled();
    expect(screen.getByLabelText('Machine name')).toBeDisabled();
    expect(
      screen.getByRole('radiogroup', { name: 'The attached screen is signed in as' }),
    ).toBeInTheDocument();
  });
});
