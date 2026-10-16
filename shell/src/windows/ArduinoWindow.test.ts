import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, listing } from '../test/fakeShell';
import type { FakeRoute } from '../test/fakeFetch';
import ArduinoWindow from './ArduinoWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';
import type {
  ArduinoInfo,
  BoardCodeInfo,
  CompileResult,
  FlashRecord,
  PortInfo,
} from '../lib/state/types';

const SKETCH = 'home:/Arduino/Rover/Rover.ino';
const q = (path: string) => `?path=${encodeURIComponent(path)}`;
const enc = (s: string) => new TextEncoder().encode(s);

function win(params: Record<string, string> = { path: SKETCH }): WindowState {
  return {
    id: 'w1',
    kind: 'arduino',
    key: 'arduino:home:/Arduino/Rover',
    title: 'Rover · Arduino',
    rect: { x: 0, y: 0, w: 880, h: 540 },
    chosen: { x: 0, y: 0, w: 880, h: 540 },
    maximised: false,
    minimised: false,
    z: 1,
    params,
  };
}

function info(state: ArduinoInfo['tools']['state'] = 'ready'): ArduinoInfo {
  return {
    tools: {
      state,
      installing: false,
      install: {
        cliVersion: '1.5.1',
        url: 'https://github.com/arduino/arduino-cli/releases/download/v1.5.1/arduino-cli_1.5.1_Linux_ARM64.tar.gz',
        core: 'arduino:avr',
        approxMb: 150,
      },
      ...(state === 'missing' ? {} : { cli: { path: '/x/arduino-cli', source: 'faber' as const } }),
    },
    boards: [
      { id: 'uno', name: 'Arduino Uno', chip: 'ATmega328P', fqbn: 'arduino:avr:uno', room: 32256 },
      {
        id: 'mega2560',
        name: 'Arduino Mega 2560',
        chip: 'ATmega2560',
        fqbn: 'arduino:avr:mega:cpu=atmega2560',
        room: 253952,
      },
    ],
    templates: [
      { id: 'blank', name: 'Empty sketch', description: 'setup() and loop().', faberlink: false },
      {
        id: 'faberlink-board',
        name: 'FaberLink board',
        description: 'A board Faber talks to.',
        faberlink: true,
      },
    ],
    sketchbook: 'home:/Arduino',
  };
}

const UNO_PORT: PortInfo = {
  id: 'usb:2341:0043',
  kind: 'usb-serial',
  label: 'Arduino Uno (COM4)',
  usb: { vid: 0x2341, pid: 0x0043 },
  suggestions: [],
  board: { board: 'uno' },
};

const sketchFiles: FakeRoute[] = [
  listing('home:/Arduino/Rover', [
    { name: 'motor.h' },
    { name: 'Rover.ino' },
    { name: 'notes.txt' },
  ]),
  { path: `/api/v0/files/content${q(SKETCH)}`, bytes: enc('void setup() {}\nvoid loop() {}\n') },
  {
    path: `/api/v0/files/content${q('home:/Arduino/Rover/motor.h')}`,
    bytes: enc('#pragma once\n\n#error not wired\n'),
  },
];

const COMPILED: CompileResult = {
  ok: true,
  board: 'uno',
  sketch: SKETCH,
  sketchName: 'Rover',
  buildId: '0123456789abcdef',
  size: { flash: 6896, flashMax: 32256, ram: 590, ramMax: 2048 },
  diagnostics: [],
};

const RECORD: FlashRecord = {
  id: 'usb-2341-0043-S1',
  board: 'uno',
  boardName: 'Arduino Uno',
  sketch: SKETCH,
  sketchName: 'Rover',
  buildId: '0123456789abcdef',
  sha256: 'ab'.repeat(32),
  bytes: 6896,
  flashedAt: '2026-10-16T10:00:00Z',
  identity: { serial: 'S1', port: UNO_PORT.id, unique: true },
  checkMs: 1100,
};

async function setup(routes: FakeRoute[] = [], params?: Record<string, string>, tools = info()) {
  const shell = fakeShell([{ path: '/api/v0/arduino', body: tools }, ...sketchFiles, ...routes]);
  shell.pushPorts([UNO_PORT]);
  render(ArduinoWindow, { props: { win: win(params), visible: true }, context: shell.context });
  return shell;
}

const calls = (shell: ReturnType<typeof fakeShell>, method: string, path: string) =>
  shell.fetch.calls.filter((c) => c.method === method && c.path === path);

describe('Arduino editor', () => {
  it('lets an operator verify but not upload, and a viewer only read', async () => {
    const shell = await setup();
    shell.signInAs('operator');
    await screen.findAllByRole('tab');
    const upload = screen.getByRole('button', { name: /Upload/ });
    await waitFor(() => expect(upload).toBeDisabled());
    expect(upload).toHaveAttribute('title', 'Needs the admin role');
    expect(screen.getByRole('button', { name: /Verify/ })).toBeEnabled();

    shell.signInAs('viewer');
    await waitFor(() => expect(screen.getByRole('button', { name: /Verify/ })).toBeDisabled());
    expect(screen.getByRole('button', { name: /^New$/ })).toBeDisabled();
  });

  it('opens a sketch with its code files as tabs, the main .ino first', async () => {
    await setup();
    const tabs = await screen.findAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(['Rover.ino', 'motor.h']);
    expect(tabs[0]).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('textbox', { name: 'Code of Rover.ino' })).toHaveTextContent(
      'void setup() {}',
    );
    expect(screen.getByLabelText('Board')).toHaveValue('uno');
    expect(screen.getByLabelText('Port')).toHaveValue(UNO_PORT.id);
  });

  it('verifies, with the size report', async () => {
    const shell = await setup([
      { method: 'POST', path: '/api/v0/arduino/compile', body: COMPILED },
    ]);
    await screen.findAllByRole('tab');
    await userEvent.click(screen.getByRole('button', { name: 'Verify' }));
    expect(await screen.findByText('Compiled for the Arduino Uno.')).toBeInTheDocument();
    expect(screen.getByText('Program: 6 896 bytes of 32 256 (21 %)')).toBeInTheDocument();
    expect(screen.getByText('Memory: 590 bytes of 2 048 (29 %)')).toBeInTheDocument();
    expect(calls(shell, 'POST', '/api/v0/arduino/compile')[0]!.body).toEqual({
      sketch: SKETCH,
      board: 'uno',
    });
  });

  it('lists compile errors with file and line, and a click opens that tab', async () => {
    await setup([
      {
        method: 'POST',
        path: '/api/v0/arduino/compile',
        body: {
          ...COMPILED,
          ok: false,
          size: undefined,
          diagnostics: [
            {
              severity: 'error',
              file: 'motor.h',
              inSketch: true,
              line: 3,
              column: 2,
              message: '#error not wired',
            },
            {
              severity: 'error',
              file: 'FaberLink.h',
              inSketch: false,
              line: 9,
              message: 'oops',
            },
          ],
        },
      },
    ]);
    await screen.findAllByRole('tab');
    await userEvent.click(screen.getByRole('button', { name: 'Verify' }));
    expect(await screen.findByText('The sketch didn’t compile: 2 errors.')).toBeInTheDocument();
    const list = screen.getByRole('list', { name: 'Errors and warnings' });
    expect(within(list).getByText('#error not wired')).toBeInTheDocument();
    // A library's file can't be opened here: it is named, not a link.
    expect(within(list).queryByRole('button', { name: /FaberLink/ })).toBeNull();
    await userEvent.click(within(list).getByRole('button', { name: 'Go to motor.h:3:2' }));
    expect(screen.getByRole('tab', { name: 'motor.h' })).toHaveAttribute('aria-selected', 'true');
  });

  it('uploads only after the confirm, and offers to stop a FaberLink board in the way', async () => {
    const error = {
      code: 'port-used-by-bridge',
      message: 'Arduino Uno (COM4) is used by the FaberLink board "rover".',
      hint: 'Flashing stops "rover" (its motors stop)…',
    };
    const shell = await setup([
      { method: 'POST', path: '/api/v0/arduino/upload', status: 409, body: { error } },
    ]);
    await screen.findAllByRole('tab');
    await userEvent.click(screen.getByRole('button', { name: 'Upload' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Replace the program on the board?' });
    expect(dialog).toHaveTextContent('Faber compiles Rover and puts it on the Arduino Uno');
    expect(calls(shell, 'POST', '/api/v0/arduino/upload')).toHaveLength(0);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Replace and upload' }));
    expect(await screen.findByText(error.message)).toBeInTheDocument();
    expect(calls(shell, 'POST', '/api/v0/arduino/upload')[0]!.body).toEqual({
      sketch: SKETCH,
      board: 'uno',
      port: UNO_PORT.id,
    });

    shell.fetch.route({
      method: 'POST',
      path: '/api/v0/arduino/upload',
      body: {
        compile: COMPILED,
        flashed: {
          port: UNO_PORT.id,
          board: 'uno',
          boardName: 'Arduino Uno',
          bytes: 6896,
          pages: 54,
          stoppedPart: 'rover',
        },
        record: RECORD,
      },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Stop the board and upload' }));
    expect(
      await screen.findByText('Uploaded: the Arduino Uno runs Rover now.'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('“rover” starts again by itself, on the new sketch.'),
    ).toBeInTheDocument();
    expect(calls(shell, 'POST', '/api/v0/arduino/upload')[1]!.body).toMatchObject({
      stopPart: true,
    });
  });

  it('proves the code on a board before opening it: read back only when asked', async () => {
    const code = (c: BoardCodeInfo['code']): BoardCodeInfo => ({
      port: UNO_PORT.id,
      identity: { serial: 'S1', port: UNO_PORT.id, unique: true },
      code: c,
    });
    const CODE = `/api/v0/arduino/boards/${encodeURIComponent(UNO_PORT.id)}`;
    const shell = await setup([
      { path: `${CODE}/code`, body: code({ result: 'needs-check', record: RECORD }) },
      {
        method: 'POST',
        path: `${CODE}/check`,
        body: code({
          result: 'same',
          record: RECORD,
          proof: { method: 'read-back', elapsedMs: 1200 },
        }),
      },
      {
        method: 'POST',
        path: `/api/v0/arduino/flashed/${RECORD.id}/open`,
        body: { path: SKETCH, copy: false },
      },
    ]);
    await screen.findAllByRole('tab');
    await userEvent.click(screen.getByRole('button', { name: 'On the board' }));
    const dialog = await screen.findByRole('alertdialog', { name: 'The code on the board' });
    expect(
      await within(dialog).findByText(/Faber flashed Rover on this board on/),
    ).toHaveTextContent('resets the board and takes about 1 s');
    expect(within(dialog).queryByRole('button', { name: 'Open it' })).toBeNull();
    expect(calls(shell, 'POST', `${CODE}/check`)).toHaveLength(0);

    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Check the code on this board' }),
    );
    expect(await within(dialog).findByText(/This board still runs Rover from/)).toBeInTheDocument();
    expect(
      within(dialog).getByText('Faber read the program back and compared it (1 s).'),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Open it' }));
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull());
    expect(calls(shell, 'POST', `/api/v0/arduino/flashed/${RECORD.id}/open`)[0]!.body).toEqual({});
  });

  it('offers other code only as a copy, and says when Faber never flashed a board', async () => {
    const CODE = `/api/v0/arduino/boards/${encodeURIComponent(UNO_PORT.id)}/code`;
    const shell = await setup([
      {
        path: CODE,
        body: {
          port: UNO_PORT.id,
          identity: { port: UNO_PORT.id, unique: false },
          code: { result: 'different', record: RECORD, proof: { method: 'build-id' } },
        },
      },
    ]);
    await screen.findAllByRole('tab');
    await userEvent.click(screen.getByRole('button', { name: 'On the board' }));
    const dialog = await screen.findByRole('alertdialog', { name: 'The code on the board' });
    expect(await within(dialog).findByText(/different code now/)).toHaveTextContent(
      'The last code Faber flashed is Rover.',
    );
    expect(within(dialog).getByText(/no serial number of its own/)).toBeInTheDocument();
    expect(
      within(dialog).getByRole('button', { name: 'Open it anyway as a copy' }),
    ).toBeInTheDocument();

    shell.fetch.route({
      path: CODE,
      body: {
        port: UNO_PORT.id,
        identity: { port: UNO_PORT.id, unique: true },
        code: { result: 'never' },
      },
    });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
    await userEvent.click(screen.getByRole('button', { name: 'On the board' }));
    expect(await screen.findByText('Faber has never flashed this board.')).toBeInTheDocument();
  });

  it('without the tools, offers to install them and shows what is downloaded', async () => {
    const shell = await setup(
      [{ method: 'POST', path: '/api/v0/arduino/tools/install', body: info().tools }],
      {},
      info('missing'),
    );
    expect(await screen.findByText('The Arduino tools aren’t installed')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Install Arduino tools' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Install the Arduino tools?' });
    expect(dialog).toHaveTextContent('about 150 MB');
    expect(dialog).toHaveTextContent('arduino-cli_1.5.1_Linux_ARM64.tar.gz');
    expect(dialog).toHaveTextContent('SHA-256');
    expect(calls(shell, 'POST', '/api/v0/arduino/tools/install')).toHaveLength(0);
    shell.fetch.route({ path: '/api/v0/arduino', body: info() });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Download and install' }));
    await waitFor(() =>
      expect(screen.queryByText('The Arduino tools aren’t installed')).toBeNull(),
    );
    expect(calls(shell, 'POST', '/api/v0/arduino/tools/install')).toHaveLength(1);
  });

  it('makes a new sketch from a template, checking its name first', async () => {
    const NEW = 'home:/Arduino/Blink/Blink.ino';
    const shell = await setup(
      [
        {
          method: 'POST',
          path: '/api/v0/arduino/sketches',
          status: 201,
          body: { path: NEW, copy: false },
        },
        listing('home:/Arduino/Blink', [{ name: 'Blink.ino' }]),
        { path: `/api/v0/files/content${q(NEW)}`, bytes: enc('// Blink\n') },
      ],
      {},
    );
    await userEvent.click(await screen.findByRole('button', { name: 'New sketch' }));
    const dialog = screen.getByRole('alertdialog', { name: 'New sketch' });
    await userEvent.type(within(dialog).getByLabelText('Name'), 'my blink');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create' }));
    expect(within(dialog).getByText(/can’t be in a sketch’s name/)).toBeInTheDocument();
    expect(calls(shell, 'POST', '/api/v0/arduino/sketches')).toHaveLength(0);

    await userEvent.clear(within(dialog).getByLabelText('Name'));
    await userEvent.type(within(dialog).getByLabelText('Name'), 'Blink');
    await userEvent.click(within(dialog).getByRole('radio', { name: /FaberLink board/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create' }));
    expect(await screen.findByRole('tab', { name: 'Blink.ino' })).toBeInTheDocument();
    expect(calls(shell, 'POST', '/api/v0/arduino/sketches')[0]!.body).toEqual({
      name: 'Blink',
      template: 'faberlink-board',
    });
  });
});
