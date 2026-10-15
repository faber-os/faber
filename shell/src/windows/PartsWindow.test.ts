import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import {
  GENERIC_BASE,
  GENERIC_DISTANCE,
  LD19,
  SAMPLE_PORT,
  USB_PORT,
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
    expect(screen.getByText(/needs a FaberLink bridge to be real/)).toBeInTheDocument();

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
});
