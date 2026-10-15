import { expect, test, type APIRequestContext, type Locator, type Page } from '@playwright/test';

/**
 * The World window, end to end: from its empty state add a simulated base in the world, add a
 * world LD19 through Add part, pick the apartment, see the machine and the scan drawn, drive
 * forward from Drive for half a second, see the machine move, then put it back with Reset
 * position.
 */
const BASE = 'e2e_world_base';
const LIDAR = 'e2e_world_lidar';

async function clean(request: APIRequestContext) {
  for (const name of [BASE, LIDAR]) {
    const res = await request.delete(`/api/v0/parts/${name}`);
    expect([204, 404]).toContain(res.status());
  }
  await request.post('/api/v0/estop/release', { data: {} });
  const res = await request.put('/api/v0/world', { data: { name: 'empty' } });
  expect(res.status()).toBe(200);
}

test.beforeEach(async ({ request }) => clean(request));
test.afterEach(async ({ request }) => clean(request));

async function openProgram(page: Page, kind: string) {
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.locator(`[role="menuitem"][data-kind="${kind}"]`).first().click();
}

/** How many pixels near the machine (from the canvas's own test hooks) are in `token`'s colour. */
async function pixelsNearMachine(canvas: Locator, token: string, radius = 10): Promise<number> {
  return canvas.evaluate(
    (el, { token, radius }) => {
      const c = el as HTMLCanvasElement;
      const x = Number(c.dataset.machineX);
      const y = Number(c.dataset.machineY);
      if (!c.dataset.machineX || !Number.isFinite(x)) return -1;
      // The token's colour as the canvas draws it.
      const probe = document.createElement('canvas').getContext('2d')!;
      probe.fillStyle = getComputedStyle(c).getPropertyValue(token).trim();
      probe.fillRect(0, 0, 1, 1);
      const [r, g, b] = probe.getImageData(0, 0, 1, 1).data;
      const dpr = c.width / c.getBoundingClientRect().width;
      const side = Math.round(2 * radius * dpr);
      const data = c
        .getContext('2d')!
        .getImageData(
          Math.round((x - radius) * dpr),
          Math.round((y - radius) * dpr),
          side,
          side,
        ).data;
      let n = 0;
      for (let i = 0; i < data.length; i += 4) {
        const d =
          Math.abs(data[i]! - r!) + Math.abs(data[i + 1]! - g!) + Math.abs(data[i + 2]! - b!);
        if (d < 12) n++;
      }
      return n;
    },
    { token, radius },
  );
}

async function position(page: Page): Promise<{ x: number; y: number }> {
  const p = page.getByTestId('world-position');
  return { x: Number(await p.getAttribute('data-x')), y: Number(await p.getAttribute('data-y')) };
}

test('add a world machine and LiDAR, see them, drive, and reset the position', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  // The World window starts empty and says how to add a machine.
  await openProgram(page, 'world');
  const world = page.getByRole('dialog', { name: 'World', exact: true });
  const empty = world.getByTestId('world-empty');
  await expect(empty).toBeVisible();
  await empty.getByRole('button', { name: 'Add a simulated base' }).click();

  // Add part opens on Simulated → In the world, with the drive base picked.
  const parts = page.getByRole('dialog', { name: 'Parts' });
  const form = parts.getByRole('form', { name: 'Add part' });
  await expect(form.getByRole('radio', { name: /In the world/ })).toBeChecked();
  await expect(form.getByRole('radio', { name: /generic-planar-base/ })).toBeChecked();
  await form.getByLabel('Name').fill(BASE);
  await form.getByRole('button', { name: 'Add part' }).click();
  await expect(parts.locator(`li[data-part="${BASE}"]`)).toBeVisible();

  // A LiDAR in the world, through the same form.
  await parts
    .getByRole('button', { name: /^Add part$/ })
    .first()
    .click();
  await form.getByRole('switch', { name: /Simulated/ }).check();
  await form.getByRole('radio', { name: /In the world/ }).check();
  await form.getByRole('radio', { name: /ldrobot-ld19/ }).check();
  await form.getByLabel('Name').fill(LIDAR);
  await form.getByRole('button', { name: 'Add part' }).click();
  await expect(parts.locator(`li[data-part="${LIDAR}"]`)).toBeVisible();

  // Pick the apartment; Faber asks first.
  await openProgram(page, 'world');
  await expect(empty).toBeHidden();
  await world.getByRole('button', { name: /^World: / }).click();
  await world.getByRole('option', { name: /Apartment/ }).click();
  await world
    .getByRole('alertdialog', { name: 'Switch to Apartment?' })
    .getByRole('button', { name: 'Switch' })
    .click();
  await expect(world.getByRole('button', { name: 'World: Apartment' })).toBeVisible();

  // The machine is drawn where it stands, and the LiDAR's points around it.
  const canvas = world.getByTestId('world-canvas');
  await expect.poll(() => pixelsNearMachine(canvas, '--accent')).toBeGreaterThan(40);
  await expect.poll(() => pixelsNearMachine(canvas, '--info', 200)).toBeGreaterThan(20);
  // The window shows the machine at the apartment's start.
  const view = await (await page.request.get('/api/v0/world')).json();
  const start = { x: view.start.x as number, y: view.start.y as number };
  const fromStart = async () => {
    const p = await position(page);
    return Math.hypot(p.x - start.x, p.y - start.y);
  };
  await expect.poll(fromStart).toBeLessThan(0.01);

  // Drive forward from Drive for half a second.
  await world.getByRole('button', { name: 'Drive' }).click();
  const drive = page.getByRole('dialog', { name: 'Drive', exact: true });
  await drive
    .getByRole('alertdialog', { name: /^Start driving .+\?$/ })
    .getByRole('button', { name: 'Start' })
    .click();
  await expect(drive.getByTestId('drive-status')).toContainText('Ready');
  await page.keyboard.down('KeyW');
  await expect(drive.getByTestId('drive-status')).toContainText(`You're driving ${BASE}`);
  await page.waitForTimeout(500);
  await page.keyboard.up('KeyW');

  await expect.poll(fromStart).toBeGreaterThan(0.05);

  // Reset position, in World (brought back to the front), puts it back where it started.
  await taskbar.getByRole('button', { name: 'World', exact: true }).click();
  await world.getByRole('button', { name: 'Reset position' }).click();
  await expect.poll(fromStart).toBeLessThan(0.01);
});
