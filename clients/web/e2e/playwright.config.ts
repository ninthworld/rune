/**
 * Playwright configuration for the RUNE web client e2e suite (ADR 0011).
 *
 * The suite runs against the **real production client**: `vite build` (with the
 * test-hook flag) followed by `vite preview`, so the artifact under test is the
 * same bundle CI ships, only with the read-only scene hook compiled in. Only
 * Chromium is driven — the client targets evergreen browsers and the value here
 * is integration coverage, not a cross-browser matrix (ADR 0011). The browser is
 * the one already installed in the toolchain (found via `PLAYWRIGHT_BROWSERS_PATH`);
 * this config never downloads one.
 */
import { defineConfig, devices } from '@playwright/test';

/** Port the built client is served on for the run. */
const PREVIEW_PORT = 4173;

/**
 * Optional explicit Chromium binary. Unset in normal CI, where Playwright
 * resolves the browser revision it pins via `playwright install`. Set it (e.g. to
 * a toolchain-preinstalled Chromium) when the environment ships a browser whose
 * revision differs from the pin, to run against that binary without a download.
 */
const executablePath = process.env.RUNE_E2E_CHROMIUM_PATH || undefined;

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  // Fully deterministic: no retries, and the whole suite must finish well under
  // the 2-minute budget in the issue.
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [['github'], ['list']] : 'list',
  timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${PREVIEW_PORT}`,
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        ...(executablePath ? { launchOptions: { executablePath } } : {}),
      },
    },
  ],
  // Build the client with the scene hook, then serve the production bundle with
  // `vite preview`. `webServer` waits for the URL before any test runs (no sleeps).
  webServer: {
    command: `npm run build && npm run preview -- --port ${PREVIEW_PORT} --strictPort`,
    url: `http://127.0.0.1:${PREVIEW_PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: { VITE_RUNE_TEST_HOOKS: '1' },
  },
});
