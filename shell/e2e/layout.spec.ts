import { expect, test, type Page } from '@playwright/test';

/**
 * No window may cut its content off. A window's body never scrolls itself, so anything taller
 * than it must sit in a scrolling area inside the window. This opens every program at the
 * project's screen size and at a small browser window, and checks that no body overflows.
 */
async function clipped(page: Page): Promise<string[]> {
  return page.locator('[data-window-kind] .body').evaluateAll((bodies) =>
    bodies
      .filter((b) => b.scrollHeight > b.clientHeight + 1 || b.scrollWidth > b.clientWidth + 1)
      .map((b) => {
        const kind = b.closest('[data-window-kind]')?.getAttribute('data-window-kind');
        return `${kind}: ${b.scrollWidth}×${b.scrollHeight} in ${b.clientWidth}×${b.clientHeight}`;
      }),
  );
}

async function openEveryProgram(page: Page) {
  const start = page.getByRole('button', { name: 'Start' });
  await start.click();
  const kinds = await page
    .locator('[role="group"][aria-label="Programs"] [role="menuitem"][data-kind]')
    .evaluateAll((items) => items.map((i) => i.getAttribute('data-kind')!));
  await page.keyboard.press('Escape');
  for (const kind of kinds) {
    await start.click();
    await page.locator(`[role="menuitem"][data-kind="${kind}"]`).first().click();
    await expect(page.locator(`[data-window-kind="${kind}"]`).first()).toBeVisible();
  }
  return kinds;
}

test('no window cuts its content off, also after the browser shrinks', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected'),
  ).toBeVisible();
  const kinds = await openEveryProgram(page);
  expect(kinds.length).toBeGreaterThan(3);
  expect(await clipped(page)).toEqual([]);

  await page.setViewportSize({ width: 760, height: 520 });
  await expect.poll(() => clipped(page)).toEqual([]);
});
