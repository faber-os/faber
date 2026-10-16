import { expect, test } from '@playwright/test';

/**
 * Things found while testing a real Arduino Mega: a port problem can be dismissed, a part can be
 * paused (here a simulated one; the FaberLink board's pause is tested in core/tests/bridge.rs),
 * several desktop icons can be selected with a rubber band, and a file dropped on the desktop
 * lands in the Desktop folder. Needs a running core (see playwright.config.ts).
 */
const PART = 'e2e_paused_front';
const DROPPED = 'e2e-dropped.txt';

test.beforeEach(async ({ request }) => {
  await request.delete(`/api/v0/parts/${PART}`);
  await request.post('/api/v0/files/delete', { data: { paths: [`home:/Desktop/${DROPPED}`] } });
});

test.afterEach(async ({ request }) => {
  await request.delete(`/api/v0/parts/${PART}`);
  await request.post('/api/v0/files/delete', { data: { paths: [`home:/Desktop/${DROPPED}`] } });
});

test('a port problem can be dismissed', async ({ page }) => {
  // The core only reports problems the computer really has, so add one to what it sends.
  const problem = {
    code: 'usb-no-answer',
    label: "Plugged in, but the computer can't talk to it",
    detail: 'Something is plugged in over USB, but it did not answer.',
    hint: 'Try another USB cable.',
  };
  await page.routeWebSocket(/\/api\/v0\/ws/, (ws) => {
    const server = ws.connectToServer();
    ws.onMessage((m) => server.send(m));
    server.onMessage((m) => {
      if (typeof m !== 'string') return ws.send(m);
      const msg = JSON.parse(m) as { type?: string; problems?: unknown[] };
      if (msg.type === 'ports') msg.problems = [problem, problem];
      ws.send(JSON.stringify(msg));
    });
  });
  await page.goto('/?wire=json');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();
  await page.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Serial monitor/ }).click();
  const monitor = page.getByRole('dialog', { name: 'Serial monitor' });
  const notice = monitor.locator('[data-code="usb-no-answer"]');
  await expect(notice).toContainText('(2 times)');
  await notice.getByRole('button', { name: 'Dismiss' }).click();
  await expect(notice).toHaveCount(0);
  // Gone after a reload too: it is remembered in this browser.
  await page.reload();
  await expect(taskbar.getByText('Connected')).toBeVisible();
  await page.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Serial monitor/ }).click();
  await expect(page.getByRole('dialog', { name: 'Serial monitor' })).toBeVisible();
  await expect(page.locator('[data-code="usb-no-answer"]')).toHaveCount(0);
});

test('a part can be paused and resumed from Parts', async ({ page, request }) => {
  const res = await request.post('/api/v0/parts', {
    data: { name: PART, connector: 'generic-distance', simulate: 'manual' },
  });
  expect(res.status()).toBe(201);
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();
  // Parts opens by itself when the shell starts (step 1 of the ladder).
  await expect(page.getByRole('dialog', { name: 'Parts' })).toBeVisible();
  const row = page.locator(`li[data-part="${PART}"]`);
  await expect(row.getByText('Running')).toBeVisible();
  await row.click({ button: 'right', position: { x: 20, y: 10 } });
  await page.getByRole('menuitem', { name: 'Pause' }).click();
  await expect(row.getByText('Paused', { exact: true })).toBeVisible();
  const parts = await (await request.get('/api/v0/parts')).json();
  expect(parts.find((p: { name: string }) => p.name === PART).state.kind).toBe('paused');
  await row.getByRole('button', { name: `Resume ${PART}` }).click();
  await expect(row.getByText('Running')).toBeVisible();
});

test('a rubber band selects two desktop icons', async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('navigation', { name: 'Taskbar' }).getByText('Connected'),
  ).toBeVisible();
  // Parts opens by itself when the shell starts; on a small screen it covers the desktop.
  await page.getByRole('button', { name: 'Minimise Parts' }).click();
  const desktop = page.getByRole('listbox', { name: 'Desktop' });
  const icons = desktop.getByRole('option');
  await expect(icons.first()).toBeVisible();
  // The first column's first two cells start at (12, 12) and (12, 108); start left of them.
  const box = (await desktop.boundingBox())!;
  await page.mouse.move(box.x + 4, box.y + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + 60, box.y + 80, { steps: 4 });
  await page.mouse.move(box.x + 60, box.y + 150, { steps: 4 });
  await expect(page.getByTestId('rubber-band')).toBeVisible();
  await page.mouse.up();
  await expect(desktop.locator('[role="option"][aria-selected="true"]')).toHaveCount(2);
});

test('a file dropped on the desktop is in the Desktop folder', async ({ page }) => {
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();
  const dataTransfer = await page.evaluateHandle((name) => {
    const dt = new DataTransfer();
    dt.items.add(new File(['dropped from e2e'], name, { type: 'text/plain' }));
    return dt;
  }, DROPPED);
  const desktop = page.getByRole('listbox', { name: 'Desktop' });
  await desktop.dispatchEvent('dragover', { dataTransfer });
  await desktop.dispatchEvent('drop', { dataTransfer });
  await expect(desktop.getByRole('option', { name: DROPPED })).toBeVisible();

  await page.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  // Files may open in Desktop already; otherwise go there.
  const place = files.getByRole('button', { name: 'Desktop', exact: true });
  if ((await place.getAttribute('aria-current')) !== 'location') await place.click();
  await expect(
    files.getByRole('option', { name: new RegExp(DROPPED.replace('.', '\\.')) }),
  ).toBeVisible();
});
