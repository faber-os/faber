import { expect, test } from '@playwright/test';

/**
 * Step 1 of the ladder, end to end: add a part on the built-in demo recording, open its viewer
 * and see a scan arrive. Needs a running core (see playwright.config.ts).
 */
const PART = 'demo';

test.beforeEach(async ({ request }) => {
  // Start from a clean machine config, so the run can be repeated.
  const res = await request.delete(`/api/v0/parts/${PART}`);
  expect([204, 404]).toContain(res.status());
});

test.afterEach(async ({ request }) => {
  await request.delete(`/api/v0/parts/${PART}`);
});

test('add the demo LD19 and watch its scan', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected'),
  ).toBeVisible();

  const parts = page.getByRole('dialog', { name: 'Parts' });
  await expect(parts).toBeVisible();
  await parts
    .getByRole('button', { name: /^Add (the first )?part$/ })
    .first()
    .click();

  const form = parts.getByRole('form', { name: 'Add part' });
  await form.getByRole('radio', { name: /replay:demo-ld19/ }).check();
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
