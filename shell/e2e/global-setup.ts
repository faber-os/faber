import { chromium, expect, type FullConfig } from '@playwright/test';
import { ADMIN, STORAGE, signIn } from './auth';

/**
 * Before any test: the core starts with no users, so the first browser gets "Create the admin"
 * (docs/auth.md). This goes through that real screen once and saves the admin's session cookie
 * for every test. A core that already has the admin (one reused while developing) gets a sign-in
 * instead.
 */
export default async function globalSetup(config: FullConfig) {
  const use = config.projects[0]!.use;
  const browser = await chromium.launch({ channel: use.channel });
  const page = await browser.newPage({ baseURL: use.baseURL });
  await page.goto('/');
  const setup = page.getByRole('dialog', { name: 'Create the admin' });
  const signInCard = page.getByRole('dialog', { name: /^Sign in to / });
  await expect(setup.or(signInCard)).toBeVisible({ timeout: 30_000 });
  if (await setup.isVisible()) {
    await setup.getByLabel('Name').fill(ADMIN.name);
    await setup.getByLabel('Password', { exact: true }).fill(ADMIN.password);
    await setup.getByLabel('Repeat the password').fill(ADMIN.password);
    await setup.getByRole('button', { name: 'Create the admin' }).click();
    await expect(
      page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected', { exact: true }),
    ).toBeVisible();
  } else {
    await signIn(page, ADMIN);
  }
  await page.context().storageState({ path: STORAGE });
  await browser.close();
}
