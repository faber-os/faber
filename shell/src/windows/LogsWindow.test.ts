import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell } from '../test/fakeShell';
import LogsWindow from './LogsWindow.svelte';
import type { WindowState } from '../lib/desktop/windows';

const win: WindowState = {
  id: 'w3',
  kind: 'logs',
  key: 'logs',
  title: 'Logs',
  rect: { x: 0, y: 0, w: 600, h: 400 },
  maximised: false,
  minimised: false,
  z: 1,
  params: {},
};

describe('Logs', () => {
  it('shows live entries, filtered by level and text', async () => {
    const shell = fakeShell();
    render(LogsWindow, { props: { win, visible: true }, context: shell.context });
    expect(screen.getByText('Nothing logged yet.')).toBeInTheDocument();

    const id = shell.subscriptionId('logs');
    const entries = [
      { tUs: 10, level: 'debug', source: 'core::ports', message: 'Scanned 3 ports' },
      { tUs: 20, level: 'info', source: 'part:demo', message: 'Started' },
      { tUs: 30, level: 'warn', source: 'part:demo', message: 'Checksum mismatch, frame dropped' },
      {
        tUs: 40,
        level: 'error',
        source: 'core::config',
        message: 'parts.toml line 4: unknown key',
      },
    ];
    for (const entry of entries) shell.receive({ type: 'log', id, entry });

    expect(await screen.findByText('Started')).toBeInTheDocument();
    expect(screen.queryByText('Scanned 3 ports')).not.toBeInTheDocument();
    expect(screen.getByText('3 of 4')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('radio', { name: 'Debug' }));
    expect(screen.getByText('Scanned 3 ports')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('radio', { name: 'Warnings' }));
    expect(screen.queryByText('Started')).not.toBeInTheDocument();
    expect(screen.getByText('parts.toml line 4: unknown key')).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText('Filter by text'), 'checksum');
    expect(screen.getByText('Checksum mismatch, frame dropped')).toBeInTheDocument();
    expect(screen.queryByText('parts.toml line 4: unknown key')).not.toBeInTheDocument();
  });
});
