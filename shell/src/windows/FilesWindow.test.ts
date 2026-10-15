import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { DEFAULT_SETTINGS, fakeShell, listing } from '../test/fakeShell';
import FilesWindow from './FilesWindow.svelte';

const HOME_ROOT = { id: 'home', label: 'Home', path: 'home:/' };
const MACHINE_ROOT = {
  id: 'machine',
  label: 'This computer',
  path: 'machine:/',
  warning: 'You are looking at the whole computer. Deleting files here is for good.',
};

function setup(path: string, routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell([{ path: '/api/v0/files/roots', body: [HOME_ROOT] }, ...routes]);
  shell.pushSettings();
  const id = shell.desktop.open('files', { params: { path } });
  const win = shell.desktop.state.windows.find((w) => w.id === id)!;
  render(FilesWindow, { props: { win, visible: true }, context: shell.context });
  return shell;
}

const list = () => screen.getByRole('listbox', { name: 'Files and folders' });

describe('Files', () => {
  it('lists a folder with sizes, and opens a subfolder with a double click', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [
        { name: 'robot', kind: 'folder' },
        { name: 'notes.txt', size: 2048, modifiedMs: Date.UTC(2026, 9, 15, 8, 0) },
      ]),
      listing('home:/Documents/robot', [{ name: 'wiring.md', size: 10 }]),
    ]);
    const notes = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /notes\.txt/,
    });
    expect(within(notes).getByText('2.0 KiB')).toBeInTheDocument();
    const robot = within(list()).getByRole('option', { name: /robot/ });
    await userEvent.dblClick(robot);
    expect(await within(list()).findByRole('option', { name: /wiring\.md/ })).toBeVisible();
    expect(screen.getByRole('navigation', { name: 'Folder path' })).toHaveTextContent(
      /Home\s*Documents\s*robot/,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(await within(list()).findByRole('option', { name: /notes\.txt/ })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Forward' })).toBeEnabled();
    expect(shell.desktop.state.windows[0]!.title).toBe('Documents · Files');
  });

  it('goes to the special folders from the sidebar', async () => {
    setup('home:/Documents', [
      listing('home:/Documents', []),
      listing('home:/Trash', [{ name: 'old.txt', originalPath: 'home:/Documents/old.txt' }]),
    ]);
    await userEvent.click(screen.getByRole('button', { name: 'Trash' }));
    const row = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /old\.txt/,
    });
    expect(within(row).getByText('home:/Documents/old.txt')).toBeInTheDocument();
  });

  it('makes a new folder and renames it in place', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }]),
      { method: 'POST', path: '/api/v0/files/folder', status: 201, text: '' },
      { method: 'POST', path: '/api/v0/files/move', status: 204 },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    shell.fetch.route(
      listing('home:/Documents', [{ name: 'New folder', kind: 'folder' }, { name: 'a.txt' }]),
    );
    await userEvent.click(screen.getByRole('button', { name: /New folder/ }));
    const input = await screen.findByRole('textbox', { name: 'New name for New folder' });
    await waitFor(() => expect(input).toHaveFocus());
    await userEvent.clear(input);
    await userEvent.type(input, 'robot{Enter}');
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/move'))?.body).toEqual({
        from: 'home:/Documents/New folder',
        to: 'home:/Documents/robot',
      }),
    );
    expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/folder'))?.body).toEqual({
      path: 'home:/Documents/New folder',
    });
  });

  it('moves the selected file to the Trash with Delete', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }]),
      { method: 'POST', path: '/api/v0/files/trash', status: 204 },
    ]);
    const a = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /a\.txt/,
    });
    await userEvent.click(a);
    await userEvent.keyboard('{Delete}');
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/trash'))?.body).toEqual({
        paths: ['home:/Documents/a.txt'],
      }),
    );
  });

  it('shows the core error with its hint when an action fails', async () => {
    const error = {
      code: 'protected',
      message: 'Recordings is one of the fixed folders.',
      hint: 'Move the files inside it instead.',
    };
    setup('home:/', [
      listing('home:/', [{ name: 'notes.txt' }]),
      { method: 'POST', path: '/api/v0/files/trash', status: 409, body: { error } },
    ]);
    const a = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /notes\.txt/,
    });
    await userEvent.click(a);
    await userEvent.keyboard('{Delete}');
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(error.message)).toBeInTheDocument();
    expect(within(alert).getByText(error.hint)).toBeInTheDocument();
  });

  it('asks before emptying the Trash', async () => {
    const shell = setup('home:/Trash', [
      listing('home:/Trash', [{ name: 'old.txt', originalPath: 'home:/Documents/old.txt' }]),
      { method: 'POST', path: '/api/v0/files/empty-trash', status: 204 },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /old\.txt/ });
    await userEvent.click(screen.getByRole('button', { name: 'Empty Trash' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Empty the Trash?' });
    expect(dialog).toHaveTextContent("can't be restored");
    await userEvent.click(within(dialog).getByRole('button', { name: 'Empty Trash' }));
    await waitFor(() =>
      expect(shell.fetch.calls.some((c) => c.path.endsWith('/files/empty-trash'))).toBe(true),
    );
  });

  it('restores from the Trash, and asks before deleting for good', async () => {
    const shell = setup('home:/Trash', [
      listing('home:/Trash', [{ name: 'old.txt', originalPath: 'home:/Documents/old.txt' }]),
      { method: 'POST', path: '/api/v0/files/restore', status: 204 },
      { method: 'POST', path: '/api/v0/files/delete', status: 204 },
    ]);
    const row = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /old\.txt/,
    });
    await fireEvent.contextMenu(row, { clientX: 50, clientY: 50 });
    // The menu itself is drawn by the desktop; here it is enough that Files built the right one.
    const menu = shell.menu.current!;
    expect(menu.entries.flatMap((e) => (e.kind === 'item' ? [e.label] : []))).toEqual([
      'Restore',
      'Properties',
      'Delete for good',
    ]);
    shell.menu.choose('restore');
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/restore'))?.body).toEqual({
        paths: ['home:/Trash/old.txt'],
      }),
    );
    await userEvent.click(row);
    await userEvent.keyboard('{Delete}');
    const dialog = screen.getByRole('alertdialog', { name: /Delete “old\.txt” for good\?/ });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete for good' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/delete'))?.body).toEqual({
        paths: ['home:/Trash/old.txt'],
      }),
    );
  });

  it('offers the sample recording in an empty Recordings folder', async () => {
    const shell = setup('home:/Recordings', [
      listing('home:/Recordings', []),
      {
        method: 'POST',
        path: '/api/v0/files/samples',
        status: 201,
        body: { path: 'home:/Recordings/ld19-sample.fbcap' },
      },
    ]);
    expect(await screen.findByText('No recordings yet')).toBeInTheDocument();
    shell.fetch.route(listing('home:/Recordings', [{ name: 'ld19-sample.fbcap', size: 90000 }]));
    await userEvent.click(screen.getByRole('button', { name: 'Add sample recordings' }));
    expect(
      await within(await screen.findByRole('listbox')).findByRole('option', {
        name: /ld19-sample\.fbcap/,
      }),
    ).toBeVisible();
  });

  it('opens a recording as an explanation of its replay port', async () => {
    const shell = setup('home:/Recordings', [
      listing('home:/Recordings', [{ name: 'ld19-sample.fbcap', size: 90000 }]),
    ]);
    const rec = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /ld19-sample/,
    });
    await userEvent.dblClick(rec);
    const props = shell.desktop.state.windows.find((w) => w.kind === 'properties');
    expect(props?.params.path).toBe('home:/Recordings/ld19-sample.fbcap');
  });

  const put = (name: string, mode?: 'create') =>
    `/api/v0/files/content?path=${encodeURIComponent(`home:/Documents/${name}`)}${mode ? '&mode=create' : ''}`;
  const EXISTS = {
    status: 409,
    body: { error: { code: 'already-exists', message: 'It is already there.', hint: 'Pick.' } },
  };

  it('uploads a new file without overwriting anything', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', []),
      { method: 'PUT', path: put('b.txt', 'create'), status: 204 },
    ]);
    await screen.findByText(/This folder is empty/);
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await userEvent.upload(input, new File(['hi'], 'b.txt', { type: 'text/plain' }));
    await waitFor(() =>
      expect(shell.fetch.calls.filter((c) => c.method === 'PUT').map((c) => c.path)).toEqual([
        put('b.txt', 'create'),
      ]),
    );
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('asks when the name is taken, and keeps both under a new name', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }]),
      { method: 'PUT', path: put('a.txt', 'create'), ...EXISTS },
      { method: 'PUT', path: put('a 2.txt', 'create'), status: 204 },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await userEvent.upload(input, new File(['hi'], 'a.txt', { type: 'text/plain' }));
    const dialog = await screen.findByRole('alertdialog', {
      name: '“a.txt” is already in Documents',
    });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Keep both' }));
    expect(await screen.findByText(/a\.txt → a 2\.txt/)).toBeInTheDocument();
    expect(shell.fetch.calls.filter((c) => c.method === 'PUT').map((c) => c.path)).toEqual([
      put('a.txt', 'create'),
      put('a 2.txt', 'create'),
    ]);
  });

  it('replaces the file that is there only when asked to', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }]),
      { method: 'PUT', path: put('a.txt', 'create'), ...EXISTS },
      { method: 'PUT', path: put('a.txt'), status: 204 },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await userEvent.upload(input, new File(['hi'], 'a.txt', { type: 'text/plain' }));
    const dialog = await screen.findByRole('alertdialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Replace' }));
    expect(await screen.findByText('Replaced a.txt.')).toBeInTheDocument();
    expect(shell.fetch.calls.filter((c) => c.method === 'PUT').map((c) => c.path)).toEqual([
      put('a.txt', 'create'),
      put('a.txt'),
    ]);
  });

  it('skips every taken name at once with "do the same for the others"', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }, { name: 'b.txt' }]),
      { method: 'PUT', path: put('a.txt', 'create'), ...EXISTS },
      { method: 'PUT', path: put('b.txt', 'create'), ...EXISTS },
      { method: 'PUT', path: put('c.txt', 'create'), status: 204 },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await userEvent.upload(input, [
      new File(['1'], 'a.txt'),
      new File(['2'], 'b.txt'),
      new File(['3'], 'c.txt'),
    ]);
    const dialog = await screen.findByRole('alertdialog');
    await userEvent.click(within(dialog).getByRole('checkbox'));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Skip' }));
    expect(await screen.findByText('Skipped a.txt, b.txt.')).toBeInTheDocument();
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(shell.fetch.calls.filter((c) => c.method === 'PUT').map((c) => c.path)).toEqual([
      put('a.txt', 'create'),
      put('b.txt', 'create'),
      put('c.txt', 'create'),
    ]);
  });

  it('shows This computer with its warning only when developer settings allow it', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', []),
      listing('machine:/', [{ name: 'C:', kind: 'folder' }]),
    ]);
    expect(screen.queryByRole('button', { name: 'This computer' })).not.toBeInTheDocument();
    shell.fetch.route({ path: '/api/v0/files/roots', body: [HOME_ROOT, MACHINE_ROOT] });
    shell.pushSettings({ ...DEFAULT_SETTINGS, developer: { fullMachineFiles: true } });
    await userEvent.click(await screen.findByRole('button', { name: 'This computer' }));
    expect(await screen.findByRole('note')).toHaveTextContent(MACHINE_ROOT.warning);
  });

  it('refreshes when the core says the folder changed', async () => {
    const shell = setup('home:/Documents', [listing('home:/Documents', [{ name: 'a.txt' }])]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    shell.fetch.route(listing('home:/Documents', [{ name: 'a.txt' }, { name: 'b.txt' }]));
    shell.filesChanged(['home:/']);
    expect(await within(list()).findByRole('option', { name: /b\.txt/ })).toBeVisible();
  });

  it('zooms from details to small and large icons with Ctrl and the wheel, and Ctrl 0', async () => {
    setup('home:/Documents', [listing('home:/Documents', [{ name: 'a.txt' }])]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    const details = screen.getByRole('radio', { name: 'Details' });
    expect(details).toBeChecked();
    expect(screen.getByRole('complementary', { name: 'Details' })).toBeInTheDocument();
    const scroll = list().parentElement!;
    await fireEvent.wheel(scroll, { deltaY: -100, ctrlKey: true });
    expect(screen.getByRole('radio', { name: 'Small icons' })).toBeChecked();
    await fireEvent.wheel(scroll, { deltaY: -100, ctrlKey: true });
    expect(screen.getByRole('radio', { name: 'Large icons' })).toBeChecked();
    expect(screen.queryByRole('complementary', { name: 'Details' })).not.toBeInTheDocument();
    // Without Ctrl the wheel just scrolls.
    await fireEvent.wheel(scroll, { deltaY: 100 });
    expect(screen.getByRole('radio', { name: 'Large icons' })).toBeChecked();
    await fireEvent.keyDown(list(), { key: '-', ctrlKey: true });
    expect(screen.getByRole('radio', { name: 'Small icons' })).toBeChecked();
    await fireEvent.keyDown(list(), { key: '0', ctrlKey: true });
    expect(details).toBeChecked();
  });

  it('shows the selected file in the details panel', async () => {
    setup('home:/Recordings', [listing('home:/Recordings', [{ name: 'bench.fbcap', size: 4096 }])]);
    const rec = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /bench\.fbcap/,
    });
    await userEvent.click(rec);
    const panel = screen.getByRole('complementary', { name: 'Details' });
    expect(panel).toHaveTextContent('Recording');
    expect(panel).toHaveTextContent('replay:bench');
    expect(within(panel).getByRole('button', { name: 'Add a part on it' })).toBeInTheDocument();
    expect(document.querySelector('.status')).toHaveTextContent('1 item · 1 selected');
  });

  const DISK = { id: 'disk-e', label: 'SENNEP SD (E:)', path: 'disk-e:/' };
  const diskListing = (status = 200) => ({
    path: `/api/v0/files/list?path=${encodeURIComponent('disk-e:/')}`,
    status,
    body:
      status === 200
        ? {
            path: 'disk-e:/',
            entries: [{ name: 'log.txt', kind: 'file', size: 1, path: 'disk-e:/log.txt' }],
          }
        : { error: { code: 'not-found', message: 'disk-e:/ is not there.', hint: '' } },
  });

  it('lists connected disks under Disks, and none when nothing is plugged in', async () => {
    const shell = setup('home:/Documents', [listing('home:/Documents', [])]);
    await screen.findByText(/This folder is empty/);
    expect(screen.queryByText('Disks')).not.toBeInTheDocument();
    shell.fetch.route({ path: '/api/v0/files/roots', body: [HOME_ROOT, DISK] });
    shell.fetch.route(listing('disk-e:/', [{ name: 'log.txt' }]));
    // Plugging in a disk: the core names its root.
    shell.filesChanged(['disk-e:/']);
    const disks = await screen.findByRole('list', { name: 'Disks' });
    await userEvent.click(within(disks).getByRole('button', { name: 'SENNEP SD (E:)' }));
    expect(await within(list()).findByRole('option', { name: /log\.txt/ })).toBeVisible();
    expect(screen.getByText(/It has no Trash/)).toBeInTheDocument();
  });

  it('deletes for good on a disk, after asking, since disks have no Trash', async () => {
    const shell = setup('disk-e:/', [
      { path: '/api/v0/files/roots', body: [HOME_ROOT, DISK] },
      diskListing(),
      { method: 'POST', path: '/api/v0/files/delete', status: 204 },
    ]);
    const row = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /log\.txt/,
    });
    await fireEvent.contextMenu(row, { clientX: 50, clientY: 50 });
    const labels = shell.menu.current!.entries.flatMap((e) => (e.kind === 'item' ? [e.label] : []));
    expect(labels).toContain('Delete for good…');
    expect(labels).not.toContain('Move to Trash');
    shell.menu.choose('delete');
    const dialog = await screen.findByRole('alertdialog', { name: /Delete “log\.txt” for good\?/ });
    expect(dialog).toHaveTextContent('Disks have no Trash');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete for good' }));
    await waitFor(() =>
      expect(shell.fetch.calls.find((c) => c.path.endsWith('/files/delete'))?.body).toEqual({
        paths: ['disk-e:/log.txt'],
      }),
    );
    expect(shell.fetch.calls.some((c) => c.path.endsWith('/files/trash'))).toBe(false);
  });

  it('goes back home, saying why, when the disk it shows is pulled out', async () => {
    const shell = setup('disk-e:/', [
      { path: '/api/v0/files/roots', body: [HOME_ROOT, DISK] },
      listing('home:/Desktop', [{ name: 'todo.txt' }]),
      diskListing(),
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /log\.txt/ });
    shell.fetch.route(diskListing(404));
    shell.fetch.route({ path: '/api/v0/files/roots', body: [HOME_ROOT] });
    shell.filesChanged(['disk-e:/']);
    expect(await screen.findByText(/The disk was removed/)).toBeInTheDocument();
    expect(
      await within(await screen.findByRole('listbox')).findByRole('option', { name: /todo\.txt/ }),
    ).toBeVisible();
  });

  it('opens a folder in a new tab, and closes tabs', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'robot', kind: 'folder' }]),
      listing('home:/Documents/robot', [{ name: 'wiring.md' }]),
    ]);
    const robot = await within(await screen.findByRole('listbox')).findByRole('option', {
      name: /robot/,
    });
    await fireEvent.contextMenu(robot, { clientX: 50, clientY: 50 });
    shell.menu.choose('open-tab');
    const id = shell.desktop.state.windows[0]!.id;
    await waitFor(() =>
      expect(shell.desktop.tabs[id]!.state.tabs.map((t) => t.title)).toEqual([
        'Documents',
        'robot',
      ]),
    );
    expect(
      await within(await screen.findByRole('listbox')).findByRole('option', { name: /wiring\.md/ }),
    ).toBeVisible();
    expect(shell.desktop.state.windows[0]!.title).toBe('robot · Files');
    const tabs = shell.desktop.tabs[id]!;
    tabs.close(tabs.state.activeId!);
    await waitFor(() => expect(shell.desktop.state.windows[0]!.title).toBe('Documents · Files'));
  });
});
