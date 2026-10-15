import { describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { DEFAULT_SETTINGS, fakeShell } from '../test/fakeShell';
import { formatClockTime } from '../lib/desktop/wallClock';
import Clock from './Clock.svelte';

describe('Clock', () => {
  it("shows the time in the zone and clock format the settings pick, not the browser's", async () => {
    const shell = fakeShell();
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      time: { timeZone: 'Pacific/Auckland', clock: '12h' },
    });
    render(Clock, { context: shell.context });
    const want = formatClockTime(Date.now(), { timeZone: 'Pacific/Auckland', clock: '12h' });
    await waitFor(() => expect(screen.getByRole('time')).toHaveTextContent(want));
    expect(screen.getByRole('time').getAttribute('title')).toMatch(/\d{4}$/);
  });
});
