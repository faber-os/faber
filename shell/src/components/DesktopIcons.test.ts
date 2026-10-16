import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, listing } from '../test/fakeShell';
import { WINDOW_DEFS } from '../windows/registry';
import Desktop from './Desktop.svelte';

function setup(routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell([listing('home:/Desktop', [{ name: 'todo.txt', size: 12 }]), ...routes]);
  shell.pushSettings();
  render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
  return shell;
}

const desktopList = () => screen.getByRole('listbox', { name: 'Desktop' });
const icon = (name: string | RegExp) => within(desktopList()).getByRole('option', { name });

describe('Desktop icons', () => {
  it('shows the pinned tools and accessories, and the files in Desktop', async () => {
    setup();
    for (const name of ['Parts', 'Serial monitor', 'Files', 'Notes', 'Settings']) {
      expect(icon(name)).toBeInTheDocument();
    }
    expect(await within(desktopList()).findByRole('option', { name: 'todo.txt' })).toBeVisible();
  });

  it('selects on the first click and opens on the second', async () => {
    // Two clicks count as an open only within 450 ms; a loaded test machine can be slower
    // than that between userEvent clicks, so time stands still here.
    const now = vi.spyOn(performance, 'now').mockReturnValue(1000);
    const shell = setup();
    await userEvent.click(icon('Settings'));
    expect(icon('Settings')).toHaveAttribute('aria-selected', 'true');
    expect(shell.desktop.state.windows).toHaveLength(0);
    await userEvent.click(icon('Settings'));
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['settings']);
    now.mockRestore();
  });

  it('opens a text file on the desktop in Notes with Enter', async () => {
    const shell = setup();
    const file = await within(desktopList()).findByRole('option', { name: 'todo.txt' });
    await userEvent.click(file);
    await userEvent.keyboard('{Enter}');
    const notes = shell.desktop.state.windows.find((w) => w.kind === 'notes');
    expect(notes?.params).toEqual({ path: 'home:/Desktop/todo.txt' });
  });

  it('moves an icon by dragging and saves where it went', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    const parts = icon('Parts');
    await fireEvent.pointerDown(parts, { button: 0, clientX: 30, clientY: 30, pointerId: 1 });
    await fireEvent.pointerMove(parts, { clientX: 230, clientY: 230, pointerId: 1 });
    await fireEvent.pointerUp(parts, { clientX: 230, clientY: 230, pointerId: 1 });
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'PUT')).toBe(true));
    const put = shell.fetch.calls.find((c) => c.method === 'PUT')!.body as typeof DEFAULT_SETTINGS;
    // 200 px right and down from the first cell (12, 12) snaps to the cell at (196, 204).
    expect(put.desktop.icons.find((p) => p.key === 'tool:parts')).toEqual({
      key: 'tool:parts',
      x: 196,
      y: 204,
    });
    expect(put.developer).toEqual(DEFAULT_SETTINGS.developer);
  });

  it('places icons where the settings say', async () => {
    const shell = setup();
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      desktop: { icons: [{ key: 'accessory:notes', x: 380, y: 300 }], removed: [] },
    });
    await waitFor(() => expect(icon('Notes').style.left).toBe('380px'));
    expect(icon('Notes').style.top).toBe('300px');
  });

  it('has an icon for every program, also those a saved layout never had', async () => {
    const shell = setup();
    // A layout saved by an older Faber, before World, the flasher and Apps existed.
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      desktop: { icons: [{ key: 'tool:parts', x: 12, y: 12 }], removed: [] },
    });
    for (const d of WINDOW_DEFS.filter((d) => d.program)) {
      expect(icon(d.title), d.kind).toBeInTheDocument();
    }
    expect(icon('World')).toBeInTheDocument();
    expect(icon('Arduino flasher')).toBeInTheDocument();
    expect(icon('Apps')).toBeInTheDocument();
    expect(icon('Parts').style.left).toBe('12px');
  });

  it('removes a program from the desktop for good, and only that one', async () => {
    const saved = { ...DEFAULT_SETTINGS, desktop: { icons: [], removed: ['tool:logs'] } };
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: saved }]);
    await fireEvent.contextMenu(icon('Logs'), { clientX: 50, clientY: 50 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove from desktop' }));
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'PUT')).toBe(true));
    const put = shell.fetch.calls.find((c) => c.method === 'PUT')!.body as typeof DEFAULT_SETTINGS;
    expect(put.desktop.removed).toEqual(['tool:logs']);
    expect(within(desktopList()).queryByRole('option', { name: 'Logs' })).toBeNull();
    shell.pushSettings(put);
    expect(within(desktopList()).queryByRole('option', { name: 'Logs' })).toBeNull();
    expect(icon('Parts')).toBeInTheDocument();
  });

  it('lists the Desktop folder again when the core says it changed', async () => {
    const shell = setup();
    await within(desktopList()).findByRole('option', { name: 'todo.txt' });
    shell.fetch.route(listing('home:/Desktop', [{ name: 'wiring.md' }]));
    shell.filesChanged(['home:/Desktop']);
    expect(await within(desktopList()).findByRole('option', { name: 'wiring.md' })).toBeVisible();
  });
});

describe('Context menus', () => {
  it('opens on right-click on the desktop, with keyboard navigation and Escape', async () => {
    setup();
    await fireEvent.contextMenu(desktopList(), { clientX: 400, clientY: 300 });
    const menu = await screen.findByRole('menu', { name: 'Desktop' });
    const items = within(menu).getAllByRole('menuitem');
    expect(items.map((i) => i.textContent?.trim())).toEqual([
      'New folder',
      'New text file',
      'Line up icons',
      'Change wallpaper…',
      'Refresh',
    ]);
    await userEvent.keyboard('{ArrowDown}');
    await waitFor(() => expect(items[0]).toHaveFocus());
    await userEvent.keyboard('{ArrowDown}');
    expect(items[1]).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('runs the chosen item: Change wallpaper opens Settings', async () => {
    const shell = setup();
    await fireEvent.contextMenu(desktopList(), { clientX: 10, clientY: 10 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Change wallpaper…' }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['settings']);
  });

  it("shows what was right-clicked at the top of a file's menu, with Move to Trash last", async () => {
    setup();
    const file = await within(desktopList()).findByRole('option', { name: 'todo.txt' });
    await fireEvent.contextMenu(file, { clientX: 50, clientY: 50 });
    const menu = await screen.findByRole('menu', { name: 'todo.txt' });
    expect(menu.querySelector('.header')).toHaveTextContent('todo.txt 12 B · Desktop');
    const items = within(menu).getAllByRole('menuitem');
    expect(items.at(-1)).toHaveTextContent('Move to Trash');
    expect(items.at(-1)).toHaveClass('danger');
    expect(within(menu).getByRole('menuitem', { name: 'Rename' })).toHaveAttribute(
      'aria-keyshortcuts',
      'F2',
    );
  });

  it('pins a program to the taskbar from its desktop icon', async () => {
    const shell = setup([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    await fireEvent.contextMenu(icon('Settings'), { clientX: 50, clientY: 50 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin to taskbar' }));
    await waitFor(() =>
      expect(
        (shell.fetch.calls.find((c) => c.method === 'PUT')?.body as typeof DEFAULT_SETTINGS).taskbar
          .pinned,
      ).toContain('accessory:settings'),
    );
  });

  it('opens from the keyboard with Shift+F10 on an icon, with the first item focused', async () => {
    const shell = setup();
    const file = await within(desktopList()).findByRole('option', { name: 'todo.txt' });
    await userEvent.click(file);
    await userEvent.keyboard('{Shift>}{F10}{/Shift}');
    const menu = await screen.findByRole('menu', { name: 'todo.txt' });
    await waitFor(() => expect(within(menu).getByRole('menuitem', { name: 'Open' })).toHaveFocus());
    await userEvent.keyboard('{Enter}');
    expect(shell.desktop.state.windows.map((w) => w.kind)).toEqual(['notes']);
  });

  it('shows why an item is disabled and does nothing when it is chosen', async () => {
    const shell = setup();
    shell.fetch.route(
      listing('home:/Desktop', [{ name: 'Documents', kind: 'folder', special: 'documents' }]),
    );
    shell.filesChanged(['home:/Desktop']);
    const folder = await within(desktopList()).findByRole('option', { name: 'Documents' });
    await fireEvent.contextMenu(folder, { clientX: 50, clientY: 50 });
    const rename = await screen.findByRole('menuitem', { name: /Rename/ });
    expect(rename).toHaveAttribute('aria-disabled', 'true');
    expect(rename).toHaveAccessibleDescription(/fixed folders/);
    await userEvent.click(rename);
    expect(screen.getByRole('menu')).toBeInTheDocument();
  });

  it('has Minimise, Maximise and Close on a window title bar', async () => {
    const shell = setup();
    shell.desktop.open('about');
    const dialog = await screen.findByRole('dialog', { name: 'About Faber' });
    await fireEvent.contextMenu(within(dialog).getByRole('toolbar'), { clientX: 100, clientY: 20 });
    const menu = await screen.findByRole('menu');
    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((i) => i.textContent?.trim()),
    ).toEqual(['Minimise', 'Maximise', 'Close']);
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'Close' }));
    expect(shell.desktop.state.windows).toHaveLength(0);
  });

  it('has window actions on taskbar buttons', async () => {
    const shell = setup();
    shell.desktop.open('about');
    const task = await within(
      screen.getByRole('list', { name: 'Programs and windows' }),
    ).findByRole('button', {
      name: /About Faber/,
    });
    await fireEvent.contextMenu(task, { clientX: 100, clientY: 580 });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Minimise' }));
    expect(shell.desktop.state.windows[0]!.minimised).toBe(true);
  });
});

describe('Selecting several icons', () => {
  function setupFiles(routes: Parameters<typeof fakeShell>[0] = []) {
    const shell = fakeShell([
      listing('home:/Desktop', [
        { name: 'a.txt', size: 1 },
        { name: 'b.txt', size: 1 },
      ]),
      ...routes,
    ]);
    shell.pushSettings({
      ...DEFAULT_SETTINGS,
      desktop: {
        icons: [
          { key: 'home:/Desktop/a.txt', x: 380, y: 12 },
          { key: 'home:/Desktop/b.txt', x: 380, y: 108 },
        ],
        removed: [],
      },
    });
    render(Desktop, { props: { defs: WINDOW_DEFS }, context: shell.context });
    return shell;
  }
  const selected = () =>
    within(desktopList())
      .getAllByRole('option')
      .filter((o) => o.getAttribute('aria-selected') === 'true')
      .map((o) => o.textContent?.trim());

  it('adds to the selection with Ctrl+click and Shift+click', async () => {
    const user = userEvent.setup();
    setup();
    await user.click(icon('Parts'));
    await user.keyboard('{Control>}');
    await user.click(icon('Files'));
    await user.keyboard('{/Control}');
    expect(selected()).toEqual(expect.arrayContaining(['Parts', 'Files']));
    expect(selected()).toHaveLength(2);
    await user.click(icon('Notes'));
    expect(selected()).toEqual(['Notes']);
  });

  it('selects the icons a rubber band is drawn over', async () => {
    setupFiles();
    await within(desktopList()).findByRole('option', { name: 'b.txt' });
    const list = desktopList();
    // From an empty spot above the two files, down past both of them.
    await fireEvent.pointerDown(list, { button: 0, clientX: 370, clientY: 5, pointerId: 1 });
    await fireEvent.pointerMove(list, { clientX: 420, clientY: 150, pointerId: 1 });
    expect(screen.getByTestId('rubber-band')).toBeInTheDocument();
    await fireEvent.pointerUp(list, { clientX: 420, clientY: 150, pointerId: 1 });
    expect(screen.queryByTestId('rubber-band')).toBeNull();
    expect(selected()).toEqual(['a.txt', 'b.txt']);
  });

  it('moves the selected icons together', async () => {
    const user = userEvent.setup();
    const shell = setupFiles([{ method: 'PUT', path: '/api/v0/settings', body: DEFAULT_SETTINGS }]);
    const a = await within(desktopList()).findByRole('option', { name: 'a.txt' });
    await user.click(a);
    await user.keyboard('{Control>}');
    await user.click(icon('b.txt'));
    await user.keyboard('{/Control}');
    await fireEvent.pointerDown(a, { button: 0, clientX: 400, clientY: 30, pointerId: 1 });
    await fireEvent.pointerMove(a, { clientX: 584, clientY: 30, pointerId: 1 });
    await fireEvent.pointerUp(a, { clientX: 584, clientY: 30, pointerId: 1 });
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'PUT')).toBe(true));
    const put = shell.fetch.calls.find((c) => c.method === 'PUT')!.body as typeof DEFAULT_SETTINGS;
    const at = (key: string) => put.desktop.icons.find((p) => p.key === key);
    expect(at('home:/Desktop/a.txt')).toEqual({ key: 'home:/Desktop/a.txt', x: 564, y: 12 });
    expect(at('home:/Desktop/b.txt')).toEqual({ key: 'home:/Desktop/b.txt', x: 564, y: 108 });
  });

  it('opens every selected icon with Enter, and trashes the selected files with Delete', async () => {
    const user = userEvent.setup();
    const shell = setupFiles([{ method: 'POST', path: '/api/v0/files/trash', status: 204 }]);
    const a = await within(desktopList()).findByRole('option', { name: 'a.txt' });
    await user.click(a);
    await user.keyboard('{Control>}');
    await user.click(icon('b.txt'));
    await user.click(icon('Settings'));
    await user.keyboard('{/Control}');
    await user.keyboard('{Enter}');
    // Both text files open in Notes (as its tabs), and Settings opens too.
    expect(shell.desktop.state.windows.map((w) => w.kind).sort()).toEqual(['notes', 'settings']);

    await user.click(icon('a.txt'));
    await user.keyboard('{Control>}');
    await user.click(icon('b.txt'));
    await user.click(icon('Settings'));
    await user.keyboard('{/Control}');
    await user.keyboard('{Delete}');
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path === '/api/v0/files/trash')?.body).toEqual({
        paths: ['home:/Desktop/a.txt', 'home:/Desktop/b.txt'],
      }),
    );
  });
});

describe('Dropping files on the desktop', () => {
  const put = (path: string, mode?: 'create') =>
    `/api/v0/files/content?path=${encodeURIComponent(`home:/Desktop/${path}`)}${mode ? '&mode=create' : ''}`;
  const EXISTS = {
    status: 409,
    body: { error: { code: 'already-exists', message: 'It is already there.', hint: 'Pick.' } },
  };
  const file = (name: string, text = 'hi') => new File([text], name, { type: 'text/plain' });
  const drop = (dataTransfer: unknown) =>
    fireEvent.drop(desktopList(), {
      dataTransfer: { types: ['Files'], ...(dataTransfer as object) },
    });
  const puts = (shell: ReturnType<typeof fakeShell>) =>
    shell.fetch.calls
      .filter((c) => c.method === 'PUT' && c.path.includes('/files/'))
      .map((c) => c.path);

  it('uploads every dropped file to the desktop, asking about a name that is taken', async () => {
    const shell = setup([
      { method: 'PUT', path: put('new.txt', 'create'), status: 204 },
      { method: 'PUT', path: put('todo.txt', 'create'), ...EXISTS },
      { method: 'PUT', path: put('todo 2.txt', 'create'), status: 204 },
    ]);
    await within(desktopList()).findByRole('option', { name: 'todo.txt' });
    await drop({ files: [file('new.txt'), file('todo.txt')] });
    const dialog = await screen.findByRole('alertdialog', {
      name: '“todo.txt” is already in Desktop',
    });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Keep both' }));
    await waitFor(() =>
      expect(puts(shell)).toEqual([
        put('new.txt', 'create'),
        put('todo.txt', 'create'),
        put('todo 2.txt', 'create'),
      ]),
    );
    expect(await screen.findByText(/todo\.txt → todo 2\.txt/)).toBeInTheDocument();
  });

  it('opens a dropped connector package in Install, and uploads nothing for it', async () => {
    const shell = setup();
    await drop({ files: [new File([new Uint8Array([1])], 'acme-0.1.0.faberpkg')] });
    await waitFor(() =>
      expect(shell.desktop.state.windows.map((w) => w.kind)).toContain('install-package'),
    );
    expect(puts(shell)).toEqual([]);
  });

  it('uploads a dropped folder with what is in it', async () => {
    const shell = setup([
      { method: 'POST', path: '/api/v0/files/folder', status: 204 },
      { method: 'PUT', path: put('photos/a.txt', 'create'), status: 204 },
    ]);
    const a = file('a.txt');
    const fileEntry = {
      isFile: true,
      isDirectory: false,
      name: 'a.txt',
      file: (ok: (f: File) => void) => ok(a),
    };
    let read = false;
    const dir = {
      isFile: false,
      isDirectory: true,
      name: 'photos',
      createReader: () => ({
        readEntries: (ok: (e: unknown[]) => void) => {
          ok(read ? [] : [fileEntry]);
          read = true;
        },
      }),
    };
    await drop({ files: [], items: [{ kind: 'file', webkitGetAsEntry: () => dir }] });
    await waitFor(() => expect(puts(shell)).toEqual([put('photos/a.txt', 'create')]));
    const folder = shell.fetch.calls.find((c) => c.path === '/api/v0/files/folder');
    expect(folder?.body).toEqual({ path: 'home:/Desktop/photos' });
  });

  it('says so when the browser gives a folder it can not read', async () => {
    const shell = setup();
    const folder = new File([], 'photos');
    // A browser without folder entries hands over a folder as a File that can't be read.
    Object.defineProperty(folder, 'slice', {
      value: () => ({ arrayBuffer: () => Promise.reject(new Error('NotFoundError')) }),
    });
    await drop({ files: [folder] });
    expect(await screen.findByText(/Folders can't be dropped here/)).toBeInTheDocument();
    expect(puts(shell)).toEqual([]);
  });
});
