import { describe, expect, it } from 'vitest';
import { fakeFetch } from '../../test/fakeFetch';
import { CoreError, createHttp } from './http';

const part = {
  name: 'demo',
  connector: 'ldrobot-ld19',
  port: 'replay:ld19-sample',
  state: { kind: 'starting' },
  signals: [],
  mount: { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 },
};

describe('createHttp', () => {
  it('gets the system info', async () => {
    const f = fakeFetch([
      { path: '/api/v0/system', body: { name: 'Faber', version: '0.1.0', timeUs: 12 } },
    ]);
    const http = createHttp(f.fetch);
    await expect(http.getSystem()).resolves.toEqual({
      name: 'Faber',
      version: '0.1.0',
      timeUs: 12,
    });
  });

  it('lists connectors, ports and parts', async () => {
    const f = fakeFetch([
      { path: '/api/v0/connectors', body: [{ id: 'ldrobot-ld19' }] },
      { path: '/api/v0/ports', body: [{ id: 'replay:ld19-sample' }] },
      { path: '/api/v0/parts', body: [part] },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.getConnectors()).toHaveLength(1);
    expect(await http.getPorts()).toHaveLength(1);
    expect(await http.getParts()).toEqual([part]);
  });

  it('drops optional fields that arrive as null', async () => {
    const f = fakeFetch([
      { path: '/api/v0/ports', body: [{ id: 'serial:COM1', claimedBy: null, suggestions: [] }] },
    ]);
    const [port] = await createHttp(f.fetch).getPorts();
    expect(port && 'claimedBy' in port).toBe(false);
  });

  it('posts a new part as JSON and returns the created part', async () => {
    const f = fakeFetch([{ method: 'POST', path: '/api/v0/parts', status: 201, body: part }]);
    const http = createHttp(f.fetch);
    const created = await http.addPart({
      name: 'demo',
      connector: 'ldrobot-ld19',
      port: 'replay:ld19-sample',
    });
    expect(created.name).toBe('demo');
    expect(f.calls[0]).toEqual({
      method: 'POST',
      path: '/api/v0/parts',
      body: { name: 'demo', connector: 'ldrobot-ld19', port: 'replay:ld19-sample' },
    });
  });

  it('previews, installs and uninstalls packages, sending the file as raw bytes', async () => {
    const preview = { connector: { id: 'acme' }, readme: '# Acme', captures: [], checked: false };
    const info = { id: 'acme', version: '0.1.0', usedBy: [] };
    const f = fakeFetch([
      {
        method: 'POST',
        path: '/api/v0/packages?mode=preview&name=acme-0.1.0.faberpkg',
        body: preview,
      },
      {
        method: 'POST',
        path: '/api/v0/packages?name=acme-0.1.0.faberpkg',
        status: 201,
        body: info,
      },
      { path: '/api/v0/packages', body: [info] },
      { method: 'DELETE', path: '/api/v0/packages/acme', status: 204 },
      { path: '/api/v0/connectors/problems', body: [{ folder: 'x', message: 'm', hint: 'h' }] },
    ]);
    const http = createHttp(f.fetch);
    const file = new Uint8Array([0x50, 0x4b]);
    expect(await http.previewPackage(file, 'acme-0.1.0.faberpkg')).toEqual(preview);
    expect(f.calls[0]?.raw).toBe(file);
    expect(await http.installPackage(file, 'acme-0.1.0.faberpkg')).toEqual(info);
    expect(await http.getPackages()).toEqual([info]);
    await expect(http.uninstallPackage('acme')).resolves.toBeUndefined();
    expect(await http.getConnectorProblems()).toHaveLength(1);
  });

  it('downloads a whole file, of any size, as a blob', async () => {
    const f = fakeFetch([
      {
        path: '/api/v0/files/content?path=home%3A%2Fa.faberpkg&download=1',
        bytes: new Uint8Array([1, 2, 3]),
      },
    ]);
    const blob = await createHttp(f.fetch).downloadFile('home:/a.faberpkg');
    expect(new Uint8Array(await blob.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
  });

  it('throws the core ErrorBody as a CoreError', async () => {
    const error = {
      code: 'port-claimed',
      message: 'COM3 is used by the part "lidar".',
      hint: 'Remove or stop "lidar" first, or pick another port.',
    };
    const f = fakeFetch([{ method: 'POST', path: '/api/v0/parts', status: 409, body: { error } }]);
    const http = createHttp(f.fetch);
    const p = http.addPart({ name: 'x', connector: 'c', port: 'p' });
    await expect(p).rejects.toBeInstanceOf(CoreError);
    await expect(p).rejects.toMatchObject({ status: 409, body: error });
  });

  it('deletes a part by name, escaping it in the URL', async () => {
    const f = fakeFetch([{ method: 'DELETE', path: '/api/v0/parts/demo', status: 204 }]);
    const http = createHttp(f.fetch);
    await expect(http.removePart('demo')).resolves.toBeUndefined();
    expect(f.calls[0]?.method).toBe('DELETE');
  });

  it('explains an unreachable core in plain words', async () => {
    const f = fakeFetch([{ path: '/api/v0/parts', networkError: true }]);
    const http = createHttp(f.fetch);
    await expect(http.getParts()).rejects.toMatchObject({
      status: 0,
      body: { code: 'core-unreachable' },
    });
  });

  it('explains an error response that is not an ErrorBody', async () => {
    const f = fakeFetch([{ path: '/api/v0/parts', status: 502, text: 'Bad Gateway' }]);
    const http = createHttp(f.fetch);
    const err = await http.getParts().catch((e: unknown) => e as CoreError);
    expect(err).toBeInstanceOf(CoreError);
    expect((err as CoreError).body.code).toBe('bad-response');
    expect((err as CoreError).body.message).toContain('502');
    expect((err as CoreError).body.hint).not.toBe('');
  });
});

describe('files', () => {
  const listing = {
    path: 'home:/Desktop',
    parent: 'home:/',
    entries: [{ name: 'a.txt', path: 'home:/Desktop/a.txt', kind: 'file', size: 3 }],
  };

  it('lists a folder, with the path in the query', async () => {
    const f = fakeFetch([{ path: '/api/v0/files/list?path=home%3A%2FDesktop', body: listing }]);
    expect(await createHttp(f.fetch).listFolder('home:/Desktop')).toEqual(listing);
  });

  it('gets the roots', async () => {
    const roots = [{ id: 'home', label: 'Home', path: 'home:/' }];
    const f = fakeFetch([{ path: '/api/v0/files/roots', body: roots }]);
    expect(await createHttp(f.fetch).getRoots()).toEqual(roots);
  });

  it('reads a file as bytes', async () => {
    const f = fakeFetch([
      { path: '/api/v0/files/content?path=home%3A%2Fa.txt', bytes: new Uint8Array([104, 105]) },
    ]);
    const bytes = await createHttp(f.fetch).readFile('home:/a.txt');
    expect([...bytes]).toEqual([104, 105]);
  });

  it('writes a file as raw bytes', async () => {
    const f = fakeFetch([
      { method: 'PUT', path: '/api/v0/files/content?path=home%3A%2Fa.txt', status: 204 },
    ]);
    await createHttp(f.fetch).writeFile('home:/a.txt', new Uint8Array([1, 2]));
    const call = f.calls[0]!;
    expect(call.method).toBe('PUT');
    expect([...(call.raw as Uint8Array)]).toEqual([1, 2]);
  });

  it('creates a file without overwriting, reporting already-exists', async () => {
    const error = { code: 'already-exists', message: 'a.txt is already there.', hint: 'Pick.' };
    const f = fakeFetch([
      {
        method: 'PUT',
        path: '/api/v0/files/content?path=home%3A%2Fa.txt&mode=create',
        status: 409,
        body: { error },
      },
    ]);
    const p = createHttp(f.fetch).writeFile('home:/a.txt', new Uint8Array([1]), 'create');
    await expect(p).rejects.toMatchObject({ status: 409, body: error });
  });

  it('turns a refused config into a CoreError with the line in the message', async () => {
    const error = {
      code: 'config-invalid',
      message: 'parts.toml line 3: expected `=`.',
      hint: 'Fix line 3.',
    };
    const f = fakeFetch([
      {
        method: 'PUT',
        path: '/api/v0/files/content?path=home%3A%2FSystem%2Fparts.toml',
        status: 422,
        body: { error },
      },
    ]);
    const p = createHttp(f.fetch).writeFile('home:/System/parts.toml', new Uint8Array());
    await expect(p).rejects.toMatchObject({ status: 422, body: error });
  });

  it('sends the folder, move, trash, restore, delete, empty and samples requests', async () => {
    const f = fakeFetch([
      { method: 'POST', path: '/api/v0/files/folder', status: 201 },
      { method: 'POST', path: '/api/v0/files/move', status: 204 },
      { method: 'POST', path: '/api/v0/files/trash', status: 204 },
      { method: 'POST', path: '/api/v0/files/restore', status: 204 },
      { method: 'POST', path: '/api/v0/files/delete', status: 204 },
      { method: 'POST', path: '/api/v0/files/empty-trash', status: 204 },
      {
        method: 'POST',
        path: '/api/v0/files/samples',
        status: 201,
        body: { path: 'home:/Recordings/ld19-sample.fbcap' },
      },
    ]);
    const http = createHttp(f.fetch);
    await http.createFolder('home:/Desktop/New folder');
    await http.move('home:/a', 'home:/b');
    await http.trash(['home:/a']);
    await http.restore(['home:/Trash/a']);
    await http.deleteForGood(['home:/Trash/a']);
    await http.emptyTrash();
    await http.addSamples();
    expect(f.calls.map((c) => [c.path.replace('/api/v0/files/', ''), c.body])).toEqual([
      ['folder', { path: 'home:/Desktop/New folder' }],
      ['move', { from: 'home:/a', to: 'home:/b' }],
      ['trash', { paths: ['home:/a'] }],
      ['restore', { paths: ['home:/Trash/a'] }],
      ['delete', { paths: ['home:/Trash/a'] }],
      ['empty-trash', undefined],
      ['samples', undefined],
    ]);
  });

  it('accepts an empty 201 answer', async () => {
    const f = fakeFetch([{ method: 'POST', path: '/api/v0/files/folder', status: 201, text: '' }]);
    await expect(createHttp(f.fetch).createFolder('home:/x')).resolves.toBeUndefined();
  });

  it('builds the download address', () => {
    expect(createHttp(fakeFetch().fetch).downloadUrl('home:/Desktop/a b.txt')).toBe(
      '/api/v0/files/content?path=home%3A%2FDesktop%2Fa%20b.txt&download=1',
    );
  });
});

describe('settings', () => {
  const settings = {
    appearance: { theme: 'system', wallpaper: 'aluminium' },
    desktop: { icons: [] },
    screen: { role: 'viewer' },
    developer: { fullMachineFiles: false },
  };

  it('gets and puts the settings', async () => {
    const f = fakeFetch([
      { path: '/api/v0/settings', body: settings },
      { method: 'PUT', path: '/api/v0/settings', body: settings },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.getSettings()).toEqual(settings);
    expect(await http.putSettings(settings as never)).toEqual(settings);
    expect(f.calls[1]!.body).toEqual(settings);
  });

  it('changes developer settings through their own endpoint', async () => {
    const f = fakeFetch([{ method: 'POST', path: '/api/v0/settings/developer', body: settings }]);
    await createHttp(f.fetch).setDeveloper({ fullMachineFiles: true, acknowledgedRisk: true });
    expect(f.calls[0]!.body).toEqual({ fullMachineFiles: true, acknowledgedRisk: true });
  });
});

describe('apps', () => {
  const app = { id: 'wall-follower', name: 'Wall follower', enabled: true };

  it('lists apps, turns them on and off and picks their parts', async () => {
    const f = fakeFetch([
      { path: '/api/v0/apps', body: [app] },
      { method: 'POST', path: '/api/v0/apps/wall-follower/enable', body: app },
      { method: 'POST', path: '/api/v0/apps/wall-follower/disable', body: app },
      { method: 'PUT', path: '/api/v0/apps/wall-follower/roles', body: app },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.getApps()).toEqual([app]);
    await http.setAppEnabled('wall-follower', true);
    await http.setAppEnabled('wall-follower', false);
    await http.setAppRoles('wall-follower', { lidar: 'lidar', front: ['a', 'b'] });
    expect(f.calls.map((c) => `${c.method} ${c.path}`)).toEqual([
      'GET /api/v0/apps',
      'POST /api/v0/apps/wall-follower/enable',
      'POST /api/v0/apps/wall-follower/disable',
      'PUT /api/v0/apps/wall-follower/roles',
    ]);
    expect(f.calls[3]!.body).toEqual({ roles: { lidar: 'lidar', front: ['a', 'b'] } });
  });

  it('reads a signal once, an app’s derived one too', async () => {
    const reading = { capability: 'distance@1', reading: { tUs: 1, meters: 0.3, status: 'ok' } };
    const value = { address: 'app:wall-follower/wall-distance', capability: 'distance@1', reading };
    const f = fakeFetch([
      { path: '/api/v0/signals/app%3Awall-follower/wall-distance', body: value },
    ]);
    expect(await createHttp(f.fetch).getSignal('app:wall-follower/wall-distance')).toEqual(value);
  });
});

describe('driving', () => {
  const grant = {
    lease: 'L1',
    part: 'base',
    holder: 'Drive window',
    deadlineUs: 9,
    durationMs: 250,
    locks: [],
  };
  const stopped = { engaged: false };

  it('requests, renews and releases a lease, escaping the part and the id', async () => {
    const f = fakeFetch([
      { method: 'POST', path: '/api/v0/parts/my%20base/lease', status: 201, body: grant },
      { method: 'POST', path: '/api/v0/parts/my%20base/lease/renew', body: grant },
      { method: 'DELETE', path: '/api/v0/parts/my%20base/lease?lease=L%2B1', status: 204 },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.requestLease('my base', 'Drive window')).toEqual(grant);
    expect(await http.requestLease('my base', 'Drive window', true)).toEqual(grant);
    expect(await http.renewLease('my base', 'L1')).toEqual(grant);
    await expect(http.releaseLease('my base', 'L+1')).resolves.toBeUndefined();
    expect(f.calls.map((c) => c.body)).toEqual([
      { holder: 'Drive window' },
      { holder: 'Drive window', takeOver: true },
      { lease: 'L1' },
      undefined,
    ]);
  });

  it('lists the leases', async () => {
    const leases = [{ part: 'base', holder: 'x', deadlineUs: 1, locks: [] }];
    const f = fakeFetch([{ path: '/api/v0/leases', body: leases }]);
    expect(await createHttp(f.fetch).getLeases()).toEqual(leases);
  });

  it('sends a command with its lease and returns what was applied', async () => {
    const command = {
      capability: 'planar-base@1',
      command: { kind: 'set-velocity', vx: 0.2, vy: 0, omega: 0 },
    } as const;
    const result = { applied: command, clamped: false, deadlineUs: 10 };
    const f = fakeFetch([{ method: 'POST', path: '/api/v0/parts/base/command', body: result }]);
    expect(await createHttp(f.fetch).command('base', 'L1', command)).toEqual(result);
    expect(f.calls[0]!.body).toEqual({ lease: 'L1', command });
  });

  it('engages, reads and releases the e-stop, saying the shell did it', async () => {
    const engaged = {
      engaged: true,
      engagedBy: { tUs: 1, engaged: true, source: 'shell', reason: 'Stop button' },
    };
    const f = fakeFetch([
      { method: 'POST', path: '/api/v0/estop', body: engaged },
      { path: '/api/v0/estop', body: engaged },
      { method: 'POST', path: '/api/v0/estop/release', body: stopped },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.engageEstop('Stop button')).toEqual(engaged);
    expect(await http.getEstop()).toEqual(engaged);
    expect(await http.releaseEstop()).toEqual(stopped);
    expect(f.calls.map((c) => c.body)).toEqual([
      { reason: 'Stop button', source: 'shell' },
      undefined,
      { source: 'shell' },
    ]);
  });

  it('gets and puts a manual part simulation', async () => {
    const sim = { pattern: { kind: 'steady', value: 1.5 } } as const;
    const f = fakeFetch([
      { path: '/api/v0/parts/front/simulation', body: sim },
      { method: 'PUT', path: '/api/v0/parts/front/simulation', body: sim },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.getSimulation('front')).toEqual(sim);
    expect(await http.putSimulation('front', sim.pattern)).toEqual(sim);
    expect(f.calls[1]!.body).toEqual(sim);
  });
});

describe('the simulated world', () => {
  it('lists the worlds, reads and switches the current one, and resets the machine', async () => {
    const view = { name: 'apartment', title: 'Apartment' };
    const f = fakeFetch([
      { path: '/api/v0/worlds', body: [{ name: 'empty' }] },
      { path: '/api/v0/world', body: view },
      { method: 'PUT', path: '/api/v0/world', body: view },
      { method: 'POST', path: '/api/v0/world/reset', status: 204 },
    ]);
    const http = createHttp(f.fetch);
    expect(await http.getWorlds()).toEqual([{ name: 'empty' }]);
    expect(await http.getWorld()).toEqual(view);
    expect(await http.putWorld('apartment')).toEqual(view);
    await expect(http.resetWorld()).resolves.toBeUndefined();
    expect(f.calls.map((c) => [c.method, c.body])).toEqual([
      ['GET', undefined],
      ['GET', undefined],
      ['PUT', { name: 'apartment' }],
      ['POST', undefined],
    ]);
  });
});
