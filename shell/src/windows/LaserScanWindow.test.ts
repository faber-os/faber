import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { fakeShell, part } from '../test/fakeShell';
import LaserScanWindow from './LaserScanWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';

const win: WindowState = {
  id: 'w2',
  kind: 'laser-scan',
  key: 'demo/laser-scan',
  title: 'demo · scan',
  rect: { x: 0, y: 0, w: 500, h: 500 },
  chosen: { x: 0, y: 0, w: 500, h: 500 },
  maximised: false,
  minimised: false,
  z: 1,
  params: { address: 'demo/laser-scan', part: 'demo' },
};

function setup() {
  const shell = fakeShell();
  render(LaserScanWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

describe('Laser-scan viewer', () => {
  it('subscribes to its signal at about 15 Hz', () => {
    const shell = setup();
    const sub = shell.sent().find((m) => m.topic?.kind === 'signal');
    expect(sub?.topic).toEqual({ kind: 'signal', address: 'demo/laser-scan', maxHz: 15 });
  });

  it('waits for the first scan, saying why when the part is not running', async () => {
    const shell = setup();
    expect(screen.getByText(/Waiting for the first scan/)).toBeInTheDocument();
    shell.pushParts([part('demo', { state: { kind: 'starting', message: 'Opening the port.' } })]);
    expect(await screen.findByText('The part is starting: Opening the port.')).toBeInTheDocument();
  });

  it('shows points, no-returns and the revolution rate of the newest scan', async () => {
    const shell = setup();
    const ranges = [1, 2, Infinity, 3, Infinity, 4];
    shell.receive({
      type: 'signal',
      id: shell.subscriptionId('signal', 'demo/laser-scan'),
      address: 'demo/laser-scan',
      reading: {
        capability: 'laser-scan@1',
        reading: { tUs: 1, angleMin: 0, angleIncrement: 1, scanTime: 0.1, ranges },
      },
    });
    await expect.poll(() => screen.getByTestId('scan-points').textContent).toBe('4');
    expect(screen.getByTestId('scan-no-return')).toHaveTextContent('2');
    expect(screen.getByTestId('scan-rate')).toHaveTextContent('10.0 Hz');
    expect(screen.queryByText(/Waiting for the first scan/)).not.toBeInTheDocument();
  });

  it('shows a signal error with its hint', async () => {
    const shell = setup();
    shell.receive({
      type: 'error',
      id: shell.subscriptionId('signal', 'demo/laser-scan'),
      error: {
        code: 'unknown-signal',
        message: 'There is no signal demo/laser-scan.',
        hint: 'The part may have been removed. Close this viewer and open it again from Parts.',
      },
    });
    expect(await screen.findByText('There is no signal demo/laser-scan.')).toBeInTheDocument();
    expect(screen.getByText(/Close this viewer and open it again from Parts/)).toBeInTheDocument();
  });

  it('unsubscribes when the window closes', () => {
    const shell = fakeShell();
    const { unmount } = render(LaserScanWindow, {
      props: { win, visible: true },
      context: shell.context,
    });
    const id = shell.subscriptionId('signal', 'demo/laser-scan');
    unmount();
    expect(shell.sent().at(-1)).toEqual({ type: 'unsubscribe', id });
  });
});
