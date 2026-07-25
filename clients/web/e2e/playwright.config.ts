/**
 * Playwright configuration for the RUNE browser suite (ADR 0011).
 *
 * Scope: two specs, not the full ADR 0011 matrix — the smoke canary (issue #279)
 * and the four-player vertical slice (issue #499). The canary exists because the
 * canvas-vanishes-in-dev bug shipped while CI was green — jsdom cannot see a
 * render path, so nothing below a real browser can catch it; the slice exists
 * because multi-seat play was proven only by jsdom component tests.
 *
 * **Two targets, on purpose** (see `support/targets.ts`). ADR 0011 specifies the
 * production *preview* build as what the suite runs against, and the four-player
 * slice runs there. The canary is the one exception: the failure it guards
 * against is React StrictMode's development-only double-invoke of effects, which
 * a production bundle does not perform at all, so a preview-only canary would be
 * structurally incapable of seeing its own regression. It therefore runs the dev
 * server — the configuration that actually regressed. Both targets are
 * built/served with `VITE_RUNE_TEST_HOOKS` on so the read-only
 * `window.__RUNE_TEST__` hook is compiled in.
 *
 * The suite is deliberately outside `make check`: it needs a browser, a Vite
 * server, and the `rune-server` binary. It rides `make e2e` / `make verify` and
 * its own CI job.
 */
import { defineConfig, devices } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import { DEV_PORT, DEV_URL, PREVIEW_OUT_DIR, PREVIEW_PORT, PREVIEW_URL } from './support/targets';

/** `clients/web` — where both Vite servers run from. */
const CLIENT_DIR = fileURLToPath(new URL('..', import.meta.url));

/** Compiles in the read-only `window.__RUNE_TEST__` hook (ADR 0011). Vite picks
 * up inline `VITE_*` process env, so no committed `.env` file is needed — and the
 * flag can never leak into a production build from here. */
const HOOK_ENV = { VITE_RUNE_TEST_HOOKS: '1' };

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  // Serial: each spec launches its own `rune-server` and several browser contexts,
  // so running them at once would only make the CI runner contend with itself and
  // the failure output unreadable.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  // No retries anywhere: a browser test that passes on the second attempt is
  // telling us something, and hiding it defeats the point of having one.
  retries: 0,
  // The default budget is the canary's; the four-player slice raises its own with
  // `test.setTimeout` because it plays the same scenario twice across four live
  // browser contexts.
  timeout: 120_000,
  expect: { timeout: 20_000 },
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    // The canary's target. The four-player slice navigates to the preview origin
    // explicitly (`support/targets.ts`), so it does not rely on this default.
    baseURL: DEV_URL,
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
  webServer: [
    {
      command: `npm run dev -- --host 127.0.0.1 --port ${DEV_PORT} --strictPort`,
      cwd: CLIENT_DIR,
      url: DEV_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      stdout: 'pipe',
      stderr: 'pipe',
      env: HOOK_ENV,
    },
    {
      // A real production build, emitted beside `dist/` so `npm run budget` keeps
      // measuring the shipped artifact rather than this hooks-enabled one.
      command:
        `npx vite build --outDir ${PREVIEW_OUT_DIR} && ` +
        `npx vite preview --outDir ${PREVIEW_OUT_DIR} --host 127.0.0.1 --port ${PREVIEW_PORT} --strictPort`,
      cwd: CLIENT_DIR,
      url: PREVIEW_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 240_000,
      stdout: 'pipe',
      stderr: 'pipe',
      env: HOOK_ENV,
    },
  ],
});
