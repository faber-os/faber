import { readFileSync } from 'node:fs';
import { expect, test, type APIRequestContext } from '@playwright/test';

/**
 * The SENNEP app end to end (Phase 5, docs/roadmap.md): its package on the desktop is opened in
 * Files; the Install window binds its roles to a simulated SENNEP V2 in the apartment (base, LiDAR,
 * four corner distance sensors, battery); it is installed and turned on; its window shows the
 * LiDAR's view; a held key drives SENNEP (through the app's logic, which takes the base's lease)
 * and the dead-reckoned pose moves; Stop lets go of the base.
 *
 * The package is the example packed (core/tests/sennep.rs writes the fixture).
 */
const ID = 'sennep';
const FILE = `${ID}-0.1.0.faberpkg`;
const PACKAGE = `home:/Desktop/${FILE}`;
const P = 'e2e_sennep_';
const BASE = `${P}base`;
const LIDAR = `${P}lidar`;
const PACK = `${P}pack`;
const CORNERS = [
  { name: `${P}fl`, x: 0.4, y: 0.325, yaw: Math.PI / 4 },
  { name: `${P}fr`, x: 0.4, y: -0.325, yaw: -Math.PI / 4 },
  { name: `${P}rl`, x: -0.4, y: 0.325, yaw: (3 * Math.PI) / 4 },
  { name: `${P}rr`, x: -0.4, y: -0.325, yaw: (-3 * Math.PI) / 4 },
];

const fixture = readFileSync(new URL(`./fixtures/${FILE}`, import.meta.url));

async function clean(request: APIRequestContext) {
  await request.post(`/api/v0/apps/${ID}/disable`);
  expect([204, 404]).toContain((await request.delete(`/api/v0/packages/${ID}`)).status());
  for (const name of [BASE, LIDAR, PACK, ...CORNERS.map((c) => c.name)])
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
  // In the living room, facing north with open floor ahead.
  const world = await request.put('/api/v0/world', {
    data: { name: 'apartment', start: { x: 7.2, y: 2.0, yaw: Math.PI / 2 } },
  });
  expect(world.status()).toBe(200);
  const parts = [
    {
      name: BASE,
      connector: 'generic-planar-base',
      settings: { length: 0.8, width: 0.65 },
    },
    { name: LIDAR, connector: 'ldrobot-ld19', mount: { x: 0.2, z: 0.3 } },
    { name: PACK, connector: 'generic-battery' },
    ...CORNERS.map((c) => ({
      name: c.name,
      connector: 'generic-distance',
      mount: { x: c.x, y: c.y, z: 0.2, yaw: c.yaw },
    })),
  ];
  for (const p of parts) {
    const res = await request.post('/api/v0/parts', { data: { ...p, simulate: 'world' } });
    expect(res.status(), p.name).toBe(201);
  }
});

test.afterEach(async ({ request }) => clean(request));

test('install SENNEP, bind its roles, drive it with a key and stop it', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  await files.getByRole('button', { name: 'Desktop', exact: true }).click();
  await files.getByRole('option', { name: FILE }).dblclick();

  // The Install window: what it needs, with the parts that fit each role.
  const install = page.getByRole('dialog', { name: `Install ${FILE}` });
  await expect(install.getByRole('heading', { name: 'SENNEP', level: 3 })).toBeVisible();
  await expect(install).toContainText('app:sennep/pose');
  await expect(install).toContainText('app:sennep/nearest-obstacle');
  await install.getByRole('combobox', { name: 'lidar' }).selectOption(LIDAR);
  await install.getByRole('combobox', { name: 'base' }).selectOption(BASE);
  await expect(install).toContainText(`SENNEP will be able to drive "${BASE}".`);
  const roles = install.getByRole('list', { name: 'Roles' });
  await expect(roles).toContainText('any number of parts, optional');
  for (const c of CORNERS) await install.getByRole('checkbox', { name: c.name }).check();
  await install.getByRole('combobox', { name: 'battery' }).selectOption(PACK);
  await install.getByRole('checkbox', { name: 'Turn it on now' }).check();
  await install.getByRole('button', { name: 'Install', exact: true }).click();
  await expect(install).toContainText('SENNEP is installed');
  await expect(install).toContainText('It is on');
  await install.getByRole('button', { name: 'Open its window' }).click();

  // Its own page, sandboxed, sees the LiDAR, the battery and the four sensors.
  const win = page.locator('[data-window-kind="app"]');
  await expect(win).toBeVisible();
  await expect(win.locator('iframe')).toHaveAttribute('sandbox', 'allow-scripts');
  const app = page.frameLocator('[data-window-kind="app"] iframe');
  await expect(app.locator('#state')).toHaveText('On', { timeout: 15_000 });
  await expect(app.getByTestId('scan-note')).toHaveText(/^\d+ returns$/, { timeout: 15_000 });
  await expect(app.getByTestId('battery')).toHaveText(/V, OK/, { timeout: 10_000 });
  await expect(app.locator('#ring-note')).toHaveText('4 sensors, by where each looks');
  await expect(app.locator('#ring path')).toHaveCount(4);
  const pose = app.getByTestId('pose');
  await expect(pose).toHaveText(/^x 0\.00 m, y 0\.00 m/, { timeout: 10_000 });

  // Hold W for a moment: the logic drives the base under its own lease, and the pose moves.
  await app.getByTestId('lidar-view').click();
  await page.keyboard.down('KeyW');
  await expect
    .poll(async () => (await (await page.request.get('/api/v0/leases')).json()) as unknown[], {
      timeout: 5_000,
    })
    .toContainEqual(expect.objectContaining({ part: BASE, holder: 'app:sennep' }));
  await expect(app.getByTestId('velocity')).toHaveText(/m\/s ahead/);
  await page.waitForTimeout(800);
  // Stop (the window's own button) lets go, whatever keys are down.
  await app.getByTestId('stop').click();
  await page.keyboard.up('KeyW');
  await expect
    .poll(async () => (await (await page.request.get('/api/v0/leases')).json()) as unknown[])
    .toEqual([]);
  await expect(pose).not.toHaveText(/^x 0\.00 m/);
  await expect(app.getByTestId('velocity')).toHaveText('Standing still');
  const base = await (await page.request.get(`/api/v0/signals/${BASE}/planar-base`)).json();
  expect(base.reading.reading.commanded).toEqual({ vx: 0, vy: 0, omega: 0 });

  // Reset pose, once it has rolled to a stop: back to 0.
  await expect
    .poll(async () => {
      const s = await (await page.request.get(`/api/v0/signals/${BASE}/planar-base`)).json();
      return s.reading.reading.measured;
    })
    .toEqual({ vx: 0, vy: 0, omega: 0 });
  await app.getByRole('button', { name: 'Reset pose' }).click();
  await expect(pose).toHaveText(/^x 0\.00 m, y 0\.00 m/);
});
