import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

/**
 * Phase 2, end to end, without hardware: add a simulated drive base and a simulated distance
 * sensor, drive the base for a second from the Drive window (after the D46 question), watch its
 * commanded velocity change, then stop everything with the taskbar's e-stop and release it.
 */
const BASE = 'e2e_base';
const FRONT = 'e2e_front';

async function clean(request: APIRequestContext) {
  for (const name of [BASE, FRONT]) {
    const res = await request.delete(`/api/v0/parts/${name}`);
    expect([204, 404]).toContain(res.status());
  }
  await request.post('/api/v0/estop/release', { data: {} });
}

test.beforeEach(async ({ request }) => clean(request));
test.afterEach(async ({ request }) => clean(request));

async function addSimulated(page: Page, connector: RegExp, name: string) {
  const parts = page.getByRole('dialog', { name: 'Parts' });
  await parts
    .getByRole('button', { name: /^Add (the first )?part$/ })
    .first()
    .click();
  const form = parts.getByRole('form', { name: 'Add part' });
  await form.getByRole('switch', { name: /Simulated/ }).check();
  await expect(form.getByRole('radio', { name: /Manual/ })).toBeChecked();
  await form.getByRole('radio', { name: connector }).check();
  await form.getByLabel('Name').fill(name);
  await form.getByRole('button', { name: 'Add part' }).click();
  await expect(parts.locator(`li[data-part="${name}"]`)).toBeVisible();
}

test('drive a simulated base, stop it with the e-stop and release it', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  const parts = page.getByRole('dialog', { name: 'Parts' });
  if (!(await parts.isVisible()))
    await taskbar.getByRole('button', { name: 'Parts', exact: true }).click();
  await addSimulated(page, /generic-planar-base/, BASE);
  await addSimulated(page, /generic-distance/, FRONT);

  // The manual sensor has a slider; the base says nobody drives it.
  const front = parts.locator(`li[data-part="${FRONT}"]`);
  await expect(front.getByRole('slider', { name: `Value of ${FRONT}` })).toBeEnabled();
  const base = parts.locator(`li[data-part="${BASE}"]`);
  await expect(base.getByText('Nobody is driving it')).toBeVisible();

  // Open Drive on the base: Faber asks first (D46).
  await base.getByRole('button', { name: `Drive ${BASE}` }).click();
  const drive = page.getByRole('dialog', { name: 'Drive', exact: true });
  await expect(drive).toBeVisible();
  const ask = drive.getByRole('alertdialog', { name: /^Start driving .+\?$/ });
  await expect(ask).toBeVisible();
  await ask.getByRole('button', { name: 'Start' }).click();
  await expect(drive.getByTestId('drive-status')).toContainText('Ready');

  // Drive forward for a second with the W key.
  const commanded = drive.getByTestId('twist-commanded-vx');
  await page.keyboard.down('KeyW');
  await expect(drive.getByTestId('drive-status')).toContainText(`You're driving ${BASE}`);
  await expect
    .poll(async () => parseFloat((await commanded.textContent()) ?? '0'))
    .toBeGreaterThan(0);
  await page.waitForTimeout(1_000);
  await page.keyboard.up('KeyW');
  await expect.poll(async () => parseFloat((await commanded.textContent()) ?? '1')).toBe(0);

  // The taskbar's Stop engages the e-stop at once.
  await taskbar.getByRole('button', { name: /^Stop/ }).click();
  await expect(taskbar.getByText('Stopped')).toBeVisible();
  await expect(drive.getByTestId('drive-status')).toContainText('Stopped by the e-stop');

  // Releasing it asks first.
  await taskbar.getByRole('button', { name: 'Release' }).click();
  const release = page.getByRole('alertdialog', { name: 'Release the e-stop?' });
  await release.getByRole('button', { name: 'Release' }).click();
  await expect(taskbar.getByRole('button', { name: /^Stop/ })).toBeVisible();
  await expect(taskbar.getByText('Stopped')).toBeHidden();
});
