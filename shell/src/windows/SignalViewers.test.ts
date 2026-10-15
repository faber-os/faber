import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import {
  DEFAULT_SETTINGS,
  GENERIC_BASE,
  GENERIC_DISTANCE,
  fakeShell,
  simBase,
  simDistance,
} from '../test/fakeShell';
import type { WindowState } from '../lib/desktop/windows';
import DistanceWindow from './DistanceWindow.svelte';
import PlanarBaseWindow from './PlanarBaseWindow.svelte';

function win(kind: string, address: string, part: string): WindowState {
  return {
    id: 'w9',
    kind,
    key: address,
    title: part,
    rect: { x: 0, y: 0, w: 500, h: 400 },
    chosen: { x: 0, y: 0, w: 500, h: 400 },
    maximised: false,
    minimised: false,
    z: 1,
    params: { address, part },
  };
}

function setup(component: typeof DistanceWindow | typeof PlanarBaseWindow, w: WindowState) {
  const shell = fakeShell([{ path: '/api/v0/connectors', body: [GENERIC_BASE, GENERIC_DISTANCE] }]);
  render(component, { props: { win: w, visible: true }, context: shell.context });
  return shell;
}

describe('Distance viewer', () => {
  const w = win('distance', 'front/distance', 'front');

  it('subscribes to its signal and waits for the first reading', () => {
    const shell = setup(DistanceWindow, w);
    const sub = shell.sent().find((m) => m.topic?.kind === 'signal');
    expect(sub?.topic).toEqual({ kind: 'signal', address: 'front/distance', maxHz: 15 });
    expect(screen.getByText(/Waiting for the first reading/)).toBeInTheDocument();
  });

  it('shows the distance in the chosen units, on a bar against the range, with a plot', async () => {
    const shell = setup(DistanceWindow, w);
    shell.pushParts([simDistance('front')]);
    for (const [tUs, meters] of [
      [1_000_000, 1],
      [1_100_000, 1.2],
    ] as const) {
      shell.pushReading('front/distance', {
        capability: 'distance@1',
        reading: { tUs, meters, status: 'ok' },
      });
    }
    expect(await screen.findByTestId('distance-value')).toHaveTextContent('1.2 m');
    expect(screen.getByText('Measuring')).toBeInTheDocument();
    const bar = screen.getByRole('meter', { name: 'Distance' });
    expect(bar).toHaveAttribute('aria-valuenow', '1.2');
    expect(bar).toHaveAttribute('aria-valuemax', '4');
    expect(screen.getByText(/Range 2 cm to 4 m/)).toBeInTheDocument();
    expect(document.querySelectorAll('[data-testid="distance-plot"] polyline')).toHaveLength(1);

    shell.pushSettings({ ...DEFAULT_SETTINGS, units: { length: 'imperial' } });
    expect(await screen.findByTestId('distance-value')).toHaveTextContent('3.94 ft');
  });

  it('says why a reading is not a measurement', async () => {
    const shell = setup(DistanceWindow, w);
    shell.pushReading('front/distance', {
      capability: 'distance@1',
      reading: { tUs: 1, meters: 4, status: 'too-far' },
    });
    expect(await screen.findByText('Too far: nothing within its range')).toBeInTheDocument();
  });
});

describe('Drive base viewer', () => {
  const w = win('planar-base', 'base/planar-base', 'base');

  it('shows commanded and measured velocity, and when it bumped', async () => {
    const shell = setup(PlanarBaseWindow, w);
    shell.pushParts([simBase('base')]);
    shell.pushReading('base/planar-base', {
      capability: 'planar-base@1',
      reading: {
        tUs: 1,
        commanded: { vx: 0.3, vy: 0, omega: -0.5 },
        measured: { vx: 0.25, vy: 0, omega: -0.4 },
        bumped: true,
      },
    });
    expect(await screen.findByTestId('twist-commanded-vx')).toHaveTextContent('0.3 m/s');
    expect(screen.getByTestId('twist-measured-vx')).toHaveTextContent('0.25 m/s');
    expect(screen.getByTestId('twist-commanded-omega')).toHaveTextContent('-0.5 rad/s');
    expect(screen.getByText('Bumped into something')).toBeInTheDocument();
  });

  it('says who drives it, and opens the Drive window for it', async () => {
    const shell = setup(PlanarBaseWindow, w);
    shell.pushParts([simBase('base')]);
    expect(await screen.findByText('Nobody is driving it.')).toBeInTheDocument();
    shell.pushLeases([{ part: 'base', holder: 'drive-square.py' }]);
    expect(await screen.findByText('Driven by drive-square.py')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Drive' }));
    const drive = shell.desktop.state.windows.find((x) => x.kind === 'drive');
    expect(drive?.params).toEqual({ part: 'base' });
  });
});
