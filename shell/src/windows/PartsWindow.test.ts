import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { SAMPLE_PORT, USB_PORT, fakeShell, part } from '../test/fakeShell';
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
