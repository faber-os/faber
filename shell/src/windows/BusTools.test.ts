import { describe, expect, it } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, part } from '../test/fakeShell';
import type { FakeRoute } from '../test/fakeFetch';
import type { WindowState } from '../lib/desktop/windows';
import type { ConnectorInfo, PortInfo } from '../lib/state/types';
import I2cScannerWindow from './I2cScannerWindow.svelte';
import GpioPanelWindow from './GpioPanelWindow.svelte';
import PortsWindow from './PortsWindow.svelte';
import SignalPlotterWindow from './SignalPlotterWindow.svelte';
import TemperatureWindow from './TemperatureWindow.svelte';
import LevelWindow from './LevelWindow.svelte';
import FractionWindow from './FractionWindow.svelte';

function win(kind: string, params: Record<string, string> = {}): WindowState {
  return {
    id: 'w1',
    kind,
    key: kind,
    title: kind,
    rect: { x: 0, y: 0, w: 600, h: 600 },
    chosen: { x: 0, y: 0, w: 600, h: 600 },
    maximised: false,
    minimised: false,
    z: 1,
    params,
  };
}

const SYSTEM = (buses: string): FakeRoute => ({
  path: '/api/v0/system',
  body: { name: 'Faber', version: '0.1.0', timeUs: 1, timeZone: 'UTC', buses },
});

const TMP102: ConnectorInfo = {
  id: 'ti-tmp102',
  name: 'TI TMP102 temperature sensor',
  version: '1.0.0',
  kind: 'declarative',
  port: { kind: 'i2c', addresses: [0x48, 0x49, 0x4a, 0x4b], default: 0x48 },
  provides: [{ name: 'temperature', capability: 'temperature@1' }],
  settings: [],
};

const I2C1: PortInfo = { id: 'i2c-1', kind: 'i2c', label: 'I²C bus 1', suggestions: [] };

function gpioPort(line: number, extra: Partial<PortInfo> = {}): PortInfo {
  return { id: `gpio-${line}`, kind: 'gpio', label: `GPIO${line}`, suggestions: [], ...extra };
}

const HEADER = Array.from({ length: 40 }, (_, i) => {
  const pin = i + 1;
  const lines: Record<number, number> = { 3: 2, 5: 3, 11: 17, 15: 22 };
  const line = lines[pin];
  if (line !== undefined) return { pin, name: `GPIO${line}`, line };
  return { pin, name: pin % 2 === 0 ? '5V' : 'GND' };
});

function show(component: typeof I2cScannerWindow, w: WindowState, routes: FakeRoute[] = []) {
  const shell = fakeShell(routes);
  render(component, { props: { win: w, visible: true }, context: shell.context });
  return shell;
}

describe('I2C scanner', () => {
  it('says in plain words when this computer has no I²C', async () => {
    const shell = show(I2cScannerWindow, win('i2c-scanner'), [SYSTEM('none')]);
    shell.pushPorts([]);
    expect(
      await screen.findByText(/Windows and macOS don’t let programs reach one/),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Scan' })).toBeNull();
  });

  it('needs an operator to scan', async () => {
    const shell = show(I2cScannerWindow, win('i2c-scanner'), [SYSTEM('fake')]);
    shell.signInAs('viewer');
    shell.pushPorts([I2C1]);
    const button = await screen.findByRole('button', { name: 'Scan' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Needs the operator role');
  });

  it('scans only when asked, shows the grid and adds a suggested part at its address', async () => {
    const scan = {
      port: 'i2c-1',
      first: 0x08,
      last: 0x77,
      found: [
        {
          address: 0x48,
          suggestions: [
            {
              chip: 'TI TMP102 temperature sensor',
              connector: 'ti-tmp102',
              reason: '0x48 is a common default address for the TI TMP102. The scan can’t be sure.',
            },
          ],
        },
        { address: 0x4a, usedBy: 'cabin_temp', suggestions: [] },
      ],
    };
    const shell = show(I2cScannerWindow, win('i2c-scanner'), [
      SYSTEM('fake'),
      { path: '/api/v0/connectors', body: [TMP102] },
      { method: 'POST', path: '/api/v0/ports/scan-i2c', body: scan },
    ]);
    shell.pushPorts([I2C1]);
    const button = await screen.findByRole('button', { name: 'Scan' });
    expect(shell.fetch.calls.some((c) => c.path.endsWith('/scan-i2c'))).toBe(false);
    await userEvent.click(button);
    expect(await screen.findByTestId('i2c-0x48')).toHaveTextContent('48');
    expect(screen.getByTestId('i2c-0x4a')).toHaveTextContent('UU');
    expect(shell.fetch.calls.find((c) => c.path.endsWith('/scan-i2c'))?.body).toEqual({
      port: 'i2c-1',
    });
    expect(screen.getByText(/common default addresses/)).toBeInTheDocument();
    const found = screen.getByTestId('i2c-found-0x48');
    expect(within(found).getByText('Maybe a TI TMP102 temperature sensor')).toBeInTheDocument();
    await userEvent.click(within(found).getByRole('button', { name: /Add a TI TMP102.* at 0x48/ }));
    const parts = shell.desktop.state.windows.find((w) => w.kind === 'parts');
    expect(parts?.params).toEqual({
      addPort: 'i2c-1',
      addConnector: 'ti-tmp102',
      addAddress: '72',
    });
  });
});

describe('Ports', () => {
  it('lists every kind, who has which address on a bus, and what is reserved', async () => {
    const shell = show(PortsWindow, win('ports'), [SYSTEM('fake')]);
    shell.pushPorts([
      { ...I2C1, addresses: [{ address: 0x48, claimedBy: 'cabin_temp' }] },
      gpioPort(2, { reserved: 'used by I²C bus 1 (i2c-1)' }),
      gpioPort(17, { claimedBy: 'button' }),
    ]);
    expect(await screen.findByText('0x48: cabin_temp')).toBeInTheDocument();
    expect(screen.getByText('Not usable: used by I²C bus 1 (i2c-1)')).toBeInTheDocument();
    expect(screen.getByText('I²C bus')).toBeInTheDocument();
    expect(screen.getAllByText('GPIO pin')).toHaveLength(2);
    expect(await screen.findByText(/simulated/)).toBeInTheDocument();
  });
});

describe('GPIO panel', () => {
  function gpioShell() {
    const shell = show(GpioPanelWindow, win('gpio-panel'), [
      SYSTEM('fake'),
      { path: '/api/v0/ports/header', body: HEADER },
      {
        method: 'POST',
        path: '/api/v0/parts/relay/lease',
        status: 201,
        body: { lease: 'L1', part: 'relay', holder: 'gpio-panel', deadlineUs: 1, locks: [] },
      },
      {
        method: 'POST',
        path: '/api/v0/parts/relay/command',
        body: {
          applied: { capability: 'digital-out@1', command: { kind: 'set', level: true } },
          clamped: false,
          deadlineUs: 2,
        },
      },
      { method: 'DELETE', path: '/api/v0/parts/relay/lease?lease=L1', status: 204 },
    ]);
    shell.pushPorts([
      gpioPort(2, { reserved: 'used by I²C bus 1 (i2c-1)' }),
      gpioPort(17, { claimedBy: 'button' }),
      gpioPort(22, { claimedBy: 'relay' }),
    ]);
    shell.pushParts([
      part('button', {
        connector: 'gpio-input',
        port: 'gpio-17',
        signals: [{ address: 'button/digital-in', capability: 'digital-in@1' }],
      }),
      part('relay', {
        connector: 'gpio-output',
        port: 'gpio-22',
        signals: [{ address: 'relay/digital-out', capability: 'digital-out@1' }],
      }),
    ]);
    return shell;
  }

  it('shows the header with numbers and names, and inputs live', async () => {
    const shell = gpioShell();
    const pin11 = await screen.findByText('GPIO17');
    const cell = pin11.closest('[data-pin]') as HTMLElement;
    expect(cell.dataset.pin).toBe('11');
    expect(within(cell).getByText('button')).toBeInTheDocument();
    shell.pushReading('button/digital-in', {
      capability: 'digital-in@1',
      reading: { tUs: 1, level: true },
    });
    expect(await within(cell).findByRole('img', { name: 'On' })).toBeInTheDocument();
    expect(screen.getByText('used by I²C bus 1 (i2c-1)')).toBeInTheDocument();
  });

  it('switches an output only after the confirm, with a lease', async () => {
    const shell = gpioShell();
    const on = await screen.findByRole('button', { name: 'Switch on' });
    await userEvent.click(on);
    expect(screen.getByText('Switch outputs?')).toBeInTheDocument();
    expect(shell.fetch.calls.some((c) => c.path.includes('/lease'))).toBe(false);
    const dialog = screen.getByRole('alertdialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Switch on' }));
    expect(await screen.findByRole('button', { name: 'Switch off' })).toBeInTheDocument();
    const command = shell.fetch.calls.find((c) => c.path === '/api/v0/parts/relay/command');
    expect(command?.body).toEqual({
      lease: 'L1',
      command: { capability: 'digital-out@1', command: { kind: 'set', level: true } },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Switch off' }));
    expect(
      shell.fetch.calls.some((c) => c.method === 'DELETE' && c.path.includes('/relay/lease')),
    ).toBe(true);
  });
});

describe('Signal plotter', () => {
  it('plots a picked number and saves it as CSV', async () => {
    const shell = show(SignalPlotterWindow, win('signal-plotter'));
    shell.pushParts([
      part('cabin_temp', {
        connector: 'ti-tmp102',
        port: 'i2c-1',
        signals: [{ address: 'cabin_temp/temperature', capability: 'temperature@1' }],
      }),
    ]);
    await userEvent.selectOptions(
      await screen.findByRole('combobox', { name: 'Signal' }),
      'cabin_temp/temperature',
    );
    shell.pushReading('cabin_temp/temperature', {
      capability: 'temperature@1',
      reading: { tUs: 1_000_000, celsius: 21.5 },
    });
    const field = screen.getByRole('combobox', { name: 'Value' });
    await screen.findByRole('option', { name: 'celsius' });
    expect(field).toHaveValue('celsius');
    await userEvent.click(screen.getByRole('button', { name: 'Plot' }));
    shell.pushReading('cabin_temp/temperature', {
      capability: 'temperature@1',
      reading: { tUs: 2_000_000, celsius: 22 },
    });
    expect(
      await screen.findByRole('button', { name: 'Stop plotting cabin_temp/temperature celsius' }),
    ).toBeInTheDocument();
    const line = document.querySelector('[data-testid="plotter-plot"] polyline');
    expect(line?.getAttribute('points')).not.toBe('');
    expect(screen.getByRole('button', { name: 'Save as CSV' })).toBeEnabled();
    await userEvent.click(screen.getByRole('button', { name: 'Pause' }));
    expect(screen.getByRole('button', { name: 'Go on' })).toHaveAttribute('aria-pressed', 'true');
  });
});

describe('Viewers of the new capabilities', () => {
  it('shows a temperature', async () => {
    const shell = show(
      TemperatureWindow,
      win('temperature', { address: 't/temperature', part: 't' }),
    );
    shell.pushReading('t/temperature', {
      capability: 'temperature@1',
      reading: { tUs: 1, celsius: 21.5 },
    });
    expect(await screen.findByTestId('temperature-value')).toHaveTextContent('21.5 °C');
  });

  it('shows a digital level and counts its changes', async () => {
    const shell = show(LevelWindow, win('level', { address: 'b/digital-in', part: 'b' }));
    for (const [tUs, level] of [
      [1, false],
      [2, true],
      [3, false],
    ] as const)
      shell.pushReading('b/digital-in', { capability: 'digital-in@1', reading: { tUs, level } });
    expect(await screen.findByTestId('level-value')).toHaveTextContent('Off');
    expect(screen.getByText('Changed 2 times since this window opened')).toBeInTheDocument();
  });

  it('shows an analog input with its volts, and a PWM duty', async () => {
    const shell = show(FractionWindow, win('fraction', { address: 'a/analog-in', part: 'a' }));
    shell.pushReading('a/analog-in', {
      capability: 'analog-in@1',
      reading: { tUs: 1, fraction: 0.25, volts: 0.8 },
    });
    expect(await screen.findByTestId('fraction-value')).toHaveTextContent('25 %');
    expect(screen.getByText('of full scale · 0.8 V')).toBeInTheDocument();
    shell.pushReading('a/analog-in', { capability: 'pwm-out@1', reading: { tUs: 2, duty: 0.5 } });
    expect(await screen.findByText('duty')).toBeInTheDocument();
  });
});
