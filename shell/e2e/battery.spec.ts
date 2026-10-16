import { expect, test, type APIRequestContext } from '@playwright/test';

/**
 * A battery, end to end (docs/capabilities.md, battery@1): a manual simulated `generic-battery`
 * is set low with its slider in Parts; its viewer shows the voltage and the level in words, and
 * the core's notice about the level change pops up as a toast. Then its drain pattern runs it
 * down.
 */
const NAME = 'e2e_battery';

async function clean(request: APIRequestContext) {
  expect([204, 404]).toContain((await request.delete(`/api/v0/parts/${NAME}`)).status());
}

test.beforeEach(async ({ request }) => {
  await clean(request);
  const res = await request.post('/api/v0/parts', {
    data: {
      name: NAME,
      connector: 'generic-battery',
      simulate: 'manual',
      settings: { 'rate-hz': 10 },
    },
  });
  expect(res.status()).toBe(201);
});

test.afterEach(async ({ request }) => clean(request));

test('a manual battery set low shows in its viewer and raises a notice', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected', { exact: true })).toBeVisible();
  // The shell starts with Parts open.
  const parts = page.getByRole('dialog', { name: 'Parts' });

  // The slider is the pack's voltage, from empty (9 V) to full (12.6 V) for 3S Li-ion.
  const slider = parts.getByRole('slider', { name: `Value of ${NAME}` });
  await expect(slider).toHaveAttribute('min', '9');
  await expect(slider).toHaveAttribute('max', '12.6');
  await expect(slider).toBeEnabled();
  await parts.getByRole('button', { name: `Open the viewer for ${NAME}/battery` }).click();
  const viewer = page.locator('[data-window-kind="battery"]');
  await expect(viewer.getByTestId('battery-level')).toHaveText('OK');
  await expect(viewer.getByTestId('battery-volts')).toHaveText('12.3 V');
  await expect(viewer).toContainText('Low below 10.5 V, critical below 9.9 V');

  // A value on the slider's steps (9 V + 67 × 0.018 V).
  await slider.fill('10.206');
  await expect(viewer.getByTestId('battery-level')).toHaveText('Low: charge it soon');
  await expect(viewer.getByTestId('battery-volts')).toHaveText(/^10\.2\d? V$/);
  const notices = page.getByRole('region', { name: 'Notices' });
  await expect(notices).toContainText(`The battery of ${NAME} is low`);
  await notices.getByRole('button', { name: 'Dismiss' }).click();
  await expect(notices).toBeHidden();

  // Drain: from the slider's value down to empty, then critical.
  await parts.getByRole('combobox', { name: `Pattern of ${NAME}` }).selectOption('drain');
  await expect
    .poll(async () => await (await page.request.get(`/api/v0/parts/${NAME}/simulation`)).json())
    .toMatchObject({ pattern: { kind: 'drain', to: 9, periodS: 120 } });
});
