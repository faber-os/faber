import { expect, type Page } from '@playwright/test';

/**
 * The end-to-end tests' users (docs/auth.md). The core they start has none (playwright.config.ts
 * removes its users.toml), so global-setup.ts creates the admin through the real first-run
 * screen and every test reuses its session cookie (STORAGE).
 */
export const ADMIN = { name: 'e2e-admin', password: 'e2e admin password' };
export const VIEWER = { name: 'e2e-viewer', password: 'e2e viewer password' };
export const STORAGE = 'e2e/.auth/admin.json';

/** Signs in on the sign-in screen and waits for the desktop. */
export async function signIn(page: Page, who: { name: string; password: string }) {
  const card = page.getByRole('dialog', { name: /^Sign in to / });
  await expect(card).toBeVisible();
  await card.getByLabel('Name').fill(who.name);
  await card.getByLabel('Password').fill(who.password);
  await card.getByRole('button', { name: 'Sign in' }).click();
  await expect(
    page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected', { exact: true }),
  ).toBeVisible();
}
