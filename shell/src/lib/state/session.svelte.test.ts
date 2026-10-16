import { describe, expect, it } from 'vitest';
import { createHttp } from '../api/http';
import { fakeFetch } from '../../test/fakeFetch';
import { sessionAs } from '../../test/fakeShell';
import { SessionState, roleAllows, rolesUpTo } from './session.svelte';

function setup(routes: Parameters<typeof fakeFetch>[0] = []) {
  const f = fakeFetch(routes);
  return { f, auth: new SessionState(createHttp(f.fetch)) };
}

describe('roles', () => {
  it('each role can do everything the one below it can', () => {
    expect(roleAllows('admin', 'operator')).toBe(true);
    expect(roleAllows('operator', 'operator')).toBe(true);
    expect(roleAllows('viewer', 'operator')).toBe(false);
    expect(roleAllows(null, 'viewer')).toBe(false);
  });

  it('a token gets a role up to its user’s', () => {
    expect(rolesUpTo('operator')).toEqual(['viewer', 'operator']);
    expect(rolesUpTo('admin')).toEqual(['viewer', 'operator', 'admin']);
  });
});

describe('SessionState', () => {
  it('says why a role can’t, and nothing when it can', () => {
    const { auth } = setup();
    auth.info = sessionAs('viewer');
    expect(auth.lock('operator')).toBe('Needs the operator role');
    expect(auth.lock('viewer')).toBeNull();
    auth.info = sessionAs('admin');
    expect(auth.lock('admin')).toBeNull();
  });

  it('nobody signed in may do nothing', () => {
    const { auth } = setup();
    auth.info = { ...sessionAs('admin'), identity: undefined };
    expect(auth.role).toBeNull();
    expect(auth.can('viewer')).toBe(false);
  });

  it('signs in, and knows who it is', async () => {
    const { f, auth } = setup([
      { method: 'POST', path: '/api/v0/session', body: sessionAs('operator', 'olive') },
    ]);
    const r = await auth.signIn('olive', 'password1');
    expect(r.ok).toBe(true);
    expect(f.calls.at(-1)?.body).toEqual({ name: 'olive', password: 'password1' });
    expect(auth.role).toBe('operator');
    expect(auth.user).toBe('olive');
  });

  it('keeps the core’s reason when signing in fails', async () => {
    const { auth } = setup([
      {
        method: 'POST',
        path: '/api/v0/session',
        status: 429,
        body: { error: { code: 'too-many-attempts', message: 'Wait 2 s', hint: 'h' } },
      },
    ]);
    const r = await auth.signIn('olive', 'nope');
    expect(r).toEqual({
      ok: false,
      error: { code: 'too-many-attempts', message: 'Wait 2 s', hint: 'h' },
    });
    expect(auth.info).toBeNull();
  });

  it('creates the first admin', async () => {
    const { f, auth } = setup([
      { method: 'POST', path: '/api/v0/setup', status: 201, body: sessionAs('admin') },
    ]);
    expect((await auth.setup('mikkel', 'correct horse')).ok).toBe(true);
    expect(f.calls.at(-1)).toMatchObject({ method: 'POST', path: '/api/v0/setup' });
    expect(auth.role).toBe('admin');
  });

  it('signs out, then asks who it is now', async () => {
    const { f, auth } = setup([
      { method: 'DELETE', path: '/api/v0/session', status: 204 },
      { path: '/api/v0/session', body: { ...sessionAs('admin'), identity: undefined } },
    ]);
    auth.info = sessionAs('admin');
    await auth.signOutAndForget();
    expect(f.calls.map((c) => `${c.method} ${c.path}`)).toEqual([
      'DELETE /api/v0/session',
      'GET /api/v0/session',
    ]);
    expect(auth.role).toBeNull();
  });

  it('manages users and tokens through the core', async () => {
    const { f, auth } = setup([
      { method: 'POST', path: '/api/v0/users', status: 201, body: { name: 'vic', role: 'viewer' } },
      { method: 'PUT', path: '/api/v0/users/vic/role', status: 204 },
      { method: 'PUT', path: '/api/v0/users/vic/password', status: 204 },
      { method: 'DELETE', path: '/api/v0/users/vic', status: 204 },
      {
        method: 'POST',
        path: '/api/v0/tokens',
        status: 201,
        body: {
          token: { id: 'ab12', name: 't', user: 'vic', role: 'viewer', created: 'now' },
          secret: 'faber_x',
        },
      },
      { method: 'DELETE', path: '/api/v0/tokens/ab12', status: 204 },
    ]);
    expect((await auth.addUser('vic', 'password1', 'viewer')).ok).toBe(true);
    expect((await auth.setRole('vic', 'operator')).ok).toBe(true);
    expect(f.calls.at(-1)?.body).toEqual({ role: 'operator' });
    expect((await auth.setPassword('vic', 'new password', 'old password')).ok).toBe(true);
    expect(f.calls.at(-1)?.body).toEqual({ password: 'new password', current: 'old password' });
    expect((await auth.setPassword('vic', 'reset one')).ok).toBe(true);
    expect(f.calls.at(-1)?.body).toEqual({ password: 'reset one' });
    expect((await auth.removeUser('vic')).ok).toBe(true);
    const made = await auth.createToken('t', 'viewer');
    expect(made.ok && made.value.secret).toBe('faber_x');
    expect((await auth.revokeToken('ab12')).ok).toBe(true);
  });
});
