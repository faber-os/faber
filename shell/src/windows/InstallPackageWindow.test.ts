import { describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell } from '../test/fakeShell';
import type { FakeRoute } from '../test/fakeFetch';
import { openEntry } from '../components/fileOps';
import { openDroppedPackage } from '../components/installWindow';
import InstallPackageWindow from './InstallPackageWindow.svelte';

const PATH = 'home:/Desktop/acme-ranger-0.1.0.faberpkg';
const DOWNLOAD = `/api/v0/files/content?path=${encodeURIComponent(PATH)}&download=1`;
const NAME = 'name=acme-ranger-0.1.0.faberpkg';

const PREVIEW = {
  connector: {
    id: 'acme-ranger',
    name: 'Acme ranger',
    version: '0.1.0',
    kind: 'wasm',
    port: { kind: 'serial', baud: 9600 },
    provides: [{ name: 'distance', capability: 'distance@1' }],
    settings: [],
  },
  publisher: 'Ada',
  readme: '# Acme ranger\n\nReads `D:123` lines.\n\n<script>alert(1)</script>\n\n- one\n- two',
  captures: ['sample.fbcap'],
  checked: false,
};

const INFO = {
  id: 'acme-ranger',
  name: 'Acme ranger',
  version: '0.1.0',
  kind: 'wasm',
  installedAt: '2026-10-15T10:00:00Z',
  source: 'acme-ranger-0.1.0.faberpkg',
  checked: false,
  usedBy: [],
};

function setup(
  routes: FakeRoute[],
  open = (shell: ReturnType<typeof fakeShell>) => {
    openEntry(shell.desktop, {
      kind: 'file',
      name: 'acme-ranger-0.1.0.faberpkg',
      path: PATH,
      size: 9,
    });
  },
) {
  const shell = fakeShell([{ path: DOWNLOAD, bytes: new Uint8Array([0x50, 0x4b]) }, ...routes]);
  open(shell);
  const win = shell.desktop.state.windows.at(-1)!;
  render(InstallPackageWindow, { props: { win, visible: true }, context: shell.context });
  return { shell, win };
}

describe('Install window', () => {
  it('opens from a package in Files and shows what it is, what it can touch and its README', async () => {
    const { shell, win } = setup([
      { method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: PREVIEW },
    ]);
    expect(win.kind).toBe('install-package');
    expect(win.title).toBe('Install acme-ranger-0.1.0.faberpkg');
    const dialog = await screen.findByTestId('install-package');
    await waitFor(() => expect(dialog).toHaveTextContent('Acme ranger'));
    expect(dialog).toHaveTextContent('Code, run sandboxed (WebAssembly)');
    expect(dialog).toHaveTextContent('0.1.0');
    expect(dialog).toHaveTextContent('Ada');
    expect(dialog).toHaveTextContent('A serial port you pick, at 9600 baud');
    expect(dialog).toHaveTextContent('Nothing else: no files, no network');
    expect(dialog).toHaveTextContent('Not checked by the Faber registry');
    // The README as safe Markdown: its script is text, never an element.
    expect(screen.getByText('D:123').tagName).toBe('CODE');
    expect(dialog).toHaveTextContent('<script>alert(1)</script>');
    expect(dialog.querySelector('script')).toBeNull();
    expect(screen.getAllByRole('listitem').map((li) => li.textContent)).toContain('two');
    // The file went to the core as it is, for the core to check.
    const preview = shell.fetch.calls.find((c) => c.path.includes('mode=preview'));
    expect(preview?.raw).toBeInstanceOf(Blob);
  });

  it('installs it, then offers to add a part with the new connector', async () => {
    const { shell, win } = setup([
      { method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: PREVIEW },
      { method: 'POST', path: `/api/v0/packages?${NAME}`, status: 201, body: INFO },
    ]);
    shell.fetch.route({ path: '/api/v0/packages', body: [INFO] });
    await userEvent.click(await screen.findByRole('button', { name: 'Install' }));
    expect(await screen.findByText('Acme ranger is installed')).toBeInTheDocument();
    expect(shell.core.packages).toEqual([INFO]);
    await userEvent.click(screen.getByRole('button', { name: 'Add part' }));
    const parts = shell.desktop.state.windows.find((w) => w.kind === 'parts');
    expect(parts?.params).toEqual({ addConnector: 'acme-ranger' });
    expect(shell.desktop.state.windows.some((w) => w.id === win.id)).toBe(false);
  });

  it('says why a package can not be installed, and offers no Install button', async () => {
    const error = {
      code: 'package-not-loadable',
      message: 'Acme ranger asks for wasi:cli/environment, which Faber doesn’t give connectors.',
      hint: 'Build it for wasm32-unknown-unknown, not with WASI.',
    };
    setup([
      {
        method: 'POST',
        path: `/api/v0/packages?mode=preview&${NAME}`,
        status: 422,
        body: { error },
      },
    ]);
    expect(await screen.findByText(error.message)).toBeInTheDocument();
    expect(screen.getByText(error.hint)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install' })).toBeNull();
  });

  it('installs a package dropped from this computer without saving it in Files', async () => {
    const file = new File([new Uint8Array([0x50, 0x4b])], 'acme-ranger-0.1.0.faberpkg');
    const { shell } = setup(
      [{ method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: PREVIEW }],
      (s) => openDroppedPackage(s.desktop, file),
    );
    await screen.findByRole('heading', { name: 'Acme ranger', level: 3 });
    expect(shell.fetch.calls.some((c) => c.path.startsWith('/api/v0/files'))).toBe(false);
    expect(shell.fetch.calls.find((c) => c.path.includes('mode=preview'))?.raw).toBe(file);
  });
});
