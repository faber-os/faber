import { expect, test } from '@playwright/test';

/**
 * The Arduino editor end to end (docs/arduino-editor.md): a new sketch from the start menu,
 * typing in the editor, Verify with an error that points at its line, Verify again with the size
 * report, and the sketch opened again from Files. The core runs a stand-in for arduino-cli
 * (`--fake-arduino-cli`, see playwright.config.ts): a line with `#error` fails like avr-gcc does.
 * No board: uploading and the same-code check are tested through the API with simulated
 * bootloaders (core/tests/arduino.rs).
 */
test.afterEach(async ({ request }) => {
  await request.post('/api/v0/files/delete', { data: { paths: ['home:/Arduino'] } });
});

test('write a sketch, see its error at the line, fix it and verify', async ({ page, request }) => {
  const name = `Rover${Date.now()}`;
  await page.goto('/');
  const taskbar = page.getByRole('navigation', { name: 'Taskbar' });
  await expect(taskbar.getByText('Connected')).toBeVisible();

  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.locator('[role="menuitem"][data-kind="arduino"]').first().click();
  const editor = page.locator('[data-window-kind="arduino"]');
  await editor.getByRole('button', { name: 'New sketch' }).click();
  const dialog = page.getByRole('alertdialog', { name: 'New sketch' });
  await dialog.getByLabel('Name').fill(name);
  await dialog.getByRole('radio', { name: /FaberLink board/ }).check();
  await dialog.getByRole('button', { name: 'Create' }).click();

  await expect(editor.getByRole('tab', { name: `${name}.ino` })).toBeVisible();
  // The board is picked from a port's USB ids when one names it; this test compiles for an Uno.
  await editor.getByLabel('Board').selectOption('uno');
  const code = editor.getByRole('textbox', { name: `Code of ${name}.ino` });
  await expect(code).toContainText('link.begin(115200');

  // A broken line at the end: Verify saves the tab first, then lists the error at its line.
  await code.click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n#error the motor is not wired yet');
  await expect(editor.getByRole('tab', { name: `${name}.ino, not saved` })).toBeVisible();
  await editor.getByRole('button', { name: 'Verify' }).click();
  await expect(editor.getByText('The sketch didn’t compile: one error.')).toBeVisible();
  await expect(editor.getByRole('tab', { name: `${name}.ino` })).toBeVisible();
  const saved = await request.get(`/api/v0/files/content?path=home:/Arduino/${name}/${name}.ino`);
  expect(await saved.text()).toContain('#error the motor is not wired yet');

  const lines = (await saved.text()).split('\n');
  const line = lines.findIndex((l) => l.includes('#error')) + 1;
  await editor.getByRole('button', { name: `Go to ${name}.ino:${line}:2` }).click();
  await expect(editor.locator('.cm-activeLine')).toContainText('#error the motor is not wired yet');

  // Undo the broken line (the editor keeps its history), and verify again.
  await page.keyboard.press('Control+z');
  await editor.getByRole('button', { name: 'Verify' }).click();
  await expect(editor.getByText('Compiled for the Arduino Uno.')).toBeVisible();
  await expect(editor.getByText(/^Program: [\d  ]+ bytes of 32[  ]256/)).toBeVisible();

  // The sketch in Files opens in this same window.
  await taskbar.getByRole('button', { name: 'Start' }).click();
  await page.getByRole('menuitem', { name: /^Files/ }).click();
  const files = page.getByRole('dialog', { name: /Files$/ });
  await files.getByRole('button', { name: 'Home', exact: true }).click();
  await files.getByRole('option', { name: /^Arduino/ }).dblclick();
  await files.getByRole('option', { name: new RegExp(`^${name}`) }).dblclick();
  await files.getByRole('option', { name: new RegExp(`^${name}\\.ino`) }).dblclick();
  await expect(page.locator('[data-window-kind="arduino"]')).toHaveCount(1);
});
