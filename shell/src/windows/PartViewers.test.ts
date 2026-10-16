import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { fakeShell, part } from '../test/fakeShell';
import type { WindowState } from '../lib/desktop/windows';
import { hasViewer } from '../components/driveWindow';
import LevelWindow from './LevelWindow.svelte';
import ServoWindow from './ServoWindow.svelte';
import EncoderWindow from './EncoderWindow.svelte';
import ImuWindow from './ImuWindow.svelte';

// The viewers of the first wave of part capabilities (docs/research/common-parts.md §3):
// obstacle@1, servo@1, encoder@1 and imu@1.

function win(kind: string, address: string): WindowState {
  return {
    id: 'w1',
    kind,
    key: address,
    title: kind,
    rect: { x: 0, y: 0, w: 500, h: 400 },
    chosen: { x: 0, y: 0, w: 500, h: 400 },
    maximised: false,
    minimised: false,
    z: 1,
    params: { address, part: address.split('/')[0] ?? '' },
  };
}

function show(component: typeof LevelWindow, w: WindowState) {
  const shell = fakeShell([]);
  render(component, { props: { win: w, visible: true }, context: shell.context });
  return shell;
}

describe('Viewers of the first wave of parts', () => {
  it('has one for every new capability', () => {
    for (const c of ['obstacle@1', 'servo@1', 'encoder@1', 'imu@1'])
      expect(hasViewer(c)).toBe(true);
  });

  it('shows an obstacle as detected or clear, and counts the bumps', async () => {
    const shell = show(LevelWindow, win('level', 'bumper/obstacle'));
    for (const [tUs, detected] of [
      [1, false],
      [2, true],
    ] as const)
      shell.pushReading('bumper/obstacle', {
        capability: 'obstacle@1',
        reading: { tUs, detected },
      });
    expect(await screen.findByTestId('level-value')).toHaveTextContent('Detected');
    expect(screen.getByText('Changed once since this window opened')).toBeInTheDocument();
  });

  it('shows the angle a servo was told, or that it is released', async () => {
    const shell = show(ServoWindow, win('servo', 'arm/servo'));
    shell.pushParts([
      part('arm', {
        connector: 'generic-servo',
        port: undefined,
        simulate: 'manual',
        signals: [
          {
            address: 'arm/servo',
            capability: 'servo@1',
            info: {
              capability: 'servo@1',
              info: { minAngleRad: 0, maxAngleRad: Math.PI, safeState: 'hold' },
            },
          },
        ],
      }),
    ]);
    shell.pushReading('arm/servo', {
      capability: 'servo@1',
      reading: { tUs: 1, angleRad: Math.PI / 2 },
    });
    expect(await screen.findByTestId('servo-value')).toHaveTextContent('1.57 rad (90°)');
    expect(screen.getByText('From 0 rad (0°) to 3.14 rad (180°)')).toBeInTheDocument();
    shell.pushReading('arm/servo', { capability: 'servo@1', reading: { tUs: 2 } });
    expect(await screen.findByText('Released')).toBeInTheDocument();
  });

  it("shows an encoder's count and its speed in rad/s once it knows its counts per turn", async () => {
    const shell = show(EncoderWindow, win('encoder', 'wheel/encoder'));
    shell.pushReading('wheel/encoder', {
      capability: 'encoder@1',
      reading: { tUs: 1, count: 1200, delta: 12, countsPerS: 120 },
    });
    expect(await screen.findByTestId('encoder-count')).toHaveTextContent('1200 counts');
    expect(screen.getByTestId('encoder-speed')).toHaveTextContent('120 counts/s');
    shell.pushReading('wheel/encoder', {
      capability: 'encoder@1',
      reading: { tUs: 2, count: 1212, delta: 12, countsPerS: 120, velocityRadS: 0.75 },
    });
    expect(await screen.findByText('0.75 rad/s')).toBeInTheDocument();
  });

  it('shows how an IMU turns and is accelerated', async () => {
    const shell = show(ImuWindow, win('imu', 'imu/imu'));
    shell.pushReading('imu/imu', {
      capability: 'imu@1',
      reading: {
        tUs: 1,
        angularVelocity: { x: 0, y: 0, z: 0.5 },
        linearAcceleration: { x: 0.2, y: 0, z: 9.80665 },
      },
    });
    expect(await screen.findByTestId('imu-turn')).toHaveTextContent('0.5 rad/s');
    expect(screen.getByTestId('imu-acceleration')).toHaveTextContent('9.81 m/s²');
  });
});
