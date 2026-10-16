import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import {
  BRIDGE,
  GENERIC_BASE,
  GENERIC_DISTANCE,
  LD19,
  MEGA_PORT,
  SAMPLE_PORT,
  USB_PORT,
  boardMotor,
  bridgePart,
  fakeShell,
  part,
  simBase,
  simDistance,
} from '../test/fakeShell';
import PartsWindow from './PartsWindow.svelte';

function setup(routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  const win = shell.desktop.state.windows[0] ?? {
    id: 'w0',
    kind: 'parts',
    key: 'parts',
    title: 'Parts',
    rect: { x: 0, y: 0, w: 600, h: 400 },
    chosen: { x: 0, y: 0, w: 600, h: 400 },
    maximised: false,
    minimised: false,
    z: 1,
    params: {},
  };
  render(PartsWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

describe('Parts window', () => {
  it('says it is loading until the parts topic answers', () => {
    setup();
    expect(screen.getByText('Loading parts…')).toBeInTheDocument();
  });

  it('shows an honest empty state that points to the sample recordings', async () => {
    const shell = setup();
    shell.pushParts([]);
    expect(await screen.findByText('No parts yet')).toBeInTheDocument();
    expect(screen.getByText(/Faber finds it/)).toBeInTheDocument();
    expect(screen.queryByText(/replay:/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Try a sample recording' }));
    const files = shell.desktop.state.windows.find((w) => w.kind === 'files');
    expect(files?.params).toEqual({ path: 'home:/Recordings' });
  });

  it('lists parts with live state badges, messages and hints', async () => {
    const shell = setup();
    shell.pushPorts([SAMPLE_PORT, USB_PORT]);
    shell.pushParts([
      part('demo'),
      part('lidar', {
        port: 'usb:10c4:ea60',
        state: {
          kind: 'failed',
          message: 'No data from the LD19 for 2 s.',
          hint: 'Check the TX wire and that the LiDAR has 5 V.',
        },
      }),
    ]);

    const demo = (await screen.findByText('demo')).closest('li')!;
    expect(within(demo).getByText('Running')).toBeInTheDocument();
    // The connector's name arrives with GET /connectors, a moment after the parts topic.
    expect(await within(demo).findByText('LD19 LiDAR')).toBeInTheDocument();
    expect(within(demo).getByText('replay:ld19-sample')).toBeInTheDocument();

    const lidar = screen.getByText('lidar').closest('li')!;
    expect(within(lidar).getByText('Failed')).toBeInTheDocument();
    expect(within(lidar).getByText('No data from the LD19 for 2 s.')).toBeInTheDocument();
    expect(
      within(lidar).getByText('Check the TX wire and that the LiDAR has 5 V.'),
    ).toBeInTheDocument();

    // A live update changes the badge without a reload.
    shell.pushParts([part('demo', { state: { kind: 'degraded' } })]);
    expect(await screen.findByText('Degraded')).toBeInTheDocument();
  });

  it('opens the viewer when a part is opened', async () => {
    const shell = setup();
    shell.pushParts([part('demo')]);
    await userEvent.click(await screen.findByRole('button', { name: 'demo' }));
    const viewer = shell.desktop.state.windows.find((w) => w.kind === 'laser-scan');
    expect(viewer?.params).toEqual({ address: 'demo/laser-scan', part: 'demo' });
  });

  it('removes a part after confirming', async () => {
    const shell = setup([{ method: 'DELETE', path: '/api/v0/parts/demo', status: 204 }]);
    shell.pushParts([part('demo')]);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove demo' }));
    expect(screen.getByText('Remove from parts.toml?')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'DELETE')).toBe(true));
  });

  it('shows the core error when removing fails', async () => {
    const error = {
      code: 'unknown-part',
      message: 'There is no part called "demo".',
      hint: 'Reload the list.',
    };
    const shell = setup([
      { method: 'DELETE', path: '/api/v0/parts/demo', status: 404, body: { error } },
    ]);
    shell.pushParts([part('demo')]);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove demo' }));
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(error.message)).toBeInTheDocument();
    expect(within(alert).getByText(error.hint)).toBeInTheDocument();
  });
});

describe('A FaberLink board in Parts', () => {
  it('shows the board, its link and firmware, with its parts indented beneath it', async () => {
    const shell = setup();
    shell.pushPorts([{ ...MEGA_PORT, claimedBy: 'mega' }]);
    shell.pushParts([
      bridgePart('mega'),
      boardMotor('mega', 'left'),
      part('mega.front', {
        connector: 'faber-bridge',
        port: MEGA_PORT.id,
        bridge: 'mega',
        signals: [{ address: 'mega.front/distance', capability: 'distance@1' }],
      }),
    ]);
    const mega = (await screen.findByText('Connected')).closest('li')!;
    expect(mega).toHaveAttribute('data-part', 'mega');
    expect(
      within(mega).getByText('sim-mega, firmware faberlink-harness 0.1.0'),
    ).toBeInTheDocument();
    expect(within(mega).getByText('FaberLink 1')).toBeInTheDocument();

    const left = document.querySelector<HTMLElement>('li[data-part="mega.left"]')!;
    expect(left).toHaveClass('sub');
    expect(left).toHaveAttribute('data-bridge', 'mega');
    expect(within(left).getByText('On the board mega')).toBeInTheDocument();
    expect(within(left).getByText('motor@1')).toBeInTheDocument();
    // Drive works on a board's motor as on any part; only the board removes its parts.
    expect(within(left).getByRole('button', { name: 'Drive mega.left' })).toBeInTheDocument();
    expect(
      within(left).queryByRole('button', { name: 'Remove mega.left' }),
    ).not.toBeInTheDocument();
    expect(within(mega).getByRole('button', { name: 'Remove mega' })).toBeInTheDocument();

    const front = document.querySelector<HTMLElement>('li[data-part="mega.front"]')!;
    await userEvent.click(within(front).getByRole('button', { name: 'mega.front' }));
    const viewer = shell.desktop.state.windows.find((w) => w.kind === 'distance');
    expect(viewer?.params).toEqual({ address: 'mega.front/distance', part: 'mega.front' });
  });

  it('shows the pins a part on the board uses, when the board says', async () => {
    const shell = setup();
    shell.pushParts([
      bridgePart('mega'),
      boardMotor('mega', 'left', {
        pins: [
          { role: 'pwm', pin: 9, label: 'D9' },
          { role: 'dir', pin: 8, label: 'D8' },
        ],
      }),
      boardMotor('mega', 'right'),
    ]);
    const left = (await screen.findByText('PWM D9 · DIR D8')).closest('li')!;
    expect(left).toHaveAttribute('data-part', 'mega.left');
    expect(within(left).getByTitle('The board’s pins this part uses')).toHaveTextContent(
      'PWM D9 · DIR D8',
    );
    // A board with an older library doesn't say: nothing is shown, not "no pins".
    const right = document.querySelector<HTMLElement>('li[data-part="mega.right"]')!;
    expect(within(right).queryByTitle('The board’s pins this part uses')).not.toBeInTheDocument();
  });

  it('shows a latched stop button on the board, and what to do', async () => {
    const shell = setup();
    shell.pushParts([bridgePart('mega', { hardwareEstop: true }), boardMotor('mega', 'left')]);
    const mega = (await screen.findByText('Stop button pressed')).closest('li')!;
    expect(
      within(mega).getByText('Reset it on the board, then release the e-stop.'),
    ).toBeInTheDocument();
  });

  it('says which code the board runs, from its build id', async () => {
    const shell = setup();
    const record = {
      id: 'usb-2341-0042-S1',
      board: 'mega2560',
      boardName: 'Arduino Mega 2560',
      sketch: 'home:/Arduino/Rover/Rover.ino',
      sketchName: 'Rover',
      buildId: '0123456789abcdef',
      sha256: 'ab'.repeat(32),
      bytes: 9000,
      flashedAt: '2026-10-16T10:00:00Z',
      identity: { serial: 'S1', port: MEGA_PORT.id, unique: true },
      checkMs: 1500,
    };
    shell.pushParts([bridgePart('mega', { code: { buildId: record.buildId, record } })]);
    expect(await screen.findByText('Code from Faber: Rover')).toBeInTheDocument();
    shell.pushParts([bridgePart('mega', { code: {} })]);
    expect(await screen.findByText('Code not flashed from Faber (unknown)')).toBeInTheDocument();
  });

  it('says when the board is out of reach', async () => {
    const shell = setup();
    shell.pushParts([
      bridgePart('mega', {
        link: 'lost',
      }),
      boardMotor('mega', 'left', {
        state: {
          kind: 'failed',
          message: 'Stopped: "mega" can\'t reach its board right now.',
          hint: 'It comes back by itself as soon as the board answers again.',
        },
      }),
    ]);
    expect(await screen.findByText('Board not reachable')).toBeInTheDocument();
    expect(
      screen.getByText('Stopped: "mega" can\'t reach its board right now.'),
    ).toBeInTheDocument();
  });

  it('can remove a part parts.toml names but the board does not have', async () => {
    const shell = setup([{ method: 'DELETE', path: '/api/v0/parts/mega.arm', status: 204 }]);
    shell.pushParts([
      bridgePart('mega'),
      boardMotor('mega', 'arm', {
        signals: [],
        state: { kind: 'degraded', message: 'Not on the board: "mega" has no part called "arm".' },
      }),
    ]);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove mega.arm' }));
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'DELETE' && c.path.endsWith('mega.arm')),
      ).toBe(true),
    );
  });
});

describe('Pausing a part', () => {
  it('needs an admin: an operator sees Pause and Resume disabled, with the reason', async () => {
    const shell = setup();
    shell.signInAs('operator');
    shell.pushParts([part('lidar')]);
    const row = (await screen.findByText('lidar')).closest('li')!;
    fireEvent.contextMenu(row);
    const pause = shell.menu.current!.entries.find((e) => e.kind === 'item' && e.id === 'pause');
    expect(pause).toMatchObject({ disabledReason: 'Needs the admin role' });
    shell.menu.close(false);

    shell.pushParts([part('lidar', { state: { kind: 'paused' } })]);
    const resume = await screen.findByRole('button', { name: 'Resume lidar' });
    expect(resume).toBeDisabled();
    expect(resume).toHaveAttribute('title', 'Needs the admin role');
  });

  it('pauses any part from its menu, and shows Paused with Resume', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/parts/lidar/pause', body: part('lidar') },
      { method: 'POST', path: '/api/v0/parts/lidar/resume', body: part('lidar') },
    ]);
    shell.pushParts([part('lidar')]);
    const row = (await screen.findByText('lidar')).closest('li')!;
    fireEvent.contextMenu(row);
    const items = shell.menu.current!.entries.flatMap((e) => (e.kind === 'item' ? [e] : []));
    expect(items.map((i) => i.id)).toContain('pause');
    shell.menu.choose('pause');
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'POST' && c.path.endsWith('/lidar/pause')),
      ).toBe(true),
    );

    // The core says it is paused: the row says so, offers Resume, and nothing to view or drive.
    shell.pushParts([part('lidar', { state: { kind: 'paused' } })]);
    const paused = await screen.findByText('Paused');
    const li = paused.closest('li')!;
    expect(within(li).getByText(/Its port is free for other programs/)).toBeInTheDocument();
    expect(within(li).queryByRole('button', { name: /^View/ })).not.toBeInTheDocument();
    fireEvent.contextMenu(li);
    const again = shell.menu.current!.entries.flatMap((e) => (e.kind === 'item' ? [e] : []));
    expect(again.map((i) => i.id)).toContain('resume');
    expect(again.map((i) => i.id)).not.toContain('pause');
    shell.menu.close(false);
    await userEvent.click(within(li).getByRole('button', { name: 'Resume lidar' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'POST' && c.path.endsWith('/lidar/resume')),
      ).toBe(true),
    );
  });

  it("pauses a board's parts with it: they show Paused, and only the board resumes", async () => {
    const shell = setup();
    shell.pushParts([
      part('mega', { connector: 'faber-bridge', port: MEGA_PORT.id, state: { kind: 'paused' } }),
      boardMotor('mega', 'left', { signals: [], state: { kind: 'paused' } }),
    ]);
    await screen.findAllByText('Paused');
    const left = document.querySelector<HTMLElement>('li[data-part="mega.left"]')!;
    expect(within(left).getByText('Paused')).toBeInTheDocument();
    expect(within(left).queryByRole('button', { name: /Resume/ })).not.toBeInTheDocument();
    expect(within(left).queryByRole('button', { name: /Drive/ })).not.toBeInTheDocument();
    const mega = document.querySelector<HTMLElement>('li[data-part="mega"]')!;
    expect(within(mega).getByRole('button', { name: 'Resume mega' })).toBeInTheDocument();
  });

  it('suggests pausing a board that has trouble, for an upload from the Arduino IDE', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/parts/mega/pause', body: bridgePart('mega') },
    ]);
    shell.pushParts([bridgePart('mega', { link: 'no-answer' })]);
    const hint = await screen.findByText(/Uploading from the Arduino IDE\? Pause this board first/);
    const li = hint.closest('li')!;
    await userEvent.click(within(li).getByRole('button', { name: 'Pause mega' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'POST' && c.path.endsWith('/mega/pause')),
      ).toBe(true),
    );
    // A board that works doesn't get the hint.
    shell.pushParts([bridgePart('mega')]);
    await waitFor(() => expect(screen.queryByText(/Uploading from the Arduino IDE/)).toBeNull());
  });

  it('says why the core refused to pause', async () => {
    const shell = setup([
      {
        method: 'POST',
        path: '/api/v0/parts/lidar/pause',
        status: 409,
        body: {
          error: { code: 'config-broken', message: 'parts.toml has a problem.', hint: 'Fix it.' },
        },
      },
    ]);
    shell.pushParts([part('lidar')]);
    const row = (await screen.findByText('lidar')).closest('li')!;
    fireEvent.contextMenu(row);
    shell.menu.choose('pause');
    expect(await screen.findByText('parts.toml has a problem.')).toBeInTheDocument();
  });
});

describe('Adding a FaberLink board', () => {
  async function openForm(check: unknown) {
    const shell = setup([
      { path: '/api/v0/connectors', body: [LD19, BRIDGE] },
      { method: 'POST', path: '/api/v0/ports/check-board', body: check },
    ]);
    shell.pushPorts([SAMPLE_PORT, USB_PORT, MEGA_PORT]);
    shell.pushParts([]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    await screen.findByRole('radio', { name: /faber-bridge/ });
    return shell;
  }

  const found = {
    result: 'found',
    board: 'sim-mega',
    firmware: 'faberlink-harness',
    firmwareVersion: '0.1.0',
    parts: 4,
  };

  it('checks a free port only when asked, then picks the board, its port and a name', async () => {
    const shell = await openForm(found);
    const checks = screen.getAllByRole('button', { name: 'Check for a FaberLink board' });
    expect(checks).toHaveLength(1); // not on a recording, and not on a port in use
    expect(screen.getByText(/only checks a port when you ask/)).toBeInTheDocument();
    expect(shell.fetch.calls.some((c) => c.path.endsWith('/check-board'))).toBe(false);

    await userEvent.click(checks[0]!);
    expect(
      await screen.findByText('FaberLink board: sim-mega (faberlink-harness 0.1.0)'),
    ).toBeInTheDocument();
    expect(shell.fetch.calls.find((c) => c.path.endsWith('/check-board'))?.body).toEqual({
      port: 'usb:2341:0042',
    });
    expect(screen.getByRole('radio', { name: /usb:2341:0042/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /faber-bridge/ })).toBeChecked();
    expect(screen.getByLabelText('Name')).toHaveValue('sim_mega');
    const simulated = screen.getByRole('switch', { name: /Simulated/ });
    expect(simulated).not.toBeChecked();
    expect(simulated).toBeDisabled();
    expect(screen.getByText(/is real hardware/)).toBeInTheDocument();
    expect(
      screen.getByText('Its motors and sensors appear as parts by themselves'),
    ).toBeInTheDocument();

    shell.fetch.route({
      method: 'POST',
      path: '/api/v0/parts',
      status: 201,
      body: bridgePart('sim_mega'),
    });
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.find((c) => c.path === '/api/v0/parts' && c.method === 'POST')?.body,
      ).toEqual({
        name: 'sim_mega',
        connector: 'faber-bridge',
        port: 'usb:2341:0042',
      }),
    );
  });

  it('says when nothing answered, and leaves the form as it was', async () => {
    await openForm({ result: 'nothing' });
    await userEvent.click(screen.getByRole('button', { name: 'Check for a FaberLink board' }));
    expect(
      await screen.findByText('No FaberLink board answered on this port.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /usb:2341:0042/ })).not.toBeChecked();
  });

  it('a board the core found is suggested on its port from then on', async () => {
    const shell = await openForm(found);
    shell.pushPorts([
      SAMPLE_PORT,
      {
        ...MEGA_PORT,
        suggestions: [
          {
            connector: 'faber-bridge',
            reason: 'A FaberLink board: sim-mega (faberlink-harness 0.1.0).',
          },
        ],
      },
    ]);
    expect(await screen.findByText('A FaberLink board answered here')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('radio', { name: /usb:2341:0042/ }));
    expect(screen.getByRole('radio', { name: /faber-bridge/ })).toBeChecked();
  });
});

describe('Add part flow', () => {
  async function openForm() {
    const shell = setup();
    shell.pushPorts([SAMPLE_PORT, USB_PORT]);
    shell.pushParts([]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    return shell;
  }

  it('lists ports with friendly suggestions and who is using them', async () => {
    await openForm();
    expect(screen.getByText('Often used for: LD19 LiDAR')).toBeInTheDocument();
    expect(screen.getByText('Used by the part "lidar"')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /usb:10c4:ea60/ })).toBeDisabled();
  });

  it('picks the suggested connector when a port is picked', async () => {
    await openForm();
    await userEvent.click(screen.getByRole('radio', { name: /replay:ld19-sample/ }));
    expect(screen.getByRole('radio', { name: /ldrobot-ld19/ })).toBeChecked();
  });

  it('says when installed connectors could not be loaded, and where to read why', async () => {
    const shell = setup();
    shell.fetch.route({
      path: '/api/v0/connectors/problems',
      body: [{ folder: 'broken', message: 'It broke.', hint: 'Fix it.' }],
    });
    shell.pushPorts([SAMPLE_PORT]);
    shell.pushParts([]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    expect(
      await screen.findByText(/One installed connector could not be loaded/),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    const settings = shell.desktop.state.windows.find((w) => w.kind === 'settings');
    expect(settings?.params).toEqual({ section: 'connectors' });
  });

  it('lets the connector problems be dismissed until another connector breaks', async () => {
    const shell = setup();
    const broken = { folder: 'broken', message: 'It broke.', hint: 'Fix it.' };
    shell.fetch.route({ path: '/api/v0/connectors/problems', body: [broken] });
    shell.pushPorts([SAMPLE_PORT]);
    shell.pushParts([]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    await screen.findByText(/One installed connector could not be loaded/);
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(/could not be loaded/)).not.toBeInTheDocument();

    // Loaded again (the form reopened): the same problem stays hidden, a new one shows.
    await shell.core.loadConnectors();
    expect(screen.queryByText(/could not be loaded/)).not.toBeInTheDocument();
    shell.fetch.route({
      path: '/api/v0/connectors/problems',
      body: [broken, { folder: 'also-broken', message: 'Also.', hint: 'Fix it too.' }],
    });
    await shell.core.loadConnectors();
    expect(
      await screen.findByText(/2 installed connectors could not be loaded/),
    ).toBeInTheDocument();
  });

  it('offers a connector installed since the form was last opened', async () => {
    const shell = setup();
    shell.pushPorts([SAMPLE_PORT]);
    shell.pushParts([]);
    const acme = { ...LD19, id: 'acme-ranger', name: 'Acme ranger', kind: 'wasm' };
    shell.fetch.route({ path: '/api/v0/connectors', body: [LD19, acme] });
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    expect(await screen.findByRole('radio', { name: /Acme ranger/ })).toBeInTheDocument();
  });

  it('explains an invalid name before sending anything', async () => {
    const shell = await openForm();
    await userEvent.click(screen.getByRole('radio', { name: /replay:ld19-sample/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'Front-Lidar');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    expect(screen.getByText(/"Front-Lidar" isn't a valid part name\./)).toBeInTheDocument();
    expect(screen.getByLabelText('Name')).toHaveAttribute('aria-invalid', 'true');
    expect(shell.fetch.calls.some((c) => c.method === 'POST')).toBe(false);
  });

  it('shows the API error message and hint when the core says no', async () => {
    const shell = await openForm();
    const error = {
      code: 'name-taken',
      message: 'There is already a part called "demo".',
      hint: 'Pick another name, or remove the old "demo" first.',
    };
    shell.fetch.route({ method: 'POST', path: '/api/v0/parts', status: 409, body: { error } });

    await userEvent.click(screen.getByRole('radio', { name: /replay:ld19-sample/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'demo');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));

    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(error.message)).toBeInTheDocument();
    expect(within(alert).getByText(error.hint)).toBeInTheDocument();
    expect(within(alert).getByText('name-taken')).toBeInTheDocument();
    expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toEqual({
      name: 'demo',
      connector: 'ldrobot-ld19',
      port: 'replay:ld19-sample',
    });
  });

  it('goes back to the list after adding', async () => {
    const shell = await openForm();
    shell.fetch.route({
      method: 'POST',
      path: '/api/v0/parts',
      status: 201,
      body: part('demo', { state: { kind: 'starting' } }),
    });
    await userEvent.click(screen.getByRole('radio', { name: /replay:ld19-sample/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'demo');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    shell.pushParts([part('demo', { state: { kind: 'starting' } })]);
    expect(await screen.findByText('Starting')).toBeInTheDocument();
    expect(screen.queryByRole('form', { name: 'Add part' })).not.toBeInTheDocument();
  });
});

describe('Adding a simulated part', () => {
  async function openForm() {
    const shell = setup([{ path: '/api/v0/connectors', body: [LD19, GENERIC_BASE] }]);
    shell.pushPorts([SAMPLE_PORT]);
    shell.pushParts([]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add the first part$/ }));
    return shell;
  }
  const simulated = () => screen.getByRole('switch', { name: /Simulated/ });
  const posted = (shell: ReturnType<typeof fakeShell>) =>
    shell.fetch.calls.find((c) => c.method === 'POST')?.body;

  it('needs no port when simulated, and sends where its values come from', async () => {
    const shell = await openForm();
    expect(simulated()).not.toBeChecked();
    await userEvent.click(simulated());
    expect(screen.queryByRole('radio', { name: /replay:ld19-sample/ })).not.toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Manual/ })).toBeChecked();
    await userEvent.click(screen.getByRole('radio', { name: /In the world/ }));
    await userEvent.click(screen.getByRole('radio', { name: /ldrobot-ld19/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'lidar');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    await waitFor(() =>
      expect(posted(shell)).toEqual({
        name: 'lidar',
        connector: 'ldrobot-ld19',
        simulate: 'world',
      }),
    );
  });

  it('locks a generic part to simulated, says why, and sends its settings', async () => {
    const shell = await openForm();
    await userEvent.click(screen.getByRole('radio', { name: /generic-planar-base/ }));
    expect(simulated()).toBeChecked();
    expect(simulated()).toBeDisabled();
    expect(screen.getByText(/sits on a FaberLink board/)).toBeInTheDocument();

    expect(screen.getByLabelText(/Length/)).toHaveValue(0.8);
    expect(screen.getByLabelText(/Width/)).toHaveValue(0.65);
    expect(screen.getByLabelText(/Top speed forward/)).toHaveValue(0.6);
    expect(screen.getByLabelText(/Top turning speed/)).toHaveValue(1.5);
    await userEvent.clear(screen.getByLabelText(/Top speed forward/));
    await userEvent.type(screen.getByLabelText(/Top speed forward/), '0.4');
    await userEvent.click(screen.getByLabelText(/Can drive sideways/));

    await userEvent.type(screen.getByLabelText('Name'), 'base');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    await waitFor(() =>
      expect(posted(shell)).toEqual({
        name: 'base',
        connector: 'generic-planar-base',
        simulate: 'manual',
        settings: {
          length: 0.8,
          width: 0.65,
          holonomic: false,
          'max-vx': 0.4,
          'max-vy': 0.6,
          'max-omega': 1.5,
          'max-accel': 1,
        },
      }),
    );
  });

  it("builds the form from the core's description: help, choices and advanced settings", async () => {
    const shell = setup([
      { path: '/api/v0/connectors', body: [LD19, GENERIC_BASE, GENERIC_DISTANCE] },
    ]);
    shell.pushPorts([]);
    shell.pushParts([simBase('left'), simBase('right')]);
    await userEvent.click(await screen.findByRole('button', { name: /^Add part$/ }));
    await userEvent.click(screen.getByRole('radio', { name: /generic-distance/ }));
    expect(screen.getByLabelText(/Farthest it can measure/)).toHaveAccessibleDescription(
      'About Farthest it can measure.',
    );
    await userEvent.selectOptions(screen.getByLabelText(/Kind of sensor/), 'Laser time-of-flight');
    expect(screen.getByLabelText(/Kind of sensor/)).toHaveValue('laser-tof');

    await userEvent.click(screen.getByRole('radio', { name: /generic-planar-base/ }));
    // Advanced settings wait behind their own heading.
    expect(screen.getByLabelText(/Rate/)).not.toBeVisible();
    await userEvent.click(screen.getByText('Advanced settings'));
    await userEvent.clear(screen.getByLabelText(/Rate/));
    await userEvent.type(screen.getByLabelText(/Rate/), '50');
    const motors = screen.getByRole('group', { name: 'Built from motors' });
    await userEvent.click(within(motors).getByRole('checkbox', { name: 'left' }));
    await userEvent.type(screen.getByLabelText('Name'), 'base');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toMatchObject({
        settings: { 'rate-hz': 50, 'built-from': ['left'] },
      }),
    );
  });

  it('explains a setting that is not a number before sending anything', async () => {
    const shell = await openForm();
    await userEvent.click(screen.getByRole('radio', { name: /generic-planar-base/ }));
    await userEvent.clear(screen.getByLabelText(/Width/));
    await userEvent.type(screen.getByLabelText('Name'), 'base');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    expect(screen.getByText('Width must be a number.')).toBeInTheDocument();
    expect(screen.getByLabelText(/Width/)).toHaveAttribute('aria-invalid', 'true');
    expect(posted(shell)).toBeUndefined();
  });

  it.each([
    ['needs-bridge', 'Tick Simulated for now.'],
    ['port-and-simulate', 'Give a port or simulate, not both.'],
    ['invalid-settings', 'max-vx must be above 0.'],
    ['world-has-a-base', 'Simulate this one by hand, or remove the other base.'],
  ])('shows the core error %s as it comes', async (code, hint) => {
    const shell = await openForm();
    const error = { code, message: `The core said ${code}.`, hint };
    shell.fetch.route({ method: 'POST', path: '/api/v0/parts', status: 400, body: { error } });
    await userEvent.click(screen.getByRole('radio', { name: /generic-planar-base/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'base');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(error.message)).toBeInTheDocument();
    expect(within(alert).getByText(hint)).toBeInTheDocument();
    expect(within(alert).getByText(code)).toBeInTheDocument();
  });

  it('keeps a real part on its port when Simulated is off', async () => {
    const shell = await openForm();
    await userEvent.click(simulated());
    await userEvent.click(simulated());
    await userEvent.click(screen.getByRole('radio', { name: /replay:ld19-sample/ }));
    await userEvent.type(screen.getByLabelText('Name'), 'lidar');
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    await waitFor(() =>
      expect(posted(shell)).toEqual({
        name: 'lidar',
        connector: 'ldrobot-ld19',
        port: 'replay:ld19-sample',
      }),
    );
  });
});

describe('Simulated parts in the list', () => {
  function withSim() {
    const shell = setup([
      { path: '/api/v0/connectors', body: [LD19, GENERIC_BASE, GENERIC_DISTANCE] },
      {
        path: '/api/v0/parts/front/simulation',
        body: { pattern: { kind: 'steady', value: 1 } },
      },
      {
        method: 'PUT',
        path: '/api/v0/parts/front/simulation',
        body: { pattern: { kind: 'steady', value: 1 } },
      },
    ]);
    shell.pushParts([simBase('base'), simDistance('front'), part('lidar')]);
    return shell;
  }
  const row = async (name: string) => (await screen.findByText(name)).closest('li')!;

  it('marks simulated parts', async () => {
    withSim();
    expect(within(await row('base')).getByText('Simulated')).toBeInTheDocument();
    expect(within(await row('front')).getByText('Simulated')).toBeInTheDocument();
    expect(within(await row('lidar')).queryByText('Simulated')).not.toBeInTheDocument();
  });

  it('sets a manual sensor by hand with a slider and a pattern', async () => {
    const shell = withSim();
    const slider = await within(await row('front')).findByRole('slider', {
      name: 'Value of front',
    });
    await waitFor(() => expect(slider).toHaveValue('1'));
    await fireEvent.input(slider, { target: { value: '2.5' } });
    await waitFor(() =>
      expect(shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body).toEqual({
        pattern: { kind: 'steady', value: 2.5 },
      }),
    );
    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Pattern of front' }),
      'Wave',
    );
    await waitFor(() =>
      expect(shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body).toEqual({
        pattern: { kind: 'sine', center: 2.5, amplitude: 0.4, periodS: 4 },
      }),
    );
    // Only manual sensors have one: not the base (it is driven) nor the real LiDAR.
    expect(within(await row('base')).queryByRole('slider')).not.toBeInTheDocument();
    expect(within(await row('lidar')).queryByRole('slider')).not.toBeInTheDocument();
  });

  it('says who drives an actuator, and opens the Drive window for it', async () => {
    const shell = withSim();
    expect(within(await row('base')).getByText('Nobody is driving it')).toBeInTheDocument();
    shell.pushLeases([{ part: 'base', holder: 'drive-square.py' }]);
    expect(await within(await row('base')).findByText('Driven by drive-square.py')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Drive base' }));
    expect(shell.desktop.state.windows.find((w) => w.kind === 'drive')?.params).toEqual({
      part: 'base',
    });
  });

  it('opens the distance and drive base viewers', async () => {
    const shell = withSim();
    await userEvent.click(
      await screen.findByRole('button', { name: 'Open the viewer for front/distance' }),
    );
    await userEvent.click(
      screen.getByRole('button', { name: 'Open the viewer for base/planar-base' }),
    );
    const kinds = shell.desktop.state.windows.map((w) => [w.kind, w.title]);
    expect(kinds).toContainEqual(['distance', 'front · distance']);
    expect(kinds).toContainEqual(['planar-base', 'base · base']);
  });
});

describe('Parts menus and recordings', () => {
  it('has a right-click menu that says why the port cannot open in the serial monitor', async () => {
    const shell = setup();
    shell.pushPorts([{ ...SAMPLE_PORT, claimedBy: 'lidar' }]);
    shell.pushParts([part('lidar')]);
    const row = (await screen.findByText('lidar')).closest('li')!;
    await fireEvent.contextMenu(row, { clientX: 20, clientY: 20 });
    const menu = shell.menu.current!;
    const items = menu.entries.flatMap((e) => (e.kind === 'item' ? [e] : []));
    expect(items.map((i) => i.label)).toEqual([
      'Open viewer',
      'Open recording port in serial monitor',
      'Pause',
      'Remove',
    ]);
    expect(items[1]!.disabledReason).toContain('"lidar" is using this port');
    shell.menu.choose('view');
    expect(shell.desktop.state.windows.some((w) => w.kind === 'laser-scan')).toBe(true);
  });

  it('starts the add form on a recording when asked to', async () => {
    const shell = fakeShell();
    const id = shell.desktop.open('parts');
    shell.desktop.update(id, { params: { addPort: 'replay:ld19-sample' } });
    const win = shell.desktop.state.windows[0]!;
    render(PartsWindow, { props: { win, visible: true }, context: shell.context });
    shell.pushPorts([SAMPLE_PORT]);
    shell.pushParts([]);
    const form = await screen.findByRole('form', { name: 'Add part' });
    await waitFor(() =>
      expect(within(form).getByRole('radio', { name: /replay:ld19-sample/ })).toBeChecked(),
    );
    expect(within(form).getByRole('radio', { name: /ldrobot-ld19/ })).toBeChecked();
  });
});

describe('Adding a part from another window', () => {
  it('starts the form simulated in the world, on the drive base, when asked to', async () => {
    const shell = fakeShell([{ path: '/api/v0/connectors', body: [LD19, GENERIC_BASE] }]);
    const id = shell.desktop.open('parts');
    shell.desktop.update(id, {
      params: { addSimulate: 'world', addConnector: 'generic-planar-base' },
    });
    const win = shell.desktop.state.windows[0]!;
    render(PartsWindow, { props: { win, visible: true }, context: shell.context });
    shell.pushPorts([]);
    shell.pushParts([]);
    const form = await screen.findByRole('form', { name: 'Add part' });
    expect(within(form).getByRole('switch', { name: /Simulated/ })).toBeChecked();
    expect(within(form).getByRole('radio', { name: /In the world/ })).toBeChecked();
    await waitFor(() =>
      expect(within(form).getByRole('radio', { name: /generic-planar-base/ })).toBeChecked(),
    );
    expect(within(form).getByLabelText(/Length/)).toHaveValue(0.8);
    expect(shell.desktop.state.windows[0]!.params).toEqual({});
  });
});

describe('Adding an I²C part from the scanner', () => {
  it('starts on the bus, the connector and the address the scan found, and sends the address', async () => {
    const tmp102 = {
      id: 'ti-tmp102',
      name: 'TI TMP102 temperature sensor',
      version: '1.0.0',
      kind: 'declarative',
      port: { kind: 'i2c' as const, addresses: [0x48, 0x49], default: 0x48 },
      provides: [{ name: 'temperature', capability: 'temperature@1' as const }],
      settings: [],
    };
    const shell = fakeShell([{ path: '/api/v0/connectors', body: [LD19, tmp102] }]);
    const id = shell.desktop.open('parts');
    shell.desktop.update(id, {
      params: { addPort: 'i2c-1', addConnector: 'ti-tmp102', addAddress: '73' },
    });
    const win = shell.desktop.state.windows[0]!;
    render(PartsWindow, { props: { win, visible: true }, context: shell.context });
    shell.pushPorts([{ id: 'i2c-1', kind: 'i2c', label: 'I²C bus 1', suggestions: [] }]);
    shell.pushParts([]);
    const form = await screen.findByRole('form', { name: 'Add part' });
    await waitFor(() => expect(within(form).getByRole('radio', { name: /i2c-1/ })).toBeChecked());
    await waitFor(() =>
      expect(within(form).getByRole('radio', { name: /ti-tmp102/ })).toBeChecked(),
    );
    expect(within(form).getByText('On an I²C bus')).toBeInTheDocument();
    expect(within(form).getByRole('radio', { name: '0x49' })).toBeChecked();
    shell.fetch.route({
      method: 'POST',
      path: '/api/v0/parts',
      status: 201,
      body: part('cabin_temp', { connector: 'ti-tmp102', port: 'i2c-1', address: 0x49 }),
    });
    await userEvent.type(within(form).getByLabelText('Name'), 'cabin_temp');
    await userEvent.click(within(form).getByRole('button', { name: 'Add part' }));
    const posted = shell.fetch.calls.find((c) => c.method === 'POST' && c.path === '/api/v0/parts');
    expect(posted?.body).toEqual({
      name: 'cabin_temp',
      connector: 'ti-tmp102',
      port: 'i2c-1',
      address: 0x49,
    });
  });
});

describe('Port problems', () => {
  it('explains hardware that is plugged in but never became a port, grouped by kind', async () => {
    const shell = setup();
    const problem = {
      code: 'usb-no-answer',
      label: "Plugged in, but the computer can't talk to it",
      detail: "Something is plugged in over USB, but it didn't answer.",
      hint: 'Try another USB cable; some only carry power.',
    };
    shell.receive({
      type: 'ports',
      id: shell.subscriptionId('ports'),
      ports: [],
      problems: [problem, problem],
    });
    shell.pushParts([]);
    const notice = await screen.findByText(problem.label);
    const box = notice.closest('[data-code]')!;
    expect(box).toHaveTextContent('(2 times)');
    // One line until asked: what happened and what to try are folded away.
    const more = box.querySelector('details')!;
    expect(more.open).toBe(false);
    await userEvent.click(within(box as HTMLElement).getByText('What to try'));
    expect(more.open).toBe(true);
    expect(within(more).getByText(problem.detail)).toBeInTheDocument();
    expect(within(more).getByText(problem.hint)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/device/i);

    // The add form shows it too, next to the empty port list.
    await userEvent.click(screen.getByRole('button', { name: /^Add the first part$/ }));
    const form = screen.getByRole('form', { name: 'Add part' });
    expect(within(form).getByText(problem.label)).toBeInTheDocument();

    // Gone when the core stops reporting it (the field is left out when there are none).
    shell.pushPorts([]);
    await waitFor(() => expect(screen.queryByText(problem.label)).not.toBeInTheDocument());
  });

  it('can be dismissed until a new problem comes along, everywhere it shows', async () => {
    const shell = setup();
    const problem = {
      code: 'usb-no-answer',
      label: "Plugged in, but the computer can't talk to it",
      detail: "Something is plugged in over USB, but it didn't answer.",
      hint: 'Try another USB cable; some only carry power.',
    };
    const other = {
      code: 'usb-no-driver',
      label: 'Plugged in, but it needs a driver',
      detail: 'Windows found no driver for it.',
      hint: 'Install the driver from the maker.',
    };
    const push = (problems: unknown[]) =>
      shell.receive({ type: 'ports', id: shell.subscriptionId('ports'), ports: [], problems });
    push([problem, problem]);
    shell.pushParts([]);
    await screen.findByText(problem.label);

    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(problem.label)).not.toBeInTheDocument();
    // The add form shares the dismissal.
    await userEvent.click(screen.getByRole('button', { name: /^Add the first part$/ }));
    const form = screen.getByRole('form', { name: 'Add part' });
    expect(within(form).queryByText(problem.label)).not.toBeInTheDocument();

    // The same problems again (or fewer of them) stay hidden; a new one brings the notice back.
    push([problem]);
    push([problem, problem]);
    expect(within(form).queryByText(problem.label)).not.toBeInTheDocument();
    // Each kind of problem is its own notice: another kind shows on its own…
    push([problem, problem, other]);
    expect(await within(form).findByText(other.label)).toBeInTheDocument();
    expect(within(form).queryByText(problem.label)).not.toBeInTheDocument();
    // …and one more of the dismissed kind brings that one back.
    push([problem, problem, problem, other]);
    expect(await within(form).findByText(problem.label)).toBeInTheDocument();
    expect(shell.desktop.dismissals).toEqual({
      'portProblems:usb-no-answer': ['usb-no-answer', 'usb-no-answer'],
    });
  });
});
