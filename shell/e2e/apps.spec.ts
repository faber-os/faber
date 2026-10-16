import { readFileSync } from 'node:fs';
import { expect, test, type APIRequestContext } from '@playwright/test';

/**
 * Apps through the shell, end to end (docs/apps.md): the wall-follower package on the desktop is
 * double-clicked in Files; the Install window shows what it needs and that it will drive the base;
 * its roles get a simulated base and LiDAR in the apartment; it is installed and turned on; its
 * window (its own page, sandboxed, talking only through @faber-os/shell) shows the wall distance
 * updating; it is turned off in Apps, and the window says so.
 *
 * The package is the example packed (core/tests/apps.rs writes the fixture).
 */
const ID = 'wall-follower';
const FILE = `${ID}-0.1.0.faberpkg`;
const PACKAGE = `home:/Desktop/${FILE}`;
const BASE = 'e2e_app_base';
const LIDAR = 'e2e_app_lidar';

const fixture = readFileSync(new URL(`./fixtures/${FILE}`, import.meta.url));

async function clean(request: APIRequestContext) {
  await request.post(`/api/v0/apps/${ID}/disable`);
  expect([204, 404]).toContain((await request.delete(`/api/v0/packages/${ID}`)).status());
  for (const name of [BASE, LIDAR])
    expect([204, 404]).toContain((await request.delete(`/api/v0/parts/${name}`)).status());
  await request.post('/api/v0/files/trash', { data: { paths: [PACKAGE] } });
  await request.post('/api/v0/estop/release', { data: {} });
  expect((await request.put('/api/v0/world', { data: { name: 'empty' } })).status()).toBe(200);
}

test.beforeEach(async ({ request }) => {
  await clean(request);
  const put = await request.put(`/api/v0/files/content?path=${encodeURIComponent(PACKAGE)}`, {
    data: fixture,
  });
  expect(put.status()).toBe(204);
  // SENNEP in the apartment's living room, near the south wall (the example's README).
  const world = await request.put('/api/v0/world', {
    data: { name: 'apartment', start: { x: 7.0, y: 0.8, yaw: 0 } },
  });
  expect(world.status()).toBe(200);
  const base = await request.post('/api/v0/parts', {
    data: {
      name: BASE,
      connector: 'generic-planar-base',
      simulate: 'world',
      settings: { length: 0.8, width: 0.65 },
    },
  });
  expect(base.status()).toBe(201);
  const lidar = await request.post('/api/v0/parts', {
    data: { name: LIDAR, connector: 'ldrobot-ld19', simulate: 'world' },
  });
  expect(lidar.status()).toBe(201);
});

test.afterEach(async ({ request }) => clean(request));

test('install the wall follower, pick its parts, run it and watch it in its window', async ({
  page,
}) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  await files.getByRole('button', { name: 'Desktop', exact: true }).click();
  await files.getByRole('option', { name: FILE }).dblclick();

  const install = page.getByRole('dialog', { name: `Install ${FILE}` });
  await expect(install.getByRole('heading', { name: 'Wall follower', level: 3 })).toBeVisible();
  await expect(install).toContainText('Not checked by the Faber registry');
  await expect(install).toContainText('app:wall-follower/wall-distance');
  await install.getByRole('combobox', { name: 'lidar' }).selectOption(LIDAR);
  await install.getByRole('combobox', { name: 'base' }).selectOption(BASE);
  await expect(install).toContainText(`Wall follower will be able to drive "${BASE}".`);
  await install.getByRole('checkbox', { name: 'Turn it on now' }).check();
  await install.getByRole('button', { name: 'Install', exact: true }).click();
  await expect(install).toContainText('Wall follower is installed');
  await expect(install).toContainText('It is on');
  await install.getByRole('button', { name: 'Open its window' }).click();

  // Its own page, in a sandboxed frame, gets everything through the shell.
  const win = page.locator('[data-window-kind="app"]');
  await expect(win).toBeVisible();
  const frameEl = win.locator('iframe');
  await expect(frameEl).toHaveAttribute('sandbox', 'allow-scripts');
  const app = page.frameLocator('[data-window-kind="app"] iframe');
  await expect(app.locator('#state')).toHaveText('On, driving along the wall', {
    timeout: 15_000,
  });
  const gap = app.getByTestId('wall-distance');
  await expect(gap).toHaveText(/^\d\.\d\d m$/, { timeout: 15_000 });
  const first = await gap.textContent();
  await expect.poll(() => gap.textContent(), { timeout: 15_000 }).not.toBe(first);

  // The logic drives the base as app:wall-follower (the window only watches).
  const leases = await page.request.get('/api/v0/leases');
  expect(await leases.json()).toContainEqual(
    expect.objectContaining({ part: BASE, holder: 'app:wall-follower' }),
  );

  // Its speed slider is the app's state, shared with its logic: sliding it saves it, and the
  // logic drives at the new speed.
  const speed = app.getByTestId('speed');
  await expect(speed).toHaveValue('0.25');
  await speed.fill('0.1');
  await expect(app.locator('#speed-value')).toHaveText('0.10 m/s');
  await expect
    .poll(async () => (await (await page.request.get(`/api/v0/apps/${ID}/state`)).json()).values)
    .toEqual({ speed: '0.1' });
  await expect
    .poll(
      async () => {
        const s = await (await page.request.get(`/api/v0/signals/${BASE}/planar-base`)).json();
        return s.reading.reading.commanded.vx;
      },
      { timeout: 10_000 },
    )
    .toBeCloseTo(0.1, 3);
  // Set elsewhere (another window, a program): the slider follows.
  const put = await page.request.put(`/api/v0/apps/${ID}/state`, {
    data: { key: 'speed', value: '0.3' },
  });
  expect(put.status()).toBe(200);
  await expect(speed).toHaveValue('0.3');

  // Off in Apps: the window hears it, and the lease ends.
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.locator('[role="menuitem"][data-kind="apps"]').first().click();
  const apps = page.getByRole('dialog', { name: 'Apps' });
  // The switch shows what the core says, so it flips once the core has turned the app off.
  const on = apps.getByRole('switch', { name: 'Turn Wall follower on' });
  await expect(on).toBeChecked();
  await on.click();
  await expect(on).not.toBeChecked();
  await expect(app.locator('#state')).toHaveText('Off');
  await expect(app.getByTestId('wall-distance')).toHaveText(/^\d\.\d\d m$/);
  await expect
    .poll(async () => (await (await page.request.get('/api/v0/leases')).json()) as unknown[])
    .toEqual([]);
});
