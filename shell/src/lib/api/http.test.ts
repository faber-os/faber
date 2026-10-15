import { describe, expect, it } from 'vitest';
import { fakeFetch } from '../../test/fakeFetch';
import { CoreError, createHttp } from './http';

const part = {
  name: 'demo',
  connector: 'ldrobot-ld19',
  port: 'replay:demo-ld19',
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
      { path: '/api/v0/ports', body: [{ id: 'replay:demo-ld19' }] },
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
      port: 'replay:demo-ld19',
    });
    expect(created.name).toBe('demo');
    expect(f.calls[0]).toEqual({
      method: 'POST',
      path: '/api/v0/parts',
      body: { name: 'demo', connector: 'ldrobot-ld19', port: 'replay:demo-ld19' },
    });
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
