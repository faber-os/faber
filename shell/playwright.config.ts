import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end test against the real thing: the built shell compiled into `faber-core`
 * (`--features embed-shell`), exactly as a release runs it. Playwright builds and starts it.
 *
 * Browsers: `npx playwright install chromium` once, or set FABER_E2E_CHANNEL=chrome (or msedge)
 * to use a browser that is already installed.
 */
const channel = process.env.FABER_E2E_CHANNEL || undefined;
// FABER_E2E_PORT runs the test core beside one that is already using 7878 (e.g. your own).
const port = process.env.FABER_E2E_PORT || '7878';
export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  // One machine config is shared by every test, so they must not run at the same time.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    channel,
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'desktop',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } },
    },
    {
      name: 'kiosk-7in',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1024, height: 600 } },
    },
  ],
  webServer: {
    // --fake-hardware: simulated I²C, SPI and GPIO (a TMP102 on i2c-1), never real buses. The
    // TMP102 example connector is installed in the test data folder for the I²C test.
    command:
      `node -e "require('fs').cpSync('examples/connectors/ti-tmp102', 'target/e2e-data/connectors/ti-tmp102', { recursive: true })" && ` +
      'npm --prefix shell run build && cargo run -p faber-core --features embed-shell -- ' +
      `--data target/e2e-data --listen 127.0.0.1:${port} --fake-hardware`,
    cwd: '..',
    url: `http://127.0.0.1:${port}/api/v0/system`,
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
  },
});
