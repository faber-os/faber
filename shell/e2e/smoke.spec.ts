import { expect, test } from '@playwright/test';

/**
 * Step 1 of the ladder, end to end, without hardware: add the sample LD19 recording (D32), add a
 * part on its replay port, open the viewer and see a scan arrive. Needs a running core (see
 * playwright.config.ts).
 */
const PART = 'sample_lidar';
const SAMPLE_PORT = 'replay:ld19-sample';

test.beforeEach(async ({ request }) => {
  // Start from a clean machine config, so the run can be repeated.
  const res = await request.delete(`/api/v0/parts/${PART}`);
  expect([204, 404]).toContain(res.status());
});

test.afterEach(async ({ request }) => {
  await request.delete(`/api/v0/parts/${PART}`);
});

test('add the sample recording, add a part on it and watch its scan', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  // Files → Recordings → Add sample recordings (only offered while Recordings is empty).
  await page.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  await files.getByRole('button', { name: 'Recordings' }).click();
  const addSamples = files.getByRole('button', { name: 'Add sample recordings' });
  const sample = files.getByRole('option', { name: /ld19-sample\.fbcap/ });
  await expect(addSamples.or(sample)).toBeVisible();
  if (await addSamples.isVisible()) await addSamples.click();
  await expect(sample).toBeVisible();

  const parts = page.getByRole('dialog', { name: 'Parts' });
  await taskbar.getByRole('button', { name: 'Parts', exact: true }).click();
  await expect(parts).toBeVisible();
  await parts
    .getByRole('button', { name: /^Add (the first )?part$/ })
    .first()
    .click();

  const form = parts.getByRole('form', { name: 'Add part' });
  await form.getByRole('radio', { name: new RegExp(SAMPLE_PORT) }).check();
  await form.getByRole('radio', { name: /ldrobot-ld19/ }).check();
  await form.getByLabel('Name').fill(PART);
  await form.getByRole('button', { name: 'Add part' }).click();

  const row = parts.locator(`li[data-part="${PART}"]`);
  await expect(row).toBeVisible();
  await row.getByRole('button', { name: PART, exact: true }).click();

  const viewer = page.getByRole('dialog', { name: `${PART} · scan` });
  await expect(viewer).toBeVisible();
  await expect(viewer.locator('canvas')).toBeVisible();
  await expect
    .poll(async () => Number(await viewer.getByTestId('scan-points').textContent()), {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);
});
