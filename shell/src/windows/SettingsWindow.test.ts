import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, listing } from '../test/fakeShell';
import SettingsWindow from './SettingsWindow.svelte';
import xx from '../test/fixtures/xx.json';
import { compareTranslation } from '../lib/i18n/check';
import { en } from '../lib/i18n/en';

function setup(routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  shell.pushSettings();
  const id = shell.desktop.open('settings');
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(SettingsWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

const MACHINE = 'Allow Files to see the whole machine';
const ON = { ...DEFAULT_SETTINGS, developer: { fullMachineFiles: true } };

describe('Settings', () => {
  it('changes the theme for every screen through the core', async () => {
    const dark = {
      ...DEFAULT_SETTINGS,
      appearance: { ...DEFAULT_SETTINGS.appearance, theme: 'dark' as const },
    };
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: dark }]);
    await userEvent.click(await screen.findByRole('radio', { name: 'Dark' }));
    await waitFor(() => expect(shell.core.settings?.appearance.theme).toBe('dark'));
    expect(shell.fetch.calls.find((c) => c.method === 'PUT')?.body).toEqual(dark);
  });

  it('picks a wallpaper', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    await userEvent.click(await screen.findByRole('radio', { name: /Cutting mat/ }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.find((c) => c.method === 'PUT')?.body as typeof DEFAULT_SETTINGS)
          .appearance.wallpaper,
      ).toBe('cutting-mat'),
    );
  });

  it('follows changes made on another screen', async () => {
    const shell = setup();
    const mode = await screen.findByRole('radiogroup', { name: 'Mode' });
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      appearance: { theme: 'light', style: 'aluminium', wallpaper: 'graphite', language: 'en' },
    });
    await waitFor(() => expect(within(mode).getByRole('radio', { name: 'Light' })).toBeChecked());
  });

  it('turns on whole-machine files only after the warning is confirmed', async () => {
    const shell = setup([{ method: 'POST', path: '/api/v0/settings/developer', body: ON }]);
    const toggle = await screen.findByRole('switch', {
      name: 'Allow Files to see the whole machine',
    });
    await userEvent.click(toggle);
    expect(toggle).not.toBeChecked();
    const dialog = screen.getByRole('alertdialog', { name: 'Let Files see the whole machine?' });
    expect(dialog).toHaveTextContent('no Trash outside the home folder');
    const allow = within(dialog).getByRole('button', { name: 'Allow whole-machine access' });
    expect(allow).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('checkbox'));
    expect(allow).toBeDisabled();
    await userEvent.type(within(dialog).getByRole('textbox'), 'machine');
    expect(allow).toBeEnabled();
    expect(shell.fetch.calls.some((c) => c.method === 'POST')).toBe(false);
    await userEvent.click(allow);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toEqual({
      fullMachineFiles: true,
      acknowledgedRisk: true,
    });
  });

  it('cancelling the warning leaves it off', async () => {
    const shell = setup();
    await userEvent.click(await screen.findByRole('switch', { name: MACHINE }));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(screen.getByRole('switch', { name: MACHINE })).not.toBeChecked();
    expect(shell.fetch.calls.some((c) => c.method === 'POST')).toBe(false);
  });

  it('turns it off without a warning', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/settings/developer', body: DEFAULT_SETTINGS },
    ]);
    shell.pushSettings(ON);
    const toggle = await screen.findByRole('switch', { name: MACHINE });
    await waitFor(() => expect(toggle).toBeChecked());
    await userEvent.click(toggle);
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.method === 'POST')?.body).toEqual({
        fullMachineFiles: false,
        acknowledgedRisk: false,
      }),
    );
  });

  const lastPut = (shell: ReturnType<typeof fakeShell>) =>
    shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body as
      typeof DEFAULT_SETTINGS | undefined;

  it('picks a style from the style cards', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    const group = await screen.findByRole('group', { name: 'Style' });
    expect(within(group).getAllByRole('radio')).toHaveLength(6);
    expect(within(group).getByRole('radio', { name: 'Aluminium' })).toBeChecked();
    await userEvent.click(within(group).getByRole('radio', { name: 'Cobalt' }));
    await waitFor(() => expect(lastPut(shell)?.appearance.style).toBe('cobalt'));
  });

  it('sets the taskbar size and what is pinned to it', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    await userEvent.click(await screen.findByRole('radio', { name: 'Large' }));
    await waitFor(() => expect(lastPut(shell)?.taskbar.size).toBe('large'));
    const logs = screen.getByRole('switch', { name: 'Pin Logs to the taskbar' });
    expect(logs).not.toBeChecked();
    expect(screen.getByRole('switch', { name: 'Pin Parts to the taskbar' })).toBeChecked();
    await userEvent.click(logs);
    await waitFor(() => expect(lastPut(shell)?.taskbar.pinned).toContain('tool:logs'));
  });

  it('has a sidebar of sections in groups', async () => {
    setup();
    const nav = await screen.findByRole('navigation', { name: 'Settings sections' });
    expect(
      within(nav)
        .getAllByRole('button')
        .map((b) => b.textContent?.trim()),
    ).toEqual([
      'Appearance',
      'Accessibility',
      'Units',
      'Desktop and taskbar',
      'Name',
      'Date and time',
      'Storage and backups',
      'About and diagnostics',
      'Developer',
    ]);
    expect([...nav.querySelectorAll('.group')].map((g) => g.textContent?.trim())).toEqual([
      'You',
      'This machine',
      'Advanced',
    ]);
    expect(within(nav).getByRole('button', { name: 'Appearance' })).toHaveAttribute(
      'aria-current',
      'true',
    );
    await userEvent.click(within(nav).getByRole('button', { name: 'Developer' }));
    expect(within(nav).getByRole('button', { name: 'Developer' })).toHaveAttribute(
      'aria-current',
      'true',
    );
  });
});

const ok = (shell: ReturnType<typeof fakeShell>) =>
  shell.fetch.route({ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS });
const putBody = (shell: ReturnType<typeof fakeShell>) =>
  shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body as
    typeof DEFAULT_SETTINGS | undefined;

describe('Settings: machine name', () => {
  it('saves a tidied name, and explains a name it cannot use without saving it', async () => {
    const shell = setup();
    ok(shell);
    const input = await screen.findByLabelText('Machine name');
    await userEvent.clear(input);
    await userEvent.type(input, 'My Robot{Enter}');
    await waitFor(() => expect(putBody(shell)?.machine.name).toBe('my-robot'));

    const puts = shell.fetch.calls.filter((c) => c.method === 'PUT').length;
    await userEvent.clear(input);
    await userEvent.type(input, 'æble');
    expect(screen.getByRole('alert')).toHaveTextContent(/lowercase/);
    await userEvent.tab();
    expect(shell.fetch.calls.filter((c) => c.method === 'PUT')).toHaveLength(puts);
  });
});

describe('Settings: accessibility, units and time', () => {
  it('sets reduce motion, higher contrast and larger text for every screen', async () => {
    const shell = setup();
    ok(shell);
    const motion = await screen.findByRole('radiogroup', { name: 'Reduce motion' });
    expect(within(motion).getByRole('radio', { name: 'System' })).toBeChecked();
    await userEvent.click(within(motion).getByRole('radio', { name: 'On' }));
    await waitFor(() => expect(putBody(shell)?.accessibility.reduceMotion).toBe('on'));
    const contrast = screen.getByRole('radiogroup', { name: 'Higher contrast' });
    await userEvent.click(within(contrast).getByRole('radio', { name: 'Off' }));
    await waitFor(() => expect(putBody(shell)?.accessibility.higherContrast).toBe('off'));
    const text = screen.getByRole('radiogroup', { name: 'Text size' });
    expect(within(text).getAllByRole('radio')).toHaveLength(4);
    await userEvent.click(within(text).getByRole('radio', { name: '130 %' }));
    await waitFor(() => expect(putBody(shell)?.accessibility.textSize).toBe('130'));
  });

  it('switches lengths to imperial', async () => {
    const shell = setup();
    ok(shell);
    const length = await screen.findByRole('radiogroup', { name: 'Lengths' });
    await userEvent.click(within(length).getByRole('radio', { name: /Imperial/ }));
    await waitFor(() => expect(putBody(shell)?.units.length).toBe('imperial'));
    expect(screen.queryByRole('radiogroup', { name: 'Angles' })).toBeNull();
  });

  it("offers the machine's own time zone first, then every zone, and the clock format", async () => {
    const shell = setup();
    ok(shell);
    const zone = await screen.findByRole('combobox', { name: 'Time zone' });
    await waitFor(() =>
      expect(within(zone).getAllByRole('option')[0]).toHaveTextContent(
        "Machine's own (Europe/Copenhagen)",
      ),
    );
    expect((zone as HTMLSelectElement).value).toBe('');
    await userEvent.selectOptions(zone, 'America/New_York');
    await waitFor(() => expect(putBody(shell)?.time.timeZone).toBe('America/New_York'));
    const clock = screen.getByRole('radiogroup', { name: 'Clock' });
    await userEvent.click(within(clock).getByRole('radio', { name: /12-hour/ }));
    await waitFor(() => expect(putBody(shell)?.time.clock).toBe('12h'));
  });
});

const STORAGE = {
  recordingsBytes: 2 * 1024 * 1024,
  documentsBytes: 1024,
  desktopBytes: 0,
  backupsBytes: 5000,
  freeBytes: 50 * 1024 ** 3,
  totalBytes: 100 * 1024 ** 3,
};

describe('Settings: storage and backups', () => {
  it('backs up now, without recordings unless ticked', async () => {
    const shell = setup([
      { path: '/api/v0/storage', body: STORAGE },
      {
        method: 'POST',
        path: '/api/v0/backups',
        status: 201,
        body: { path: 'home:/Backups/faber-backup-2026-10-15-1430.zip', bytes: 5000 },
      },
    ]);
    const box = await screen.findByRole('checkbox', { name: 'Include recordings (2.0 MiB)' });
    expect(box).not.toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Back up now' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path === '/api/v0/backups')?.body).toEqual({
        includeRecordings: false,
      }),
    );
    expect(await screen.findByText(/Saved faber-backup-2026-10-15-1430\.zip/)).toBeInTheDocument();
    await userEvent.click(box);
    await userEvent.click(screen.getByRole('button', { name: 'Back up now' }));
    await waitFor(() =>
      expect(shell.fetch.calls.filter((c) => c.path === '/api/v0/backups').at(-1)?.body).toEqual({
        includeRecordings: true,
      }),
    );
  });

  it('lists the backups and restores one only after a confirm', async () => {
    const shell = setup([
      { path: '/api/v0/storage', body: STORAGE },
      listing('home:/Backups', [
        { name: 'faber-backup-2026-10-15-1430.zip', size: 5000, modifiedMs: 1 },
      ]),
      { method: 'POST', path: '/api/v0/backups/restore', status: 204 },
    ]);
    const list = await screen.findByRole('list', { name: 'Backups' });
    const restore = await within(list).findByRole('button', {
      name: 'Restore faber-backup-2026-10-15-1430.zip',
    });
    // Any size downloads (the core streams it), so a copy can leave the machine.
    const download = within(list).getByRole('link', { name: 'Download' });
    expect(download.getAttribute('href')).toContain('download=1');
    expect(download.getAttribute('href')).toContain(
      encodeURIComponent('home:/Backups/faber-backup-2026-10-15-1430.zip'),
    );
    await userEvent.click(restore);
    const dialog = screen.getByRole('alertdialog', { name: /Restore this backup\?/ });
    expect(dialog).toHaveTextContent('Trash');
    expect(shell.fetch.calls.some((c) => c.path === '/api/v0/backups/restore')).toBe(false);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Restore' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path === '/api/v0/backups/restore')?.body).toEqual({
        path: 'home:/Backups/faber-backup-2026-10-15-1430.zip',
      }),
    );
  });

  it('shows what restore refused, with what to do', async () => {
    setup([
      { path: '/api/v0/storage', body: STORAGE },
      listing('home:/Backups', [{ name: 'b.zip', size: 10 }]),
      {
        method: 'POST',
        path: '/api/v0/backups/restore',
        status: 422,
        body: {
          error: {
            code: 'config-invalid',
            message: 'parts.toml has a problem on line 2: expected a value',
            hint: 'Fix the line.',
          },
        },
      },
    ]);
    await userEvent.click(await screen.findByRole('button', { name: 'Restore b.zip' }));
    await userEvent.click(screen.getByRole('button', { name: 'Restore' }));
    expect(await screen.findByText(/problem on line 2/)).toBeInTheDocument();
  });

  it('shows how full the disk is, and warns above 80 %', async () => {
    const full = { ...STORAGE, freeBytes: 15 * 1024 ** 3 };
    setup([{ path: '/api/v0/storage', body: full }]);
    const bar = await screen.findByRole('meter', { name: 'Disk space used' });
    expect(bar).toHaveAttribute('aria-valuenow', '85');
    expect(screen.getByText(/85 % full/)).toBeInTheDocument();
  });

  it('does not warn when there is room', async () => {
    setup([{ path: '/api/v0/storage', body: STORAGE }]);
    await screen.findByRole('meter', { name: 'Disk space used' });
    expect(screen.queryByText(/% full/)).not.toBeInTheDocument();
  });
});

describe('Settings: about and diagnostics', () => {
  const ABOUT = {
    version: '0.1.0',
    api: '5499d40f4fdcb9d4',
    dataFolder: 'C:\\Users\\someone\\faber\\data',
    uptimeS: 65,
    os: 'windows (x86_64)',
    timeZone: 'Europe/Copenhagen',
  };

  it('says which Faber this is and where its data is', async () => {
    setup([{ path: '/api/v0/about', body: ABOUT }]);
    const about = await screen.findByRole('region', { name: 'About and diagnostics' });
    await waitFor(() => expect(about).toHaveTextContent('0.1.0'));
    expect(about).toHaveTextContent('C:\\Users\\someone\\faber\\data');
    expect(about).toHaveTextContent('windows (x86_64)');
  });

  it('copies diagnostics without personal data', async () => {
    const user = userEvent.setup();
    setup([{ path: '/api/v0/about', body: ABOUT }]);
    await screen.findByText('windows (x86_64)');
    await user.click(screen.getByRole('button', { name: 'Copy diagnostics' }));
    await screen.findByText('Copied.');
    const text = await navigator.clipboard.readText();
    expect(text).toContain('Faber 0.1.0');
    expect(text).not.toContain('someone');
  });

  it('resets the settings only after a confirm', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/settings/reset', body: DEFAULT_SETTINGS },
    ]);
    await userEvent.click(await screen.findByRole('button', { name: 'Reset settings…' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Reset every setting?' });
    expect(dialog).toHaveTextContent('Parts and files');
    expect(shell.fetch.calls.some((c) => c.path === '/api/v0/settings/reset')).toBe(false);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Reset settings' }));
    await waitFor(() =>
      expect(shell.fetch.calls.some((c) => c.path === '/api/v0/settings/reset')).toBe(true),
    );
  });

  it('shows no language picker while English is the only language (D44)', async () => {
    setup();
    await screen.findByRole('radiogroup', { name: 'Mode' });
    expect(screen.queryByRole('combobox', { name: 'Language' })).not.toBeInTheDocument();
  });

  it('offers a translation on the machine, with how complete it is', async () => {
    const testish = {
      ...DEFAULT_SETTINGS,
      appearance: { ...DEFAULT_SETTINGS.appearance, language: 'xx' },
    };
    const shell = setup([
      { path: '/api/v0/translations', body: [{ code: 'xx', name: 'Testish' }] },
      { path: '/api/v0/translations/xx', body: xx },
      { method: 'PUT', path: '/api/v0/settings', body: testish },
    ]);
    const picker = await screen.findByRole('combobox', { name: 'Language' });
    const percent = compareTranslation(en, xx, 'xx').percent;
    expect(within(picker).getByRole('option', { name: `Testish (${percent} %)` })).toBeVisible();
    await userEvent.selectOptions(picker, 'xx');
    await waitFor(() => expect(shell.core.settings?.appearance.language).toBe('xx'));
  });

  it('says which translation file is broken', async () => {
    setup([
      {
        path: '/api/v0/translations',
        body: [
          { code: 'xx', name: 'xx', problem: 'xx.json has a problem on line 3: expected `,`' },
        ],
      },
    ]);
    expect(
      await screen.findByText(
        "A translation can't be used: xx.json has a problem on line 3: expected `,`",
      ),
    ).toBeVisible();
    expect(screen.queryByRole('combobox', { name: 'Language' })).not.toBeInTheDocument();
  });
});
