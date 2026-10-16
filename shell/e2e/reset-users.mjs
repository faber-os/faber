// Run before the end-to-end core starts (playwright.config.ts): a core without users, so
// global-setup.ts goes through the real "Create the admin" screen every run (docs/auth.md).
import { rmSync } from 'node:fs';

rmSync('target/e2e-data/config/users.toml', { force: true });
