import { expect, test, type APIRequestContext } from '@playwright/test';

/**
 * I²C, end to end, on the core's fake buses (`faber-core --fake-hardware`): scan i2c-1 in the
 * I2C scanner, add the TMP102 it suggests at 0x48 (the `ti-tmp102` example connector, installed
 * in the test data folder by playwright.config.ts), see its temperature, and plot it.
 */
const NAME = 'e2e_cabin_temp';

async function clean(request: APIRequestContext) {
  expect([204, 404]).toContain((await request.delete(`/api/v0/parts/${NAME}`)).status());
}

test.beforeEach(async ({ request }) => clean(request));
test.afterEach(async ({ request }) => clean(request));

test('scan, add the TMP102 from the suggestion, see its temperature and plot it', async ({
  page,
}) => {
  const system = await (await page.request.get('/api/v0/system')).json();
  expect(system.buses, 'the e2e core runs with --fake-hardware').toBe('fake');

  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^I2C scanner/ }).click();
  const scanner = page.locator('[data-window-kind="i2c-scanner"]');

  // Nothing is scanned until asked.
  await expect(scanner.getByTestId('i2c-0x48')).toHaveCount(0);
  await scanner.getByRole('button', { name: 'Scan', exact: true }).click();
  await expect(scanner.getByTestId('i2c-0x48')).toHaveText('48');
  await expect(scanner.getByTestId('i2c-0x76')).toHaveText('76');
  await expect(scanner).toContainText('common default addresses');
  const found = scanner.getByTestId('i2c-found-0x48');
  await expect(found).toContainText('Maybe a TI TMP102 temperature sensor');
  await found.getByRole('button', { name: /Add a TI TMP102 .* at 0x48/ }).click();

  const parts = page.getByRole('dialog', { name: 'Parts' });
  const form = parts.getByRole('form', { name: 'Add part' });
  await expect(form.getByRole('radio', { name: /i2c-1/ })).toBeChecked();
  await expect(form.getByRole('radio', { name: /ti-tmp102/ })).toBeChecked();
  await expect(form.getByRole('radio', { name: '0x48' })).toBeChecked();
  await form.getByLabel('Name').fill(NAME);
  await form.getByRole('button', { name: 'Add part' }).click();

  await parts.getByRole('button', { name: `Open the viewer for ${NAME}/temperature` }).click();
  const viewer = page.locator('[data-window-kind="temperature"]');
  // The fake TMP102 drifts around 21.5 °C.
  await expect(viewer.getByTestId('temperature-value')).toHaveText(/^(19|2\d)\.\d °C$/);

  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Signal plotter/ }).click();
  const plotter = page.locator('[data-window-kind="signal-plotter"]');
  await plotter.getByRole('combobox', { name: 'Signal' }).selectOption(`${NAME}/temperature`);
  await expect(plotter.getByRole('combobox', { name: 'Value' })).toHaveValue('celsius');
  await plotter.getByRole('button', { name: 'Plot', exact: true }).click();
  await expect(
    plotter.getByRole('button', { name: `Stop plotting ${NAME}/temperature celsius` }),
  ).toBeVisible();
  // One reading a second: wait for two, so there is a line.
  await expect
    .poll(
      async () =>
        (await plotter.locator('[data-testid="plotter-plot"] polyline').getAttribute('points')) ??
        '',
      { timeout: 10_000 },
    )
    .toContain(' ');
  const download = page.waitForEvent('download');
  await plotter.getByRole('button', { name: 'Save as CSV' }).click();
  const csv = await download;
  expect(csv.suggestedFilename()).toBe('faber-signals.csv');
});
