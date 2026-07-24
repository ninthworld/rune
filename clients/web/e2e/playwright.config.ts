/**
 * Playwright configuration for the RUNE browser smoke suite (ADR 0011, issue #279).
 *
 * Scope: one canary spec, not the full ADR 0011 matrix. It exists because the
 * canvas-vanishes-in-dev bug shipped while CI was green — jsdom cannot see a
 * render path, so nothing below a real browser can catch it.
 *
 * **Why the dev server and not `vite preview`.** ADR 0011 specifies the preview
 * build for the fixture/mock tier, and that still stands for the wider suite.
 * The canary is different: the failure it guards against is React StrictMode's
 * development-only double-invoke of effects, which a production bundle does not
 * perform at all. Running the canary against a production preview would make it
 * structurally incapable of seeing its own regression. It therefore runs the dev
 * server — the configuration that actually regressed — with
 * `VITE_RUNE_TEST_HOOKS` on so the read-only `window.__RUNE_TEST__` hook is
 * compiled in.
 *
 * The suite is deliberately outside `make check`: it needs a browser, a Vite
 * server, and the `rune-server` binary. It rides `make e2e` / `make verify` and
 * its own CI job.
 */
import { defineConfig, devices } from '@playwright/test';
import { fileURLToPath } from 'node:url';

/** `clients/web` — where the Vite dev server runs from. */
const CLIENT_DIR = fileURLToPath(new URL('..', import.meta.url));

/**
 * Fixed dev-server port. Not the Vite default, so a developer's own `npm run dev`
 * is never mistaken for the suite's server (and vice versa). Override with
 * `RUNE_E2E_PORT`.
 */
const PORT = Number(process.env.RUNE_E2E_PORT ?? 5179);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  // One canary spec; serial keeps the CI cost predictable and the failure output
  // readable. Growth beyond this belongs to the wider ADR 0011 suite.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  // No retries anywhere: a canary that passes on the second attempt is telling us
  // something, and hiding it defeats the point of the canary.
  retries: 0,
  timeout: 120_000,
  expect: { timeout: 20_000 },
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    baseURL: BASE_URL,
    // A desktop viewport: below 900px the table takes its compact composition,
    // which is a different (also valid) layout the canary does not speak for.
    viewport: { width: 1280, height: 900 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        // The full Chromium build rather than the headless shell: the scene's
        // effects overlay wants a real WebGL context, and SwiftShader gives a
        // deterministic software one on a GPU-less CI runner.
        channel: 'chromium',
        launchOptions: {
          args: [
            '--use-gl=angle',
            '--use-angle=swiftshader',
            '--enable-unsafe-swiftshader',
            '--disable-lcd-text',
          ],
        },
      },
    },
  ],
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${PORT} --strictPort`,
    cwd: CLIENT_DIR,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    stdout: 'pipe',
    stderr: 'pipe',
    // Compiles in the read-only `window.__RUNE_TEST__` hook (ADR 0011). Vite
    // picks up inline `VITE_*` process env, so no committed `.env` file is
    // needed — and the flag can never leak into a production build from here.
    env: { VITE_RUNE_TEST_HOOKS: '1' },
  },
});
