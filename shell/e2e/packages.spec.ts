import { readFileSync } from 'node:fs';
import { expect, test, type APIRequestContext } from '@playwright/test';

/**
 * Installing a connector package through the shell, end to end (docs/packages.md §1): a
 * .faberpkg on the desktop is double-clicked in Files, the Install window shows what it is and the
 * "not checked" warning, Install makes its connector appear in Add part at once, a part on a
 * recording gives readings, Settings → Connectors refuses to uninstall it while that part
 * exists, and uninstalls it once the part is gone.
 *
 * (Dropping a file from this computer can't be done from Playwright without a real drag; the
 * unit tests cover drops, and both paths end in the same Install window.)
 *
 * The package is the declarative template packed by `faber pack`
 * (tools/faber-cli/src/pack.rs writes the fixture).
 */
const ID = 'e2e-line-sensor';
const FILE = `${ID}-0.1.0.faberpkg`;
const PACKAGE = `home:/Desktop/${FILE}`;
const RECORDING = `home:/Recordings/${ID}.fbcap`;
const PART = 'e2e_line_sensor';

const fixture = (rel: string) => readFileSync(new URL(rel, import.meta.url));

async function clean(request: APIRequestContext) {
  expect([204, 404]).toContain((await request.delete(`/api/v0/parts/${PART}`)).status());
  expect([204, 404]).toContain((await request.delete(`/api/v0/packages/${ID}`)).status());
  await request.post('/api/v0/files/trash', { data: { paths: [PACKAGE, RECORDING] } });
}

test.beforeEach(async ({ request }) => {
  await clean(request);
  const put = (path: string, body: Buffer) =>
    request.put(`/api/v0/files/content?path=${encodeURIComponent(path)}`, { data: body });
  expect((await put(PACKAGE, fixture('./fixtures/' + FILE))).status()).toBe(204);
  const capture = fixture('../../templates/connector-declarative/captures/sample.fbcap');
  expect((await put(RECORDING, capture)).status()).toBe(204);
});

test.afterEach(async ({ request }) => clean(request));

test('install a package from the desktop, use it, and uninstall it', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected', { exact: true })).toBeVisible();

  // Double-click the package in Files (on the desktop, the Parts window may cover its icon).
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  await files.getByRole('button', { name: 'Desktop', exact: true }).click();
  await files.getByRole('option', { name: FILE }).dblclick();
  const install = page.getByRole('dialog', { name: `Install ${FILE}` });
  await expect(install.getByRole('heading', { name: 'E2e line sensor', level: 3 })).toBeVisible();
  await expect(install).toContainText('Settings only (declarative)');
  await expect(install).toContainText('Your name');
  await expect(install).toContainText('A serial port you pick, at 9600 baud');
  await expect(install).toContainText('Not checked by the Faber registry');
  await expect(install).toContainText('This text is shown in Faber');
  await install.getByRole('button', { name: 'Install', exact: true }).click();
  await expect(install).toContainText('E2e line sensor is installed');

  // Add part opens on the new connector; pick the recording and add it.
  await install.getByRole('button', { name: 'Add part' }).click();
  const parts = page.getByRole('dialog', { name: 'Parts' });
  const form = parts.getByRole('form', { name: 'Add part' });
  await expect(form.getByRole('radio', { name: /E2e line sensor/ })).toBeChecked();
  await form.getByRole('radio', { name: new RegExp(`replay:${ID}`) }).check({ timeout: 10_000 });
  await form.getByLabel('Name').fill(PART);
  await form.getByRole('button', { name: 'Add part' }).click();
  const row = parts.locator(`li[data-part="${PART}"]`);
  await expect(row).toBeVisible();
  await row.getByRole('button', { name: PART, exact: true }).click();
  const viewer = page.getByRole('dialog', { name: `${PART} · distance` });
  await expect(viewer.getByTestId('distance-value')).toBeVisible({ timeout: 15_000 });

  // Settings → Connectors: listed, and not removable while the part uses it.
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.locator('[role="menuitem"][data-kind="settings"]').first().click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  const section = settings.getByRole('region', { name: 'Connectors' });
  await expect(section).toContainText('E2e line sensor');
  await expect(section).toContainText(`Used by 1 part, "${PART}"`);
  const uninstall = section.getByRole('button', { name: 'Uninstall E2e line sensor' });
  await expect(uninstall).toBeDisabled();

  // Remove the part (the list follows the parts), then uninstall.
  expect((await page.request.delete(`/api/v0/parts/${PART}`)).status()).toBe(204);
  await expect(uninstall).toBeEnabled();
  await uninstall.click();
  await page
    .getByRole('alertdialog', { name: 'Uninstall E2e line sensor?' })
    .getByRole('button', { name: 'Uninstall' })
    .click();
  await expect(section).toContainText('No packages installed.');
});
