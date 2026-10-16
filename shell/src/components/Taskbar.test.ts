import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, WALL_FOLLOWER, fakeShell } from '../test/fakeShell';
import type { Settings } from '../lib/state/types';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';
import { openInNotes } from './fileOps';

function setup(settings: Settings = DEFAULT_SETTINGS) {
  const shell = fakeShell([{ method: 'PUT', path: '/api/v0/settings', body: settings }]);
  shell.pushSettings(settings);
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
}

const bar = () => screen.getByRole('list', { name: 'Programs and windows' });
const names = () =>
  within(bar())
    .getAllByRole('button')
    .map((b) => b.getAttribute('aria-label'));
const savedSettings = (shell: ReturnType<typeof fakeShell>) =>
  shell.fetch.calls.filter((c) => c.method === 'PUT').at(-1)?.body as Settings | undefined;

describe('Taskbar', () => {
  it('shows the pinned programs, in order, with nothing open', async () => {
    setup();
    await waitFor(() => expect(names()).toEqual(['Parts', 'Files', 'Notes', 'Serial monitor']));
    for (const b of within(bar()).getAllByRole('button')) {
      expect(b).toHaveAttribute('aria-pressed', 'false');
    }
  });

  it('starts a pinned program, and marks it open and in front', async () => {
    const shell = setup();
    await userEvent.click(await within(bar()).findByRole('button', { name: 'Notes' }));
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['notes']);
    const notes = within(bar()).getByRole('button', { name: 'Notes' });
    expect(notes).toHaveAttribute('aria-pressed', 'true');
    expect(notes).toHaveClass('open');
  });

  it('puts windows of unpinned programs after the pinned ones', async () => {
    const shell = setup();
    shell.desktop.open('logs');
    shell.desktop.open('about');
    await waitFor(() =>
      expect(names()).toEqual(['Parts', 'Files', 'Notes', 'Serial monitor', 'Logs', 'About Faber']),
    );
  });

  it('unpins a program from its taskbar button and saves it for every screen', async () => {
    const shell = setup();
    const parts = await within(bar()).findByRole('button', { name: 'Parts' });
    await fireEvent.contextMenu(parts, { clientX: 300, clientY: 580 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Unpin from taskbar' }));
    await waitFor(() =>
      expect(savedSettings(shell)?.taskbar.pinned).toEqual([
        'accessory:files',
        'accessory:notes',
        'tool:serial-monitor',
      ]),
    );
  });

  it("pins an open program from its window's taskbar button", async () => {
    const shell = setup();
    shell.desktop.open('logs');
    const logs = await within(bar()).findByRole('button', { name: 'Logs' });
    await fireEvent.contextMenu(logs, { clientX: 300, clientY: 580 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin to taskbar' }));
    await waitFor(() =>
      expect(savedSettings(shell)?.taskbar.pinned).toEqual([
        ...DEFAULT_SETTINGS.taskbar.pinned,
        'tool:logs',
      ]),
    );
  });

  it('switches to the large taskbar from its background menu', async () => {
    const shell = setup();
    const nav = screen.getByRole('navigation', { name: 'Taskbar' });
    await fireEvent.contextMenu(nav, { clientX: 900, clientY: 580 });
    const small = await screen.findByRole('menuitemradio', { name: 'Small taskbar' });
    expect(small).toHaveAttribute('aria-checked', 'true');
    await userEvent.click(
      screen.getByRole('menuitemradio', { name: 'Large taskbar, with labels' }),
    );
    await waitFor(() => expect(savedSettings(shell)?.taskbar.size).toBe('large'));
  });

  it('shows labels and gives the windows less room when large', async () => {
    const shell = setup({
      ...DEFAULT_SETTINGS,
      taskbar: { ...DEFAULT_SETTINGS.taskbar, size: 'large' },
    });
    const nav = screen.getByRole('navigation', { name: 'Taskbar' });
    await waitFor(() => expect(nav).toHaveClass('large'));
    expect(within(nav).getByText('Serial monitor')).toBeInTheDocument();
    // Desktop does not apply the height (App does); the controller is what counts.
    shell.desktop.setTaskbarHeight(64);
    expect(shell.desktop.area.height).toBe(1000 - 64);
  });

  it('steps through the windows of a pinned program with several open', async () => {
    const shell = setup();
    const a = shell.desktop.open('files', { key: 'a' });
    const b = shell.desktop.open('files', { key: 'b' });
    const files = await within(bar()).findByRole('button', { name: 'Files' });
    expect(shell.desktop.state.focusedId).toBe(b);
    await userEvent.click(files);
    expect(shell.desktop.state.focusedId).toBe(a);
    await userEvent.click(files);
    expect(shell.desktop.state.focusedId).toBe(b);
  });
});

describe('Opening files in Notes', () => {
  it('adds a tab to the Notes window that is already open, instead of a new window', async () => {
    const shell = setup();
    const enc = (s: string) => new TextEncoder().encode(s);
    shell.fetch.route({
      path: `/api/v0/files/content?path=${encodeURIComponent('home:/Desktop/a.txt')}`,
      bytes: enc('a'),
    });
    shell.fetch.route({
      path: `/api/v0/files/content?path=${encodeURIComponent('home:/Desktop/b.txt')}`,
      bytes: enc('b'),
    });
    openInNotes(shell.desktop, 'home:/Desktop/a.txt');
    await screen.findByRole('dialog', { name: 'a.txt · Notes' });
    const id = shell.desktop.state.windows[0]!.id;
    await waitFor(() => expect(shell.desktop.tabs[id]).toBeDefined());
    openInNotes(shell.desktop, 'home:/Desktop/b.txt');
    await waitFor(() =>
      expect(shell.desktop.tabs[id]!.state.tabs.map((t) => t.title)).toEqual(['a.txt', 'b.txt']),
    );
    expect(shell.desktop.state.windows).toHaveLength(1);
    // Opening one that is open goes to its tab.
    openInNotes(shell.desktop, 'home:/Desktop/a.txt');
    await waitFor(() => {
      const tabs = shell.desktop.tabs[id]!.state;
      expect(tabs.activeId).toBe(tabs.tabs[0]!.id);
    });
    expect(screen.getAllByRole('tab').map((t) => t.getAttribute('aria-selected'))).toEqual([
      'true',
      'false',
    ]);
    // Both files are now the newest recent files.
    expect(shell.desktop.recent.map((f) => f.name)).toEqual(['a.txt', 'b.txt']);
  });

  it("shows a pinned app by its name and icon, and opens the app's own window", async () => {
    const pinned = {
      ...DEFAULT_SETTINGS,
      taskbar: { ...DEFAULT_SETTINGS.taskbar, pinned: ['tool:parts', 'app:wall-follower'] },
    };
    const shell = setup(pinned);
    shell.pushApps([WALL_FOLLOWER]);
    const button = await within(bar()).findByRole('button', { name: 'Wall follower' });
    expect(button.querySelector('img')).toHaveAttribute('src', '/apps/wall-follower/icon.svg');
    await userEvent.click(button);
    const w = shell.desktop.state.windows.at(-1)!;
    expect([w.kind, w.key]).toEqual(['app', 'wall-follower']);
    expect(names()).toEqual(['Parts', 'Wall follower']);
    expect(within(bar()).getByRole('button', { name: 'Wall follower' })).toHaveClass('open');
  });

  it('pins an open app window from its taskbar button', async () => {
    const shell = setup();
    shell.pushApps([WALL_FOLLOWER]);
    shell.desktop.open('app', {
      key: 'wall-follower',
      title: 'Wall follower',
      params: { app: 'wall-follower' },
    });
    const button = await within(bar()).findByRole('button', { name: 'Wall follower' });
    await fireEvent.contextMenu(button, { clientX: 300, clientY: 580 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin to taskbar' }));
    await waitFor(() =>
      expect(savedSettings(shell)?.taskbar.pinned).toContain('app:wall-follower'),
    );
  });
});

describe('The user menu on the taskbar', () => {
  it('names who is signed in, opens their account and signs out', async () => {
    const shell = setup();
    shell.fetch.route({ method: 'DELETE', path: '/api/v0/session', status: 204 });
    const nav = screen.getByRole('navigation', { name: 'Taskbar' });
    await userEvent.click(within(nav).getByRole('button', { name: 'mikkel (Admin)' }));
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'Password and API tokens…' }),
    );
    // Settings opens at Users (and scrolls there).
    expect(await screen.findByRole('region', { name: 'Users' })).toBeInTheDocument();
    await userEvent.click(within(nav).getByRole('button', { name: 'mikkel (Admin)' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Sign out' }));
    await waitFor(() =>
      expect(shell.fetch.calls.map((c) => `${c.method} ${c.path}`)).toContain(
        'DELETE /api/v0/session',
      ),
    );
  });

  it('on the attached screen, offers to sign in instead', async () => {
    const shell = setup();
    shell.core.auth.info = {
      machineName: 'faber',
      language: 'en',
      needsSetup: true,
      signInOff: false,
      identity: { role: 'viewer', via: 'screen' },
    };
    const nav = screen.getByRole('navigation', { name: 'Taskbar' });
    await userEvent.click(
      await within(nav).findByRole('button', { name: 'Attached screen (Viewer)' }),
    );
    expect(screen.queryByRole('menuitem', { name: 'Sign out' })).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Create the admin…' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: 'Sign in as someone…' }));
    expect(shell.core.auth.prompt).toBe('sign-in');
  });
});
