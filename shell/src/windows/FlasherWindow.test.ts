import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { MEGA_PORT, SAMPLE_PORT, fakeShell, listing } from '../test/fakeShell';
import type { FakeRoute } from '../test/fakeFetch';
import FlasherWindow from './FlasherWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';
import type { PortInfo } from '../lib/state/types';

const win: WindowState = {
  id: 'w1',
  kind: 'arduino-flasher',
  key: 'arduino-flasher',
  title: 'Arduino flasher',
  rect: { x: 0, y: 0, w: 560, h: 600 },
  chosen: { x: 0, y: 0, w: 560, h: 600 },
  maximised: false,
  minimised: false,
  z: 1,
  params: {},
};

const BOARDS = [
  { id: 'uno', name: 'Arduino Uno', chip: 'ATmega328P', room: 32256 },
  { id: 'nano', name: 'Arduino Nano', chip: 'ATmega328P', room: 32256 },
  { id: 'mega2560', name: 'Arduino Mega 2560', chip: 'ATmega2560', room: 253952 },
];

const UNO_PORT: PortInfo = {
  id: 'usb:2341:0043',
  kind: 'usb-serial',
  label: 'Arduino Uno (COM4)',
  usb: { vid: 0x2341, pid: 0x0043 },
  suggestions: [],
  board: { board: 'uno' },
};

const CLONE_PORT: PortInfo = {
  id: 'usb:1a86:7523',
  kind: 'usb-serial',
  label: 'USB-SERIAL CH340 (COM7)',
  usb: { vid: 0x1a86, pid: 0x7523 },
  suggestions: [],
  board: { usbChip: 'CH340' },
};

const FLASH_URL = (port: string) => `/api/v0/ports/${encodeURIComponent(port)}/flash`;

const RESULT = {
  port: UNO_PORT.id,
  board: 'uno',
  boardName: 'Arduino Uno',
  bytes: 3000,
  pages: 24,
};

const PACKAGE = {
  id: 'my-bridge',
  name: 'My bridge',
  version: '0.2.0',
  kind: 'declarative',
  installedAt: '2026-10-15T10:00:00Z',
  source: 'my-bridge-0.2.0.faberpkg',
  checked: false,
  usedBy: [],
  firmware: ['uno'],
};

async function setup(
  routes: FakeRoute[] = [],
  ports: PortInfo[] = [SAMPLE_PORT, UNO_PORT, CLONE_PORT],
) {
  const shell = fakeShell([
    { path: '/api/v0/flasher/boards', body: BOARDS },
    listing('home:/Documents', [{ name: 'blink.hex', size: 8000 }]),
    ...routes,
  ]);
  shell.pushPorts(ports);
  render(FlasherWindow, { props: { win, visible: true }, context: shell.context });
  await screen.findByRole('option', { name: /Arduino Uno \(COM4\).*· Arduino Uno/ });
  return shell;
}

async function chooseFile() {
  await userEvent.click(screen.getByRole('button', { name: 'Choose…' }));
  const picker = await screen.findByRole('alertdialog', { name: 'Open a file' });
  await userEvent.click(await within(picker).findByRole('button', { name: 'blink.hex' }));
  await userEvent.click(within(picker).getByRole('button', { name: 'Open' }));
}

function flashCalls(shell: ReturnType<typeof fakeShell>, port = UNO_PORT.id) {
  return shell.fetch.calls.filter((c) => c.method === 'POST' && c.path === FLASH_URL(port));
}

describe('Arduino flasher', () => {
  it('picks the Arduino and its board, warns, and flashes only after the confirm', async () => {
    let finish!: () => void;
    const shell = await setup([
      {
        method: 'POST',
        path: FLASH_URL(UNO_PORT.id),
        body: RESULT,
        until: new Promise<void>((r) => (finish = r)),
      },
    ]);
    expect(screen.getByLabelText('Port')).toHaveValue(UNO_PORT.id);
    expect(screen.getByLabelText('Board')).toHaveValue('uno');
    expect(screen.getByText('Its USB ids say it is an Arduino Uno.')).toBeInTheDocument();
    expect(screen.getByRole('note')).toHaveTextContent('This replaces the program on the board');
    // Recordings have no board on them.
    expect(screen.queryByRole('option', { name: /ld19-sample/ })).toBeNull();

    const flash = screen.getByRole('button', { name: 'Flash' });
    expect(flash).toBeDisabled();
    await chooseFile();
    expect(screen.getByText('home:/Documents/blink.hex')).toBeInTheDocument();
    expect(flash).toBeEnabled();

    // The confirm can be cancelled: nothing is sent.
    await userEvent.click(flash);
    let ask = screen.getByRole('alertdialog', { name: 'Replace the program on the board?' });
    expect(ask).toHaveTextContent('Faber puts blink.hex on the Arduino Uno on Arduino Uno (COM4)');
    await userEvent.click(within(ask).getByRole('button', { name: 'Cancel' }));
    expect(flashCalls(shell)).toHaveLength(0);

    await userEvent.click(screen.getByRole('button', { name: 'Flash' }));
    ask = screen.getByRole('alertdialog', { name: 'Replace the program on the board?' });
    await userEvent.click(within(ask).getByRole('button', { name: 'Replace and flash' }));
    expect(flashCalls(shell)[0]!.body).toEqual({
      board: 'uno',
      firmware: { kind: 'file', path: 'home:/Documents/blink.hex' },
    });

    // Progress from the flash topic, for this port only.
    const bar = await screen.findByRole('progressbar', { name: 'Flashing progress' });
    expect(screen.getByRole('status')).toHaveTextContent('Waiting for the bootloader…');
    const id = shell.subscriptionId('flash');
    const tell = (port: string, stage: string, done: number) =>
      shell.receive({ type: 'flash', id, port, stage, done, total: 3072 });
    tell(UNO_PORT.id, 'writing', 1536);
    tell(MEGA_PORT.id, 'verifying', 3072);
    await expect.poll(() => screen.getByRole('status').textContent).toContain('Writing… 50 %');
    expect(bar).toHaveAttribute('aria-valuenow', '25');
    tell(UNO_PORT.id, 'verifying', 3072);
    await expect.poll(() => bar.getAttribute('aria-valuenow')).toBe('100');
    expect(screen.getByRole('status')).toHaveTextContent('Checking… 100 %');

    finish();
    expect(await screen.findByText('The Arduino Uno runs the new program.')).toBeInTheDocument();
    expect(screen.getByText('2.9 KiB written and checked.')).toBeInTheDocument();
    // A plain .hex file isn't known to be FaberLink firmware.
    expect(screen.queryByRole('button', { name: 'Check for a FaberLink board' })).toBeNull();
  });

  it('names a port once when its label already ends in it', async () => {
    const mega = { ...UNO_PORT, id: 'serial:COM6', label: 'Arduino Mega 2560 (COM6)' };
    const shell = fakeShell([{ path: '/api/v0/flasher/boards', body: BOARDS }]);
    shell.pushPorts([mega]);
    render(FlasherWindow, { props: { win, visible: true }, context: shell.context });
    const option = await screen.findByRole('option', { name: /Arduino Mega 2560/ });
    expect(option.textContent).toMatch(/^Arduino Mega 2560 \(COM6\)/);
    expect(option.textContent).not.toContain('serial:COM6');
  });

  it('asks for the board on a clone', async () => {
    await setup();
    await userEvent.selectOptions(screen.getByLabelText('Port'), CLONE_PORT.id);
    expect(screen.getByLabelText('Board')).toHaveValue('');
    expect(screen.getByText(/Clone boards use this USB chip/)).toBeInTheDocument();
    expect(
      screen.getByRole('option', { name: /COM7.*CH340 USB chip: pick the board/ }),
    ).toBeInTheDocument();
    await chooseFile();
    expect(screen.getByRole('button', { name: 'Flash' })).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText('Board'), 'nano');
    expect(screen.getByRole('button', { name: 'Flash' })).toBeEnabled();
    // Picked by hand: switching ports keeps it.
    await userEvent.selectOptions(screen.getByLabelText('Port'), UNO_PORT.id);
    expect(screen.getByLabelText('Board')).toHaveValue('nano');
  });

  it("flashes a package's firmware and offers to check for a FaberLink board", async () => {
    const shell = await setup([
      { method: 'POST', path: FLASH_URL(UNO_PORT.id), body: RESULT },
      {
        method: 'POST',
        path: '/api/v0/ports/check-board',
        body: {
          result: 'found',
          board: 'motor-and-distance',
          firmware: 'faberlink-arduino',
          firmwareVersion: '0.1.0',
          parts: 2,
        },
      },
    ]);
    shell.pushPackages([PACKAGE, { ...PACKAGE, id: 'mega-only', firmware: ['mega2560'] }]);
    await userEvent.click(screen.getByRole('radio', { name: 'Firmware from a package' }));
    // The only package with firmware for an Uno is picked by itself.
    expect(screen.getByLabelText('Package')).toHaveValue('my-bridge');
    expect(screen.queryByRole('option', { name: /mega-only/ })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Flash' }));
    const ask = screen.getByRole('alertdialog', { name: 'Replace the program on the board?' });
    expect(ask).toHaveTextContent('my-bridge: firmware/uno.hex');
    await userEvent.click(within(ask).getByRole('button', { name: 'Replace and flash' }));
    expect(flashCalls(shell)[0]!.body).toEqual({
      board: 'uno',
      firmware: { kind: 'package', package: 'my-bridge' },
    });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Check for a FaberLink board' }),
    );
    expect(await screen.findByText(/motor-and-distance/)).toBeInTheDocument();
    expect(shell.fetch.calls.at(-1)).toMatchObject({ body: { port: UNO_PORT.id } });

    // The Mega has no firmware in any package but mega-only.
    await userEvent.click(screen.getByRole('button', { name: 'Flash again' }));
    await userEvent.selectOptions(screen.getByLabelText('Board'), 'nano');
    expect(
      screen.getByText('No installed package has firmware for the Arduino Nano.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Flash' })).toBeDisabled();
  });

  it('stops a FaberLink board on the port only when asked again', async () => {
    const shell = await setup();
    shell.fetch.route({
      method: 'POST',
      path: FLASH_URL(UNO_PORT.id),
      status: 409,
      body: {
        error: {
          code: 'port-used-by-bridge',
          message: 'Arduino Uno (COM4) is used by the FaberLink board "mega".',
          hint: 'Flashing stops "mega" (its motors stop) and it starts again on the new firmware.',
        },
      },
    });
    await chooseFile();
    await userEvent.click(screen.getByRole('button', { name: 'Flash' }));
    await userEvent.click(screen.getByRole('button', { name: 'Replace and flash' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('is used by the FaberLink board "mega"');

    shell.fetch.route({
      method: 'POST',
      path: FLASH_URL(UNO_PORT.id),
      body: { ...RESULT, stoppedPart: 'mega' },
    });
    await userEvent.click(within(alert).getByRole('button', { name: 'Stop the board and flash' }));
    expect(flashCalls(shell).at(-1)!.body).toMatchObject({ stopPart: true });
    expect(
      await screen.findByText('“mega” starts again by itself, on the new firmware.'),
    ).toBeInTheDocument();
  });

  it('shows why flashing failed, with what to try', async () => {
    const shell = await setup();
    shell.fetch.route({
      method: 'POST',
      path: FLASH_URL(UNO_PORT.id),
      status: 422,
      body: {
        error: {
          code: 'board-not-answering',
          message: "The board didn't answer the bootloader.",
          hint: 'Try pressing reset just as you press Flash.',
        },
      },
    });
    await chooseFile();
    await userEvent.click(screen.getByRole('button', { name: 'Flash' }));
    await userEvent.click(screen.getByRole('button', { name: 'Replace and flash' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent("The board didn't answer the bootloader.");
    expect(alert).toHaveTextContent('Try pressing reset just as you press Flash.');
    expect(within(alert).queryByRole('button')).toBeNull();
    // Ready to try again.
    expect(screen.getByRole('button', { name: 'Flash' })).toBeEnabled();
  });
});
