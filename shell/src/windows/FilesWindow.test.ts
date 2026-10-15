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
      /Home\s*\/\s*Documents\s*\/\s*robot/,
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
      'Delete for good',
      'Properties',
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

  it('uploads dropped or picked files into the folder, never over an existing one', async () => {
    const shell = setup('home:/Documents', [
      listing('home:/Documents', [{ name: 'a.txt' }]),
      {
        method: 'PUT',
        path: '/api/v0/files/content?path=home%3A%2FDocuments%2Fa%202.txt',
        status: 204,
      },
    ]);
    await within(await screen.findByRole('listbox')).findByRole('option', { name: /a\.txt/ });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]')!;
    await userEvent.upload(input, new File(['hi'], 'a.txt', { type: 'text/plain' }));
    await waitFor(() => expect(shell.fetch.calls.some((c) => c.method === 'PUT')).toBe(true));
    expect(await screen.findByText(/a\.txt → a 2\.txt/)).toBeInTheDocument();
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
});
