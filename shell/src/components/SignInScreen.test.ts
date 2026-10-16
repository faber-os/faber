import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, sessionAs } from '../test/fakeShell';
import SignInScreen from './SignInScreen.svelte';

function setup(mode: 'sign-in' | 'setup', routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  shell.core.auth.info = {
    ...sessionAs('admin'),
    machineName: 'sennep',
    needsSetup: mode === 'setup',
    identity: undefined,
  };
  const ondone = vi.fn();
  render(SignInScreen, { props: { mode, ondone }, context: shell.context });
  return { shell, ondone };
}

describe('Signing in', () => {
  it('shows the machine’s name and signs in through the core', async () => {
    const { shell, ondone } = setup('sign-in', [
      { method: 'POST', path: '/api/v0/session', body: sessionAs('operator', 'olive') },
    ]);
    expect(screen.getByRole('heading', { name: 'Sign in to sennep' })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Name'), 'Olive');
    await userEvent.type(screen.getByLabelText('Password'), 'password1');
    await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await waitFor(() => expect(ondone).toHaveBeenCalled());
    const post = shell.fetch.calls.find((c) => c.path === '/api/v0/session');
    expect(post?.body).toEqual({ name: 'olive', password: 'password1' });
    expect(shell.core.auth.role).toBe('operator');
  });

  it('says why it didn’t work, and stays', async () => {
    const { ondone } = setup('sign-in', [
      {
        method: 'POST',
        path: '/api/v0/session',
        status: 401,
        body: {
          error: {
            code: 'sign-in-failed',
            message: 'That name and password don’t match.',
            hint: 'Check them.',
          },
        },
      },
    ]);
    await userEvent.type(screen.getByLabelText('Name'), 'olive');
    await userEvent.type(screen.getByLabelText('Password'), 'wrong one');
    await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('don’t match');
    expect(ondone).not.toHaveBeenCalled();
  });

  it('can always press Stop, without signing in', async () => {
    const { shell } = setup('sign-in', [
      { method: 'POST', path: '/api/v0/estop', body: { engaged: true } },
    ]);
    await userEvent.click(screen.getByRole('button', { name: /^Stop/ }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path === '/api/v0/estop')?.body).toEqual({
        reason: 'Stop button on the sign-in screen',
        source: 'shell',
      }),
    );
    expect(await screen.findByText('Stopped')).toBeInTheDocument();
  });
});

describe('The first run', () => {
  it('creates the admin, once both passwords match', async () => {
    const { shell, ondone } = setup('setup', [
      { method: 'POST', path: '/api/v0/setup', status: 201, body: sessionAs('admin') },
    ]);
    expect(screen.getByRole('heading', { name: 'Create the admin' })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Name'), 'mikkel');
    await userEvent.type(screen.getByLabelText('Password'), 'correct horse');
    await userEvent.type(screen.getByLabelText('Repeat the password'), 'correct hors');
    expect(screen.getByRole('alert')).toHaveTextContent('not the same');
    const create = screen.getByRole('button', { name: 'Create the admin' });
    expect(create).toBeDisabled();
    await userEvent.type(screen.getByLabelText('Repeat the password'), 'e');
    await userEvent.click(create);
    await waitFor(() => expect(ondone).toHaveBeenCalled());
    expect(shell.fetch.calls.find((c) => c.path === '/api/v0/setup')?.body).toEqual({
      name: 'mikkel',
      password: 'correct horse',
    });
  });
});
