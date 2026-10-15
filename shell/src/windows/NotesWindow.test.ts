import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell } from '../test/fakeShell';
import NotesWindow from './NotesWindow.svelte';

const q = (path: string) => `?path=${encodeURIComponent(path)}`;
const enc = (s: string) => new TextEncoder().encode(s);

function setup(path: string | null, routes: Parameters<typeof fakeShell>[0] = []) {
  const shell = fakeShell(routes);
  const id = shell.desktop.open('notes', path ? { key: path, params: { path } } : {});
  const win = () => shell.desktop.state.windows.find((w) => w.id === id)!;
  render(NotesWindow, { props: { win: win(), visible: true }, context: shell.context });
  return { shell, win };
}

const editor = () => screen.getByRole('textbox', { name: /^Text of/ });
const sent = (shell: ReturnType<typeof fakeShell>) =>
  new TextDecoder().decode(shell.fetch.calls.find((c) => c.method === 'PUT')!.raw as Uint8Array);

describe('Notes', () => {
  it('opens a text file, marks unsaved changes and saves with Ctrl+S', async () => {
    const path = 'home:/Documents/todo.txt';
    const { shell, win } = setup(path, [
      { path: `/api/v0/files/content${q(path)}`, bytes: enc('solder\r\nheaders') },
      { method: 'PUT', path: `/api/v0/files/content${q(path)}`, status: 204 },
    ]);
    await waitFor(() => expect(editor()).toHaveValue('solder\nheaders'));
    expect(win().title).toBe('todo.txt · Notes');
    await userEvent.type(editor(), '!');
    expect(win().title).toBe('*todo.txt · Notes');
    expect(screen.getByText('Unsaved changes')).toBeInTheDocument();
    await userEvent.keyboard('{Control>}s{/Control}');
    await waitFor(() => expect(win().title).toBe('todo.txt · Notes'));
    // Written back with the Windows line endings the file had.
    expect(sent(shell)).toBe('solder\r\nheaders!');
  });

  it('keeps the text unsaved and shows the line when parts.toml is refused', async () => {
    const path = 'home:/System/parts.toml';
    const error = {
      code: 'config-invalid',
      message: 'parts.toml line 2: expected `=` after the key.',
      hint: 'Fix line 2, then save again.',
    };
    const { win } = setup(path, [
      { path: `/api/v0/files/content${q(path)}`, bytes: enc('[parts.lidar]\n') },
      { method: 'PUT', path: `/api/v0/files/content${q(path)}`, status: 422, body: { error } },
    ]);
    await waitFor(() => expect(editor()).toHaveValue('[parts.lidar]\n'));
    await userEvent.type(editor(), 'port');
    await userEvent.click(screen.getByRole('button', { name: /^Save$/ }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(error.message)).toBeInTheDocument();
    expect(within(alert).getByText(error.hint)).toBeInTheDocument();
    expect(win().title).toBe('*parts.toml · Notes');
    expect(editor()).toHaveValue('[parts.lidar]\nport');
  });

  it('refuses a binary file with a clear message', async () => {
    const path = 'home:/Documents/photo.jpg';
    setup(path, [
      { path: `/api/v0/files/content${q(path)}`, bytes: new Uint8Array([0xff, 0xd8, 0, 0x10]) },
    ]);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent("This file isn't text, so Notes can't open it.");
    expect(screen.queryByRole('textbox', { name: /^Text of/ })).not.toBeInTheDocument();
  });

  it('asks before closing with unsaved changes', async () => {
    const { shell, win } = setup(null);
    await userEvent.type(editor(), 'idea');
    shell.desktop.requestClose(win().id);
    const dialog = await screen.findByRole('alertdialog', { name: 'Save changes to Untitled?' });
    expect(shell.desktop.state.windows).toHaveLength(1);
    await userEvent.click(within(dialog).getByRole('button', { name: "Don't save" }));
    expect(shell.desktop.state.windows).toHaveLength(0);
  });

  it('closes at once when there is nothing to save', async () => {
    const { shell, win } = setup(null);
    shell.desktop.requestClose(win().id);
    expect(shell.desktop.state.windows).toHaveLength(0);
  });

  it('saves a new note with Save as, into the folder picked', async () => {
    const target = 'home:/Documents/plan.txt';
    const { shell, win } = setup(null, [
      {
        path: `/api/v0/files/list${q('home:/Documents')}`,
        body: { path: 'home:/Documents', entries: [] },
      },
      { method: 'PUT', path: `/api/v0/files/content${q(target)}`, status: 204 },
    ]);
    await userEvent.type(editor(), 'step 1');
    await userEvent.keyboard('{Control>}s{/Control}');
    const dialog = await screen.findByRole('alertdialog', { name: 'Save as' });
    const name = within(dialog).getByLabelText('File name');
    await userEvent.clear(name);
    await userEvent.type(name, 'plan.txt');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(win().title).toBe('plan.txt · Notes'));
    expect(sent(shell)).toBe('step 1');
    expect(win().params).toEqual({ path: target });
  });

  it('finds text with Ctrl+F and steps through the matches', async () => {
    setup(null);
    await userEvent.type(editor(), 'lidar, LIDAR and lidar');
    await fireEvent.keyDown(editor(), { key: 'f', ctrlKey: true });
    const find = await screen.findByRole('textbox', { name: 'Find' });
    await userEvent.type(find, 'lidar');
    expect(screen.getByText('3 found')).toBeInTheDocument();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByText('1 of 3')).toBeInTheDocument();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByText('2 of 3')).toBeInTheDocument();
    await userEvent.keyboard('{Shift>}{Enter}{/Shift}');
    expect(screen.getByText('1 of 3')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('checkbox', { name: 'Match case' }));
    expect(screen.getByText('2 found')).toBeInTheDocument();
  });

  it('shows the line and column of the cursor', async () => {
    setup(null);
    await userEvent.type(editor(), 'ab{Enter}c');
    expect(screen.getByText('Ln 2, Col 2')).toBeInTheDocument();
  });

  it('turns word wrap and the monospace font off and on', async () => {
    setup(null);
    const wrap = screen.getByRole('button', { name: 'Word wrap' });
    expect(wrap).toHaveAttribute('aria-pressed', 'true');
    await userEvent.click(wrap);
    expect(editor()).toHaveAttribute('wrap', 'off');
    await userEvent.click(screen.getByRole('button', { name: 'Monospace' }));
    expect(editor()).not.toHaveClass('mono');
  });
});
