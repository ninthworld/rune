import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright config for the RUNE browser **smoke canary** (issue #279, ADR 0011).
 *
 * This is deliberately *one* spec, not the full ADR 0011 matrix: it drives the real
 * client, in a real Chromium, against a real `rune-server`, far enough to prove the
 * seams the headless unit gate cannot — most importantly that the battlefield
 * `<canvas>` stays attached and paints under React StrictMode (the #276 regression).
 *
 * It runs against the **dev server** (`npm run dev`) on purpose: StrictMode's
 * mount→cleanup→mount double-invoke only happens in development, and that is the
 * path that regressed. The dev server is launched with `VITE_RUNE_TEST_HOOKS=1` so
 * the read-only `window.__RUNE_TEST__` scene/view hooks are compiled in (they are
 * statically stripped from a normal production build).
 *
 * Browser selection:
 * - `RUNE_PW_EXECUTABLE` (set by `make smoke` when a pre-installed Chromium exists,
 *   e.g. `/opt/pw-browsers/chromium`) launches that binary directly, sidestepping a
 *   Playwright-version/browser-revision mismatch and any per-run download.
 * - When unset (CI), Playwright uses its own managed Chromium (installed by the job
 *   with `playwright install chromium`).
 */
const DEV_PORT = Number(process.env.RUNE_WEB_PORT ?? 5199);
const executablePath = process.env.RUNE_PW_EXECUTABLE || undefined;

/**
 * The Pixi battlefield needs a real WebGL context, and headless Chromium has no GPU.
 * These flags route WebGL through SwiftShader (software ANGLE) so the canvas actually
 * paints rather than tripping the client's "board rendering failed" fallback — which
 * is exactly what makes the "attached AND non-blank" assertion meaningful.
 */
const WEBGL_ARGS = [
  '--use-gl=angle',
  '--use-angle=swiftshader',
  '--enable-unsafe-swiftshader',
  '--ignore-gpu-blocklist',
];

export default defineConfig({
  testDir: '.',
  testMatch: /.*\.spec\.ts/,
  // The whole canary is meant to fit in ~1 minute; a generous per-test cap still
  // fails fast rather than hanging CI if a step deadlocks.
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    baseURL: `http://localhost:${DEV_PORT}`,
    trace: 'retain-on-failure',
    ...devices['Desktop Chrome'],
    launchOptions: { args: WEBGL_ARGS, ...(executablePath ? { executablePath } : {}) },
  },
  projects: [{ name: 'chromium' }],
  webServer: {
    command: `npm run dev -- --port ${DEV_PORT} --strictPort`,
    url: `http://localhost:${DEV_PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    env: { VITE_RUNE_TEST_HOOKS: '1' },
  },
});
