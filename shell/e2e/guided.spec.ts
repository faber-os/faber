import { expect, test, type Page } from '@playwright/test';

/**
 * Guided mode end to end (docs/wiring.md §3): Parts → Add part → On a board's pins, a new Uno,
 * an ultrasonic sensor on the pins Faber suggests, the plan saved, the wiring checklist, and the
 * generated sketch in the Arduino editor (read-only, with its bar), verified with the core's
 * stand-in for arduino-cli. Nothing is uploaded: the test core can see real boards.
 */
const SKETCHBOOK = 'home:/Arduino';

test.afterEach(async ({ request }) => {
  await request.post('/api/v0/files/delete', { data: { paths: [SKETCHBOOK] } });
});

/** No window's body may cut its content off (as layout.spec.ts checks every window). */
async function clipped(page: Page): Promise<string[]> {
  return page
    .locator('[data-window-kind] .body')
    .evaluateAll((bodies) =>
      bodies
        .filter((b) => b.scrollHeight > b.clientHeight + 1 || b.scrollWidth > b.clientWidth + 1)
        .map((b) => b.closest('[data-window-kind]')?.getAttribute('data-window-kind') ?? '?'),
    );
}

test('add an ultrasonic sensor to a new guided Uno, then verify its sketch', async ({ page }) => {
  const board = `rover${Date.now() % 100000}`;
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  const parts = page.getByRole('dialog', { name: 'Parts' });
  // Parts may still be open from another test (the desktop is remembered): the taskbar button
  // would then hide it.
  if (!(await parts.isVisible()))
    await taskbar.getByRole('button', { name: 'Parts', exact: true }).click();
  await expect(parts).toBeVisible();
  await parts
    .getByRole('button', { name: /^Add (the first )?part$/ })
    .first()
    .click();
  await parts.getByRole('radio', { name: 'On a board’s pins' }).check();

  // A new board: an Uno, named.
  await parts.getByRole('radio', { name: /A new board/ }).check();
  await parts.getByLabel('Board type').selectOption('uno');
  await parts.getByLabel('Board name').fill(board);
  await parts.getByRole('button', { name: 'Next' }).click();

  // The part, with the default name and settings.
  await expect(parts.getByText(`What goes on ${board}?`)).toBeVisible();
  await parts.getByRole('radio', { name: /^Ultrasonic distance sensor/ }).check();
  await expect(parts.getByLabel('Name')).toHaveValue('front');
  await parts.getByRole('button', { name: 'Choose pins' }).click();

  // The pins Faber suggests are filled in: the drawing says so, pin by pin.
  const drawing = parts.getByRole('group', { name: 'The pins of the Arduino Uno' });
  await expect(drawing).toBeVisible();
  const chosen = drawing.getByRole('button', { name: /, chosen for Trig$/ });
  await expect(chosen).toHaveCount(1);
  const trigPin = (await chosen.getAttribute('data-pin'))!;
  await expect(
    drawing.getByRole('button', { name: /^D0, reserved: .*USB link to Faber/ }),
  ).toBeVisible();
  expect(await clipped(page)).toEqual([]);
  await parts.getByRole('button', { name: 'Save the plan' }).click();

  // The wiring checklist, with the chosen pins.
  await expect(parts.getByText(`Wiring ${board}`)).toBeVisible();
  await expect(parts.getByRole('checkbox', { name: `Trig → ${trigPin}` })).toBeVisible();
  await parts.getByRole('checkbox', { name: 'VCC → 5V' }).check();
  await expect(parts.getByText('1 of 4 wires done')).toBeVisible();
  await parts.getByRole('button', { name: 'Next: upload' }).click();

  // The generated sketch opens read-only in the editor, with the bar.
  await parts.getByRole('button', { name: 'Open the sketch' }).click();
  const editor = page.locator('[data-window-kind="arduino"]');
  const code = editor.getByRole('textbox', { name: `Code of ${board}.ino` });
  await expect(code).toContainText(`FaberUltrasonic front("front", ${trigPin.slice(1)}`);
  await expect(code).toHaveAttribute('aria-readonly', 'true');
  await expect(editor.getByRole('note')).toContainText(
    'Faber writes this file from the board’s plan.',
  );
  await expect(editor.getByLabel('Board')).toHaveValue('uno');
  await editor.getByRole('button', { name: 'Verify' }).click();
  await expect(editor.getByText('Compiled for the Arduino Uno.')).toBeVisible();

  // my_code.h is yours, and opens normally.
  await editor.getByRole('tab', { name: 'my_code.h' }).click();
  await expect(editor.getByRole('note')).toHaveCount(0);
  await expect(editor.getByRole('textbox', { name: 'Code of my_code.h' })).not.toHaveAttribute(
    'aria-readonly',
  );
});
