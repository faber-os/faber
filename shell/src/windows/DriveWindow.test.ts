import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { GENERIC_BASE, fakeShell, part, simBase } from '../test/fakeShell';
import DriveWindow from './DriveWindow.svelte';

const GRANT = {
  lease: 'L1',
  part: 'base',
  holder: 'Drive window',
  deadlineUs: 5_000_000,
  durationMs: 250,
  locks: [],
};

function setup(opts: { confirmed?: boolean } = {}) {
  const shell = fakeShell([
    { path: '/api/v0/connectors', body: [GENERIC_BASE] },
    { method: 'POST', path: '/api/v0/parts/base/lease', status: 201, body: GRANT },
    { method: 'POST', path: '/api/v0/parts/base/lease/renew', body: GRANT },
    { method: 'DELETE', path: '/api/v0/parts/base/lease?lease=L1', status: 204 },
    {
      method: 'POST',
      path: '/api/v0/parts/base/command',
      body: { applied: null, clamped: false, deadlineUs: 6_000_000 },
    },
    { method: 'POST', path: '/api/v0/estop', body: { engaged: true } },
  ]);
  if (opts.confirmed) shell.core.confirmDriving();
  const id = shell.desktop.open('drive');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  const view = render(DriveWindow, { props: { win, visible: true }, context: shell.context });
  shell.pushParts([part('lidar'), simBase('base')]);
  const calls = (method: string, suffix: string) =>
    shell.fetch.calls.filter((c) => c.method === method && c.path.endsWith(suffix));
  return { shell, view, calls };
}

const commands = (calls: ReturnType<typeof setup>['calls']) =>
  calls('POST', '/command').map(
    (c) =>
      (c.body as { command: { command: { vx: number; vy: number; omega: number } } }).command
        .command,
  );

describe('Drive window', () => {
  it('says what to do when nothing can move', async () => {
    const shell = fakeShell();
    const id = shell.desktop.open('drive');
    const win = shell.desktop.state.windows.find((w) => w.id === id)!;
    render(DriveWindow, { props: { win, visible: true }, context: shell.context });
    shell.pushParts([part('lidar')]);
    expect(await screen.findByText('Nothing to drive yet')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Open Parts' }));
    expect(shell.desktop.state.windows.some((w) => w.kind === 'parts')).toBe(true);
  });

  it('asks before driving (D46), and only once per session', async () => {
    const { shell } = setup();
    shell.pushSettings();
    const ask = await screen.findByRole('alertdialog', { name: 'Start driving faber?' });
    expect(ask).toHaveTextContent('base moves while you use the pad or the keys.');
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(shell.core.drivingConfirmed).toBe(true);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(screen.getByTestId('drive-status')).toHaveTextContent(
      'Ready: use the pad or the keys to drive base.',
    );
  });

  it('drives nothing after Cancel, and offers to start', async () => {
    const { calls } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(screen.getByTestId('drive-status')).toHaveTextContent('Not driving.');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await new Promise((r) => setTimeout(r, 100));
    expect(calls('POST', '/lease')).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Start driving' }));
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
  });

  it('drives with the keys: takes the lease, sends commands while held and a stop on release', async () => {
    const { calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await waitFor(() => expect(calls('POST', '/command').length).toBeGreaterThanOrEqual(3));
    expect(calls('POST', '/lease')[0]?.body).toEqual({ holder: 'Drive window' });
    // The connector's limit (0.5 m/s) at the default speed limit of 50 %.
    expect(commands(calls)[0]).toEqual({ kind: 'set-velocity', vx: 0.25, vy: 0, omega: 0 });
    expect(screen.getByTestId('drive-status')).toHaveTextContent("You're driving base");

    await fireEvent.keyUp(window, { key: 'w', code: 'KeyW' });
    await waitFor(() =>
      expect(commands(calls).at(-1)).toEqual({ kind: 'set-velocity', vx: 0, vy: 0, omega: 0 }),
    );
  });

  it('turns with Q and E, slides with A and D, and stops with Space', async () => {
    const { calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'q', code: 'KeyQ' });
    await fireEvent.keyDown(window, { key: 'd', code: 'KeyD' });
    await waitFor(() =>
      expect(commands(calls).at(-1)).toEqual({
        kind: 'set-velocity',
        vx: 0,
        vy: -0.25,
        omega: 0.75,
      }),
    );
    await fireEvent.keyDown(window, { key: ' ', code: 'Space' });
    await waitFor(() =>
      expect(commands(calls).at(-1)).toEqual({ kind: 'set-velocity', vx: 0, vy: 0, omega: 0 }),
    );
  });

  it('scales with the speed limit', async () => {
    const { calls } = setup({ confirmed: true });
    const slider = await screen.findByRole('slider', { name: /Speed limit/ });
    await fireEvent.input(slider, { target: { value: '100' } });
    expect(screen.getByText('100 %')).toBeInTheDocument();
    await fireEvent.keyDown(window, { key: 'ArrowUp', code: 'ArrowUp' });
    await waitFor(() => expect(commands(calls)[0]?.vx).toBe(0.5));
  });

  it('lets go of the lease when another window comes to the front', async () => {
    const { shell, calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await waitFor(() => expect(calls('POST', '/command').length).toBeGreaterThan(0));
    shell.desktop.open('logs');
    await waitFor(() => expect(calls('DELETE', '/lease?lease=L1')).toHaveLength(1));
    const n = calls('POST', '/command').length;
    await new Promise((r) => setTimeout(r, 150));
    expect(calls('POST', '/command').length).toBe(n);
  });

  it('lets go of the lease when the browser goes to the background', async () => {
    const { calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await waitFor(() => expect(calls('POST', '/command').length).toBeGreaterThan(0));
    await fireEvent.blur(window);
    await waitFor(() => expect(calls('DELETE', '/lease?lease=L1')).toHaveLength(1));
    await fireEvent.focus(window);
  });

  it('lets go of the lease when the window closes', async () => {
    const { view, calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await waitFor(() => expect(calls('POST', '/command').length).toBeGreaterThan(0));
    view.unmount();
    await waitFor(() => expect(calls('DELETE', '/lease?lease=L1')).toHaveLength(1));
  });

  it('shows who else drives, and takes over when asked', async () => {
    const { shell, calls } = setup({ confirmed: true });
    shell.pushLeases([{ part: 'base', holder: 'drive-square.py' }]);
    expect(await screen.findByTestId('drive-status')).toHaveTextContent(
      'Driven by drive-square.py',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Take over' }));
    await waitFor(() =>
      expect(calls('POST', '/lease')[0]?.body).toEqual({
        holder: 'Drive window',
        takeOver: true,
      }),
    );
  });

  it('says when the e-stop stopped it, and when its lease ended', async () => {
    const { shell, calls } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    await waitFor(() => expect(calls('POST', '/command').length).toBeGreaterThan(0));
    shell.receive({
      type: 'lease-ended',
      id: shell.subscriptionId('leases'),
      lease: { part: 'base', holder: 'Drive window', deadlineUs: 5_100_000, locks: [] },
      reason: 'part-restarted',
    });
    expect(await screen.findByText(/base restarted because its settings changed/)).toBeVisible();

    shell.pushEstop(true);
    expect(await screen.findByText('Stopped by the e-stop.')).toBeInTheDocument();
    expect(screen.getByText(/Release the e-stop on the taskbar/)).toBeInTheDocument();

    // The core ends every lease on an e-stop; once released, the window is ready again.
    await fireEvent.keyDown(window, { key: 'w', code: 'KeyW' });
    shell.pushEstop(false);
    await waitFor(() =>
      expect(screen.getByTestId('drive-status')).toHaveTextContent('Ready: use the pad'),
    );
  });

  it('has a big Stop that engages the e-stop at once', async () => {
    const { shell, calls } = setup({ confirmed: true });
    await userEvent.click(await screen.findByRole('button', { name: /^Stop$/ }));
    expect(shell.core.estop?.engaged).toBe(true);
    await waitFor(() =>
      expect(calls('POST', '/estop')[0]?.body).toEqual({
        reason: 'Stop button in the Drive window',
        source: 'shell',
      }),
    );
  });

  it('shows the commanded and measured velocity', async () => {
    const { shell } = setup({ confirmed: true });
    await screen.findByTestId('drive-pad');
    shell.pushReading('base/planar-base', {
      capability: 'planar-base@1',
      reading: {
        tUs: 1,
        commanded: { vx: 0.2, vy: 0, omega: 0 },
        measured: { vx: 0.1, vy: 0, omega: 0 },
      },
    });
    expect(await screen.findByTestId('twist-commanded-vx')).toHaveTextContent('0.2 m/s');
    expect(screen.getByTestId('twist-measured-vx')).toHaveTextContent('0.1 m/s');
  });
});
