import { describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell, part, simBase, simBattery } from '../test/fakeShell';
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

  it('shows the version installed now, also when another tab installs one meanwhile', async () => {
    const { shell } = setup([
      { method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: PREVIEW },
    ]);
    const dialog = await screen.findByTestId('install-package');
    await waitFor(() => expect(dialog).toHaveTextContent('Acme ranger'));
    expect(dialog).not.toHaveTextContent('Replaces version');
    shell.pushPackages([{ ...INFO, version: '0.0.9' }]);
    await waitFor(() => expect(dialog).toHaveTextContent('Replaces version 0.0.9.'));
    shell.pushPackages([]);
    await waitFor(() => expect(dialog).not.toHaveTextContent('Replaces version'));
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

describe('Install window for an app', () => {
  const APP = {
    id: 'wall-follower',
    name: 'Wall follower',
    version: '0.1.0',
    publisher: 'Faber',
    enabled: false,
    state: { kind: 'stopped', message: 'The app is turned off.' },
    logic: true,
    tickHz: 20,
    roles: [
      {
        role: 'lidar',
        capability: 'laser-scan@1',
        command: false,
        count: '1',
        parts: [],
        drive: 'logic',
      },
      {
        role: 'base',
        capability: 'planar-base@1',
        command: true,
        count: '1',
        parts: [],
        drive: 'logic',
      },
    ],
    publishes: [
      {
        name: 'wall-distance',
        capability: 'distance@1',
        address: 'app:wall-follower/wall-distance',
      },
    ],
    ui: '/apps/wall-follower/ui/index.html',
    window: { title: 'Wall follower', width: 420, height: 260, pinned: false },
  };
  const APP_PREVIEW = {
    app: APP,
    readme:
      '# Wall follower\n\nKeeps the base `0.3 m` from the wall.\n\n<img src=x onerror=alert(1)>',
    checked: false,
  };
  const APP_INFO = { ...INFO, id: 'wall-follower', name: 'Wall follower', kind: 'app' };

  function setupApp(routes: FakeRoute[] = []) {
    const s = setup([
      { method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: APP_PREVIEW },
      { method: 'POST', path: `/api/v0/packages?${NAME}`, status: 201, body: APP_INFO },
      ...routes,
    ]);
    s.shell.pushParts([part('lidar'), part('lidar2'), simBase('base'), simBase('other-base')]);
    return s;
  }

  it('shows what the app needs, what it may drive and what it publishes, before installing', async () => {
    setupApp();
    const dialog = await screen.findByTestId('install-package');
    await waitFor(() => expect(dialog).toHaveTextContent('Wall follower'));
    expect(dialog).toHaveTextContent('0.1.0');
    expect(dialog).toHaveTextContent('Faber');
    expect(dialog).toHaveTextContent('In the background, 20 times a second');
    expect(dialog).toHaveTextContent('Not checked by the Faber registry');
    // Each role offers only the parts with its capability.
    const lidar = screen.getByRole('combobox', { name: 'lidar' });
    expect([...lidar.querySelectorAll('option')].map((o) => o.textContent)).toEqual([
      'Pick later',
      'lidar',
      'lidar2',
    ]);
    const base = screen.getByRole('combobox', { name: 'base' });
    expect([...base.querySelectorAll('option')].map((o) => o.value)).toEqual([
      '',
      'base',
      'other-base',
    ]);
    expect(dialog).toHaveTextContent('Wall follower will be able to drive the part you pick here.');
    await userEvent.selectOptions(base, 'base');
    expect(dialog).toHaveTextContent('Wall follower will be able to drive "base".');
    // Its derived signal, marked as the app's.
    const publishes = screen.getByRole('region', { name: 'What it publishes' });
    expect(publishes).toHaveTextContent('app:wall-follower/wall-distance');
    expect(within(publishes).getByText('app')).toBeInTheDocument();
    // The README is text: its HTML never becomes an element.
    expect(dialog.querySelector('img')).toBeNull();
    expect(dialog).toHaveTextContent('<img src=x onerror=alert(1)>');
  });

  it('turns on only once every role has a part, then installs, binds and turns it on', async () => {
    const on = { ...APP, enabled: true, state: { kind: 'running' } };
    const { shell, win } = setupApp([
      { method: 'PUT', path: '/api/v0/apps/wall-follower/roles', body: APP },
      { method: 'POST', path: '/api/v0/apps/wall-follower/enable', body: on },
    ]);
    const turnOn = await screen.findByRole('checkbox', { name: 'Turn it on now' });
    expect(turnOn).toBeDisabled();
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'lidar' }), 'lidar2');
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'base' }), 'base');
    expect(turnOn).toBeEnabled();
    await userEvent.click(turnOn);
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    expect(await screen.findByText('Wall follower is installed')).toBeInTheDocument();
    await waitFor(() =>
      expect(shell.fetch.calls.map((c) => `${c.method} ${c.path}`)).toContain(
        'POST /api/v0/apps/wall-follower/enable',
      ),
    );
    const calls = shell.fetch.calls.filter((c) => c.method !== 'GET');
    expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
      `POST /api/v0/packages?mode=preview&${NAME}`,
      `POST /api/v0/packages?${NAME}`,
      'PUT /api/v0/apps/wall-follower/roles',
      'POST /api/v0/apps/wall-follower/enable',
    ]);
    expect(calls[2]!.body).toEqual({ roles: { lidar: 'lidar2', base: 'base' } });
    await userEvent.click(screen.getByRole('button', { name: 'Open its window' }));
    const appWin = shell.desktop.state.windows.find((w) => w.kind === 'app');
    expect(appWin).toMatchObject({ key: 'wall-follower', title: 'Wall follower' });
    expect(appWin?.rect).toMatchObject({ w: 420, h: 260 });
    expect(shell.desktop.state.windows.some((w) => w.id === win.id)).toBe(false);
  });

  it('says which roles are optional and turns on without them', async () => {
    const optional = {
      ...APP,
      roles: [
        ...APP.roles,
        {
          role: 'battery',
          capability: 'battery@1',
          command: false,
          count: '0..1',
          parts: [],
          drive: 'logic',
        },
        {
          role: 'proximity',
          capability: 'distance@1',
          command: false,
          count: '0..',
          parts: [],
          drive: 'logic',
        },
      ],
    };
    const { shell } = setup([
      {
        method: 'POST',
        path: `/api/v0/packages?mode=preview&${NAME}`,
        body: { ...APP_PREVIEW, app: optional },
      },
    ]);
    shell.pushParts([part('lidar'), simBase('base'), simBattery('pack'), simBattery('spare')]);
    const battery = await screen.findByRole('combobox', { name: 'battery' });
    expect([...battery.querySelectorAll('option')].map((o) => o.textContent)).toEqual([
      'None',
      'pack',
      'spare',
    ]);
    const roles = screen.getByRole('list', { name: 'Roles' });
    expect(roles).toHaveTextContent('one part, optional');
    expect(roles).toHaveTextContent('any number of parts, optional');
    await waitFor(() =>
      expect(screen.getByRole('checkbox', { name: 'Turn it on now' })).toBeEnabled(),
    );
  });

  it('suggests the only part that fits, and installs without turning on', async () => {
    const { shell } = setup([
      { method: 'POST', path: `/api/v0/packages?mode=preview&${NAME}`, body: APP_PREVIEW },
      { method: 'POST', path: `/api/v0/packages?${NAME}`, status: 201, body: APP_INFO },
      { method: 'PUT', path: '/api/v0/apps/wall-follower/roles', body: APP },
    ]);
    shell.pushParts([part('lidar'), simBase('base')]);
    const lidar = await screen.findByRole('combobox', { name: 'lidar' });
    await waitFor(() => expect(lidar).toHaveValue('lidar'));
    expect(screen.getByRole('combobox', { name: 'base' })).toHaveValue('base');
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    expect(await screen.findByText(/It is off. Turn it on in Apps/)).toBeInTheDocument();
    expect(shell.fetch.calls.some((c) => c.path.endsWith('/enable'))).toBe(false);
  });
});
