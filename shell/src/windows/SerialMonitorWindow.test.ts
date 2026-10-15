import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { decode } from '@msgpack/msgpack';
import { DEMO_PORT, USB_PORT, fakeShell } from '../test/fakeShell';
import SerialMonitorWindow from './SerialMonitorWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';

const win: WindowState = {
  id: 'w1',
  kind: 'serial-monitor',
  key: 'serial-monitor',
  title: 'Serial monitor',
  rect: { x: 0, y: 0, w: 600, h: 400 },
  maximised: false,
  minimised: false,
  z: 1,
  params: {},
};

async function setup() {
  const shell = fakeShell();
  shell.pushPorts([USB_PORT, DEMO_PORT]);
  render(SerialMonitorWindow, { props: { win, visible: true }, context: shell.context });
  await screen.findByRole('option', { name: /usb:10c4:ea60/ });
  return shell;
}

function lastSent(shell: ReturnType<typeof fakeShell>) {
  return decode(shell.ws.sent.at(-1) as Uint8Array) as Record<string, unknown>;
}

describe('Serial monitor', () => {
  it('picks a free port first and opens it at the chosen baud', async () => {
    const shell = await setup();
    expect(screen.getByLabelText('Port')).toHaveValue('replay:demo-ld19');
    await userEvent.selectOptions(screen.getByLabelText('Baud'), '230400');
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    expect(lastSent(shell)).toMatchObject({
      type: 'serial-open',
      port: 'replay:demo-ld19',
      baud: 230400,
    });
    // Not open until the core says so; it can still be cancelled meanwhile.
    expect(screen.getByRole('status')).toHaveTextContent('Opening');
    expect(screen.getByLabelText('Line to send')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Close port' })).toBeInTheDocument();

    shell.receive({ type: 'serial-opened', id: lastSent(shell).id, port: 'replay:demo-ld19' });
    await expect.poll(() => screen.getByRole('status').textContent).toContain('230400');
    expect(screen.getByLabelText('Line to send')).toBeEnabled();
  });

  it('shows the port-claimed error clearly, with its hint', async () => {
    const shell = await setup();
    await userEvent.selectOptions(screen.getByLabelText('Port'), 'usb:10c4:ea60');
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    const id = lastSent(shell).id;
    shell.receive({
      type: 'error',
      id,
      error: {
        code: 'port-claimed',
        message: 'usb:10c4:ea60 is used by the part "lidar".',
        hint: 'Remove or stop "lidar" first, or pick another port.',
      },
    });
    const alert = await screen.findByRole('alert');
    expect(
      within(alert).getByText('usb:10c4:ea60 is used by the part "lidar".'),
    ).toBeInTheDocument();
    expect(
      within(alert).getByText('Remove or stop "lidar" first, or pick another port.'),
    ).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Open port' })).toBeInTheDocument();
  });

  it('shows received bytes as text or hex, and sends a line', async () => {
    const shell = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    const id = lastSent(shell).id;
    shell.receive({ type: 'serial-opened', id, port: 'replay:demo-ld19' });
    shell.receive({ type: 'serial-data', id, tUs: 1, data: new TextEncoder().encode('hello\r\n') });
    const out = screen.getByLabelText('Received data');
    await expect.poll(() => out.textContent).toContain('hello');

    await userEvent.click(screen.getByRole('radio', { name: 'Hex' }));
    await expect.poll(() => out.textContent).toContain('68 65 6c 6c 6f 0d 0a');

    await userEvent.type(screen.getByLabelText('Line to send'), 'ping{Enter}');
    const sent = lastSent(shell) as { type: string; data: Uint8Array };
    expect(sent.type).toBe('serial-send');
    expect([...sent.data]).toEqual([...new TextEncoder().encode('ping\n')]);
  });

  it('says why the port closed when the core closes it', async () => {
    const shell = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    shell.receive({
      type: 'serial-closed',
      id: lastSent(shell).id,
      reason: 'The adapter was unplugged.',
    });
    expect(await screen.findByText(/Closed: The adapter was unplugged\./)).toBeInTheDocument();
  });
});
