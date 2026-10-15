import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, part, simBase, simDistance } from '../test/fakeShell';
import WorldWindow from './WorldWindow.svelte';

const APARTMENT = {
  name: 'apartment',
  title: 'Apartment',
  description: 'A two-room flat.',
  start: { x: 1, y: 1, yaw: 0 },
  geometry: {
    bounds: { min: [0, 0], max: [8, 6] },
    open: false,
    walls: [{ a: [0, 0], b: [8, 0] }],
    shapes: [],
    overhead: [],
    movers: [],
  },
  footprint: { length: 0.8, width: 0.65 },
};

const WORLDS = [
  { name: 'empty', title: 'Empty floor', description: 'Nothing in the way.', size: [10, 10] },
  { name: 'apartment', title: 'Apartment', description: 'A two-room flat.', size: [8, 6] },
  { name: 'warehouse', title: 'Warehouse', description: 'Long racks.', size: [30, 20] },
];

const mount = { x: 0.3, y: 0, z: 0.2, roll: 0, pitch: 0, yaw: 0 };

function setup(parts = [simBase('base', { simulate: 'world' })]) {
  const shell = fakeShell([
    { path: '/api/v0/world', body: APARTMENT },
    { path: '/api/v0/worlds', body: WORLDS },
    { method: 'PUT', path: '/api/v0/world', body: { ...APARTMENT, name: 'warehouse' } },
    { method: 'POST', path: '/api/v0/world/reset', status: 204 },
  ]);
  const id = shell.desktop.open('world');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(WorldWindow, { props: { win, visible: true }, context: shell.context });
  shell.pushSettings();
  shell.pushParts(parts);
  const world = (over: object = {}) =>
    shell.receive({
      type: 'world',
      id: shell.subscriptionId('world'),
      pose: { x: 2, y: 1.5, yaw: 0 },
      velocity: { vx: 0.3, vy: 0, omega: 0 },
      bumped: false,
      movers: [],
      tUs: 1_000,
      ...over,
    });
  return { shell, world };
}

describe('World window', () => {
  it('shows the current world and where the machine is', async () => {
    const { world } = setup();
    expect(await screen.findByRole('button', { name: /World: Apartment/ })).toBeInTheDocument();
    world();
    await waitFor(() =>
      expect(screen.getByTestId('world-position')).toHaveTextContent('2 m, 1.5 m'),
    );
    expect(screen.getByText('0.3 m/s')).toBeInTheDocument();
  });

  it('shows distances in the units setting', async () => {
    const { shell, world } = setup();
    shell.pushSettings({ ...DEFAULT_SETTINGS, units: { length: 'imperial' } });
    await screen.findByRole('button', { name: /World: Apartment/ });
    world({ pose: { x: 0.3048, y: 0.6096, yaw: 0 } });
    await waitFor(() =>
      expect(screen.getByTestId('world-position')).toHaveTextContent('1 ft, 2 ft'),
    );
  });

  it('says clearly while the machine is bumped', async () => {
    const { world } = setup();
    await screen.findByRole('button', { name: /World: Apartment/ });
    world({ bumped: true });
    expect(await screen.findByTestId('world-bumped')).toHaveTextContent('Bumped');
    world({ bumped: false, tUs: 2_000 });
    await waitFor(() => expect(screen.queryByTestId('world-bumped')).not.toBeInTheDocument());
  });

  it('watches the world and every world sensor, but not the hand-set ones', async () => {
    const { shell } = setup([
      simBase('base', { simulate: 'world' }),
      simDistance('front', { simulate: 'world', mount }),
      simDistance('hand'),
      part('lidar', { port: undefined, simulate: 'world', mount }),
    ]);
    await screen.findByRole('button', { name: /World: Apartment/ });
    const topics = shell.sent().flatMap((m) => (m.type === 'subscribe' ? [m.topic!] : []));
    expect(topics).toContainEqual({ kind: 'world' });
    expect(topics).toContainEqual({ kind: 'signal', address: 'front/distance', maxHz: 10 });
    expect(topics).toContainEqual({ kind: 'signal', address: 'lidar/laser-scan', maxHz: 10 });
    expect(topics.some((t) => t.address === 'hand/distance')).toBe(false);
  });

  it('switches the world only after asking, saying the machine goes back to the start', async () => {
    const { shell } = setup();
    await userEvent.click(await screen.findByRole('button', { name: /World: Apartment/ }));
    const list = screen.getByRole('listbox', { name: 'World' });
    const warehouse = within(list).getByRole('option', { name: /Warehouse/ });
    expect(warehouse).toHaveTextContent('30 m × 20 m');
    expect(warehouse).toHaveTextContent('Long racks.');
    expect(within(list).getByRole('option', { name: /Apartment/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    await userEvent.click(warehouse);
    const ask = screen.getByRole('alertdialog', { name: 'Switch to Warehouse?' });
    expect(ask).toHaveTextContent('The machine goes back to the start.');
    await userEvent.click(within(ask).getByRole('button', { name: 'Cancel' }));
    expect(shell.fetch.calls.some((c) => c.method === 'PUT')).toBe(false);

    await userEvent.click(screen.getByRole('button', { name: /World: Apartment/ }));
    await userEvent.click(screen.getByRole('option', { name: /Warehouse/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Switch' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.method === 'PUT')?.body).toEqual({
        name: 'warehouse',
      }),
    );
  });

  it('reads the geometry again when the world is switched elsewhere', async () => {
    const { shell } = setup();
    await screen.findByRole('button', { name: /World: Apartment/ });
    const before = shell.fetch.calls.filter((c) => c.path === '/api/v0/world').length;
    shell.receive({ type: 'world-changed', id: shell.subscriptionId('world'), name: 'maze' });
    await waitFor(() =>
      expect(shell.fetch.calls.filter((c) => c.path === '/api/v0/world').length).toBe(before + 1),
    );
  });

  it('forgets what the sensors saw in the old world when the world changes', async () => {
    const { shell, world } = setup([
      simBase('base', { simulate: 'world' }),
      part('lidar', { port: undefined, simulate: 'world', mount }),
    ]);
    await screen.findByRole('button', { name: /World: Apartment/ });
    world();
    shell.pushReading('lidar/laser-scan', {
      capability: 'laser-scan@1',
      reading: { tUs: 1_000, angleMin: 0, angleIncrement: 1, scanTime: 0.1, ranges: [1, 2, 3] },
    });
    const canvas = screen.getByTestId('world-canvas');
    await waitFor(() => expect(canvas.dataset.scanPoints).toBe('3'));
    await waitFor(() => expect(screen.getByTestId('world-position')).toHaveTextContent('2 m'));
    shell.receive({ type: 'world-changed', id: shell.subscriptionId('world'), name: 'maze' });
    // Nor where the machine stood there.
    await waitFor(() => expect(screen.getByTestId('world-position')).toHaveTextContent('–'));
    world({ tUs: 2_000 });
    await waitFor(() => expect(canvas.dataset.scanPoints).toBe('0'));
  });

  it('puts the machine back at the start', async () => {
    const { shell } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Reset position' }));
    await waitFor(() =>
      expect(
        shell.fetch.calls.some((c) => c.method === 'POST' && c.path === '/api/v0/world/reset'),
      ).toBe(true),
    );
  });

  it('opens Drive on the world base', async () => {
    const { shell } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Drive' }));
    const drive = shell.desktop.state.windows.find((w) => w.kind === 'drive');
    expect(drive?.params).toEqual({ part: 'base' });
  });

  it('turns following off when dragged, and Fit the world shows all of it', async () => {
    setup();
    const follow = await screen.findByRole('switch', { name: 'Follow the machine' });
    expect(follow).toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Fit the world' }));
    expect(follow).not.toBeChecked();
    await userEvent.click(follow);
    expect(follow).toBeChecked();
  });

  it('explains how to add a machine when nothing is in the world yet', async () => {
    const { shell } = setup([simBase('manual-base')]);
    const empty = await screen.findByTestId('world-empty');
    expect(empty).toHaveTextContent('No simulated machine yet');
    expect(screen.queryByRole('button', { name: 'Drive' })).not.toBeInTheDocument();
    await userEvent.click(within(empty).getByRole('button', { name: 'Add a simulated base' }));
    const parts = shell.desktop.state.windows.find((w) => w.kind === 'parts');
    expect(parts?.params).toEqual({ addSimulate: 'world', addConnector: 'generic-planar-base' });
  });
});
