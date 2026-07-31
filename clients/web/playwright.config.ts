/**
 * Browser end-to-end configuration (ADR 0011).
 *
 * Two projects, and the split is the whole point:
 *
 * - **`smoke`** drives the *real* `sage-server`. It is one thin path — load the built client,
 *   reach a rendered game view, take an action — and it is the blocking gate. Its job is to
 *   catch what only reality catches: the socket, the wire contract, and view reconstruction
 *   actually agreeing.
 * - **`views`** replays committed fixtures over an intercepted WebSocket. No server, no engine,
 *   no game — just "given exactly this view, the browser renders this". Fast, deterministic,
 *   and where breadth lives, so breadth never gates a merge on browser flake.
 *
 * Both run against `vite preview` over a real build, never the dev server, so the artifact
 * under test is the one that ships.
 */
import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { defineConfig, devices } from '@playwright/test'

const PREVIEW_PORT = 4173
const SERVER_PORT = 9000
export const BASE_URL = `http://127.0.0.1:${PREVIEW_PORT}`

/**
 * The browser to drive, resolved from what the image already has.
 *
 * **Never run `playwright install`** — not in CI, not in an agent session. The pinned
 * `@playwright/test` and the image's browser build routinely disagree (this environment ships
 * chromium-1194 while the pinned package expects a later revision), and the fix is to point at
 * what exists rather than to download another copy. Returning `undefined` lets Playwright
 * resolve normally, which is correct inside the official Playwright container where the
 * versions match by construction.
 */
function preinstalledChromium(): string | undefined {
  const root = process.env.PLAYWRIGHT_BROWSERS_PATH
  if (!root || !existsSync(root)) return undefined
  const candidates = readdirSync(root)
    .filter((entry) => entry.startsWith('chromium-'))
    .sort()
    .reverse()
  for (const candidate of candidates) {
    const executable = join(root, candidate, 'chrome-linux', 'chrome')
    if (existsSync(executable)) return executable
  }
  return undefined
}

const executablePath = preinstalledChromium()

export default defineConfig({
  testDir: './e2e',
  // A browser test that hangs should fail, not stall the job.
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    baseURL: BASE_URL,
    trace: 'retain-on-failure',
    ...devices['Desktop Chrome'],
    ...(executablePath ? { launchOptions: { executablePath } } : {}),
  },
  projects: [
    { name: 'views', testMatch: /views\.spec\.ts/ },
    { name: 'smoke', testMatch: /smoke\.spec\.ts/ },
  ],
  webServer: [
    {
      // The production bundle, served statically — the same artifact CI ships.
      command: `npm run preview -- --port ${PREVIEW_PORT} --strictPort`,
      port: PREVIEW_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
    // Only the `smoke` tier talks to a real server, and only it should pay for one. The
    // `views` tier intercepts the socket in the page, so starting `sage-server` for it would
    // mean a Rust toolchain in a job that has no other use for one.
    ...(process.env.SAGE_E2E_SERVER === '1'
      ? [
          {
            // Waits on the TCP port rather than an HTTP response: this is a WebSocket server
            // and answers no plain GET.
            command: 'cargo run -q -p sage-server',
            cwd: '../..',
            port: SERVER_PORT,
            reuseExistingServer: !process.env.CI,
            timeout: 180_000,
          },
        ]
      : []),
  ],
})
