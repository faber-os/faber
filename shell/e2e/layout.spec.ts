import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

/**
 * No window may cut its content off. A window's body never scrolls itself, so anything taller
 * than it must sit in a scrolling area inside the window. This opens every program, and the
 * viewers of a simulated drive base and distance sensor, at the project's screen size and at a
 * small browser window, and checks that no body overflows.
 */
const SIM_PARTS = [
  { name: 'layout_base', connector: 'generic-planar-base' },
  { name: 'layout_front', connector: 'generic-distance' },
];

async function removeSimParts(request: APIRequestContext) {
  for (const p of SIM_PARTS) await request.delete(`/api/v0/parts/${p.name}`);
}

test.beforeEach(async ({ request }) => {
  await removeSimParts(request);
  for (const p of SIM_PARTS) {
    const res = await request.post('/api/v0/parts', { data: { ...p, simulate: 'manual' } });
    expect(res.status()).toBe(201);
  }
});

test.afterEach(async ({ request }) => removeSimParts(request));

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
  // The start button on the taskbar (Drive's own question has a Start button too).
  const start = page.getByRole('navigation', { name: 'Taskbar' }).getByRole('button', {
    name: 'Start',
  });
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

/** The viewers opened from Parts, which the start menu doesn't list. */
async function openSimViewers(page: Page) {
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  const parts = page.getByRole('dialog', { name: 'Parts' });
  for (const address of ['layout_base/planar-base', 'layout_front/distance']) {
    await taskbar.getByRole('button', { name: 'Parts', exact: true }).click();
    await parts.getByRole('button', { name: `Open the viewer for ${address}` }).click();
  }
  await expect(page.locator('[data-window-kind="planar-base"]')).toBeVisible();
  await expect(page.locator('[data-window-kind="distance"]')).toBeVisible();
}

test('no window cuts its content off, also after the browser shrinks', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected'),
  ).toBeVisible();
  const kinds = await openEveryProgram(page);
  expect(kinds).toContain('drive');
  expect(kinds).toContain('arduino-flasher');
  await openSimViewers(page);
  expect(await clipped(page)).toEqual([]);

  await page.setViewportSize({ width: 760, height: 520 });
  await expect.poll(() => clipped(page)).toEqual([]);
});
