import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import {
  DEFAULT_SETTINGS,
  GENERIC_BASE,
  GENERIC_DISTANCE,
  fakeShell,
  simBase,
  simBattery,
  simDistance,
} from '../test/fakeShell';
import type { WindowState } from '../lib/desktop/windows';
import DistanceWindow from './DistanceWindow.svelte';
import PlanarBaseWindow from './PlanarBaseWindow.svelte';
import PositionWindow from './PositionWindow.svelte';
import BatteryWindow from './BatteryWindow.svelte';

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

function setup(
  component: typeof DistanceWindow | typeof PlanarBaseWindow | typeof PositionWindow,
  w: WindowState,
) {
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

describe('Position viewer', () => {
  const w = win('position', 'gps/position', 'gps');
  const reading = {
    tUs: 1,
    latitudeDeg: 55.676098,
    longitudeDeg: 12.568337,
    altitudeM: 12.5,
    fix: 'fix3d' as const,
    satellites: 7,
  };

  it('shows the coordinates, fix and satellites, and links to a map when online', async () => {
    const shell = setup(PositionWindow, w);
    const sub = shell.sent().find((m) => m.topic?.kind === 'signal');
    expect(sub?.topic).toEqual({ kind: 'signal', address: 'gps/position', maxHz: 5 });
    shell.pushReading('gps/position', { capability: 'position@1', reading });
    const facts = await screen.findByTestId('position');
    expect(facts).toHaveTextContent('55.676098° N');
    expect(facts).toHaveTextContent('12.568337° E');
    expect(facts).toHaveTextContent('3D');
    expect(facts).toHaveTextContent('7');
    expect(facts).toHaveTextContent('12.5 m');
    const map = screen.getByRole('link', { name: 'Open in OpenStreetMap' });
    expect(map.getAttribute('href')).toContain('mlat=55.676098&mlon=12.568337');
    expect(map).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('offers no map while offline, and says when there is no fix yet', async () => {
    const online = Object.getOwnPropertyDescriptor(Navigator.prototype, 'onLine');
    Object.defineProperty(navigator, 'onLine', { configurable: true, get: () => false });
    try {
      const shell = setup(PositionWindow, w);
      shell.pushReading('gps/position', { capability: 'position@1', reading });
      expect(await screen.findByText(/Connect this computer to the internet/)).toBeInTheDocument();
      expect(screen.queryByRole('link', { name: 'Open in OpenStreetMap' })).toBeNull();
      shell.pushReading('gps/position', {
        capability: 'position@1',
        reading: { ...reading, fix: 'none', satellites: 0 },
      });
      expect(await screen.findByText('No fix yet')).toBeInTheDocument();
    } finally {
      delete (navigator as unknown as Record<string, unknown>).onLine;
      if (online) Object.defineProperty(Navigator.prototype, 'onLine', online);
    }
  });
});

describe('Battery viewer', () => {
  const w = win('battery', 'pack/battery', 'pack');

  it('shows the level in words, the voltage, how full it is and what the battery is', async () => {
    const shell = setup(BatteryWindow, w);
    shell.pushParts([simBattery('pack')]);
    const sub = shell.sent().find((m) => m.topic?.kind === 'signal');
    expect(sub?.topic).toEqual({ kind: 'signal', address: 'pack/battery', maxHz: 5 });
    shell.pushReading('pack/battery', {
      capability: 'battery@1',
      reading: { tUs: 1, volts: 11.84, amps: -1.5, charge: 0.62, level: 'ok' },
    });
    expect(await screen.findByTestId('battery-level')).toHaveTextContent('OK');
    expect(screen.getByTestId('battery-volts')).toHaveTextContent('11.84 V');
    expect(screen.getByRole('meter', { name: 'How full it is' })).toHaveAttribute(
      'aria-valuenow',
      '62',
    );
    const facts = screen.getByTestId('battery');
    expect(facts).toHaveTextContent('62 %');
    expect(facts).toHaveTextContent('1.5 A out');
    expect(facts).toHaveTextContent('Li-ion, 3 in series, 2.2 Ah');
    expect(facts).toHaveTextContent('Low below 10.5 V, critical below 9.9 V');
  });

  it('says when to charge, and gauges a battery that reports only its voltage', async () => {
    const shell = setup(BatteryWindow, w);
    shell.pushParts([simBattery('pack')]);
    shell.pushReading('pack/battery', {
      capability: 'battery@1',
      reading: { tUs: 1, volts: 9.8, level: 'critical', cellVolts: [3.25, 3.3, 3.25] },
    });
    expect(await screen.findByTestId('battery-level')).toHaveTextContent(
      'Critical: stop and charge it now',
    );
    expect(screen.getByRole('meter', { name: 'How full it is' })).toHaveAttribute(
      'aria-valuenow',
      '22',
    );
    expect(screen.getByText('Cell 2')).toBeInTheDocument();
    expect(screen.getByText('3.3 V')).toBeInTheDocument();
  });
});
