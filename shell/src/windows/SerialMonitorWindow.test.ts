import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { decode } from '@msgpack/msgpack';
import { SAMPLE_PORT, USB_PORT, fakeShell } from '../test/fakeShell';
import SerialMonitorWindow from './SerialMonitorWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';

const win: WindowState = {
  id: 'w1',
  kind: 'serial-monitor',
  key: 'serial-monitor',
  title: 'Serial monitor',
  rect: { x: 0, y: 0, w: 600, h: 400 },
  chosen: { x: 0, y: 0, w: 600, h: 400 },
  maximised: false,
  minimised: false,
  z: 1,
  params: {},
};

async function setup() {
  const shell = fakeShell();
  shell.pushPorts([USB_PORT, SAMPLE_PORT]);
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
    expect(screen.getByLabelText('Port')).toHaveValue('replay:ld19-sample');
    await userEvent.selectOptions(screen.getByLabelText('Baud'), '230400');
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    expect(lastSent(shell)).toMatchObject({
      type: 'serial-open',
      port: 'replay:ld19-sample',
      baud: 230400,
    });
    // Not open until the core says so; it can still be cancelled meanwhile.
    expect(screen.getByRole('status')).toHaveTextContent('Opening');
    expect(screen.getByLabelText('Line to send')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Close port' })).toBeInTheDocument();

    shell.receive({ type: 'serial-opened', id: lastSent(shell).id, port: 'replay:ld19-sample' });
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
    shell.receive({ type: 'serial-opened', id, port: 'replay:ld19-sample' });
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

  async function openPort(shell: ReturnType<typeof fakeShell>) {
    await userEvent.click(screen.getByRole('button', { name: 'Open port' }));
    const id = lastSent(shell).id as number;
    shell.receive({ type: 'serial-opened', id, port: 'replay:ld19-sample' });
    await userEvent.click(await screen.findByRole('button', { name: 'Record' }));
    return id;
  }

  it('records an open port into Recordings and says how much it saved', async () => {
    const shell = await setup();
    const id = await openPort(shell);
    const name = screen.getByLabelText('Recording name') as HTMLInputElement;
    expect(name.value).toMatch(/^recording_\d{4}_\d{2}_\d{2}_\d{4}$/);
    await userEvent.clear(name);
    await userEvent.type(name, 'Bench Test');
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(screen.getByText(/isn't a valid recording name/)).toBeInTheDocument();
    await userEvent.clear(name);
    await userEvent.type(name, 'bench_test');
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    expect(lastSent(shell)).toEqual({ type: 'serial-record-start', id, name: 'bench_test' });

    shell.receive({ type: 'serial-recording', id, path: 'home:/Recordings/bench_test.fbcap' });
    shell.receive({ type: 'serial-data', id, tUs: 1, data: new Uint8Array(10) });
    await expect
      .poll(() => document.body.textContent)
      .toContain('Recording to home:/Recordings/bench_test.fbcap (10 B)');
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(lastSent(shell)).toEqual({ type: 'serial-record-stop', id });
    shell.receive({
      type: 'serial-recording-stopped',
      id,
      path: 'home:/Recordings/bench_test.fbcap',
      bytes: 2048,
    });
    expect(await screen.findByText(/Saved/)).toHaveTextContent('2.0 KiB');
  });

  it('closing while recording shows what the core saved, from its stop report', async () => {
    const shell = await setup();
    const id = await openPort(shell);
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    shell.receive({ type: 'serial-recording', id, path: 'home:/Recordings/r.fbcap' });
    shell.receive({ type: 'serial-data', id, tUs: 1, data: new Uint8Array(10) });
    await expect.poll(() => document.body.textContent).toContain('(10 B)');
    await userEvent.click(screen.getByRole('button', { name: 'Close port' }));
    expect(lastSent(shell)).toEqual({ type: 'serial-close', id });
    // What this window counted, until the core's own count arrives.
    expect(await screen.findByText(/Saved/)).toHaveTextContent('10 B');
    shell.receive({
      type: 'serial-recording-stopped',
      id,
      path: 'home:/Recordings/r.fbcap',
      bytes: 3072,
    });
    shell.receive({ type: 'serial-closed', id });
    await expect.poll(() => screen.getByText(/Saved/).textContent).toContain('3.0 KiB');
  });

  it('goes back to naming when the core refuses the name', async () => {
    const shell = await setup();
    const id = await openPort(shell);
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    const error = {
      code: 'already-exists',
      message: 'There is already a recording called that.',
      hint: 'Pick another name.',
    };
    shell.receive({ type: 'error', id, error });
    expect(await screen.findByText(error.message)).toBeInTheDocument();
    expect(screen.getByLabelText('Recording name')).toBeInTheDocument();
  });

  it('names Bluetooth ports as Bluetooth', async () => {
    const shell = fakeShell();
    shell.pushPorts([
      {
        id: 'serial:COM7',
        kind: 'bluetooth',
        label: 'Standard Serial over Bluetooth link (COM7)',
        suggestions: [],
      },
    ]);
    render(SerialMonitorWindow, { props: { win, visible: true }, context: shell.context });
    expect(
      await screen.findByRole('option', { name: /^Bluetooth: Standard Serial/ }),
    ).toBeInTheDocument();
  });

  it('names a port once: no id after a label that already ends in it', async () => {
    const shell = fakeShell();
    shell.pushPorts([
      // Two Megas: the core can only tell them apart by their COM port.
      { id: 'serial:COM6', kind: 'usb-serial', label: 'Arduino Mega 2560 (COM6)', suggestions: [] },
      { id: 'serial:COM9', kind: 'serial', label: 'COM9', suggestions: [] },
      {
        id: 'serial:COM7',
        kind: 'bluetooth',
        label: 'Standard Serial over Bluetooth link (COM7)',
        suggestions: [],
      },
      {
        id: 'usb:2341:0042',
        kind: 'usb-serial',
        label: 'Arduino Mega 2560 (COM4)',
        suggestions: [],
      },
    ]);
    render(SerialMonitorWindow, { props: { win, visible: true }, context: shell.context });
    await screen.findByRole('option', { name: 'Arduino Mega 2560 (COM6)' });
    const names = screen.getAllByRole('option').map((o) => o.textContent?.trim());
    expect(names).toContain('COM9');
    expect(names).toContain('Bluetooth: Standard Serial over Bluetooth link (COM7)');
    // A USB id says something the label doesn't (which board model), so it stays.
    expect(names).toContain('Arduino Mega 2560 (COM4) (usb:2341:0042)');
  });

  it('lets a port problem be dismissed, as in Parts', async () => {
    const shell = await setup();
    const problem = {
      code: 'usb-no-answer',
      label: "Plugged in, but the computer can't talk to it",
      detail: "It didn't answer.",
      hint: 'Try another cable.',
    };
    shell.receive({
      type: 'ports',
      id: shell.subscriptionId('ports'),
      ports: [USB_PORT, SAMPLE_PORT],
      problems: [problem],
    });
    await screen.findByText(problem.label);
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(problem.label)).not.toBeInTheDocument();
    expect(shell.desktop.isDismissed('portProblems:usb-no-answer', ['usb-no-answer'])).toBe(true);
  });
});
