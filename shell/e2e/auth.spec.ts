import { expect, test, type APIRequestContext } from '@playwright/test';
import { ADMIN, VIEWER, signIn } from './auth';

/**
 * Users, roles and signing in, end to end (docs/auth.md). global-setup.ts already created the
 * admin through the first-run screen; these tests use their own browser contexts, so signing out
 * never ends the session the other tests share.
 */
const BASE = 'e2e_auth_base';

async function clean(request: APIRequestContext) {
  expect([204, 404]).toContain((await request.delete(`/api/v0/parts/${BASE}`)).status());
  await request.post('/api/v0/estop/release', { data: {} });
}

test.beforeEach(async ({ request }) => clean(request));
test.afterEach(async ({ request }) => clean(request));

test('the first run made the admin, and now the machine asks to sign in', async ({
  request,
  browser,
}) => {
  const me = await (await request.get('/api/v0/session')).json();
  expect(me).toMatchObject({
    needsSetup: false,
    identity: { user: ADMIN.name, role: 'admin', via: 'session' },
  });
  const context = await browser.newContext({ storageState: undefined });
  const page = await context.newPage();
  await page.goto('/');
  await expect(page.getByRole('dialog', { name: /^Sign in to / })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Create the admin' })).toHaveCount(0);
  // Nothing behind it answers without signing in.
  expect((await context.request.get('/api/v0/parts')).status()).toBe(401);
  await context.close();
});

test('sign out, and in again', async ({ browser }) => {
  const context = await browser.newContext({ storageState: undefined });
  const page = await context.newPage();
  await page.goto('/');
  await signIn(page, ADMIN);
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await taskbar.getByRole('button', { name: `${ADMIN.name} (Admin)` }).click();
  await page.getByRole('menuitem', { name: 'Sign out' }).click();
  await expect(page.getByRole('dialog', { name: /^Sign in to / })).toBeVisible();
  expect((await context.request.get('/api/v0/parts')).status()).toBe(401);
  await signIn(page, ADMIN);
  await expect(taskbar.getByRole('button', { name: `${ADMIN.name} (Admin)` })).toBeVisible();
  await context.close();
});

test('a viewer can’t drive, but can press Stop', async ({ request, browser }) => {
  const made = await request.post('/api/v0/users', {
    data: { name: VIEWER.name, password: VIEWER.password, role: 'viewer' },
  });
  expect([201, 409]).toContain(made.status());
  const base = await request.post('/api/v0/parts', {
    data: { name: BASE, connector: 'generic-planar-base', simulate: 'manual' },
  });
  expect(base.status()).toBe(201);

  const context = await browser.newContext({ storageState: undefined });
  const page = await context.newPage();
  await page.goto('/');
  await signIn(page, VIEWER);
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByRole('button', { name: `${VIEWER.name} (Viewer)` })).toBeVisible();

  // Parts shows the base; Add part is there but disabled, with the reason.
  const parts = page.getByRole('dialog', { name: 'Parts' });
  if (!(await parts.isVisible()))
    await taskbar.getByRole('button', { name: 'Parts', exact: true }).click();
  const add = parts.getByRole('button', { name: /^Add (the first )?part$/ }).first();
  await expect(add).toBeDisabled();
  await expect(add).toHaveAttribute('title', 'Needs the admin role');

  // Drive: no "Start driving?" question, and Start is disabled with the reason.
  await parts
    .locator(`li[data-part="${BASE}"]`)
    .getByRole('button', { name: `Drive ${BASE}` })
    .click();
  const drive = page.getByRole('dialog', { name: 'Drive', exact: true });
  await expect(drive).toBeVisible();
  await expect(drive.getByRole('alertdialog')).toHaveCount(0);
  const start = drive.getByRole('button', { name: 'Start driving' });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute('title', 'Needs the operator role');
  // The core refuses a lease too, whatever the page does.
  const lease = await context.request.post(`/api/v0/parts/${BASE}/lease`, {
    data: { holder: 'sneaky' },
  });
  expect(lease.status()).toBe(403);

  // Stop works for everyone; releasing it doesn't.
  await taskbar.getByRole('button', { name: /^Stop/ }).click();
  await expect(taskbar.getByText('Stopped')).toBeVisible();
  const release = taskbar.getByRole('button', { name: 'Release' });
  await expect(release).toBeDisabled();
  await expect(release).toHaveAttribute('title', 'Needs the operator role');
  await context.close();
});
