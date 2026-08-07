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
 * - **`scenario`** drives the contributor tool `sage-scenario` (issue #777): a hand-authored
 *   position, served on a loopback socket, opened with `?server=`. Non-blocking, like `views`,
 *   and separate from it because it has a real engine behind it and needs a Rust toolchain.
 *
 * Both run against `vite preview` over a real build, never the dev server, so the artifact
 * under test is the one that ships.
 */
import { defineConfig, devices } from '@playwright/test'

// One host for the preview server, the page URL, and (via the page's own origin) the socket
// address the client derives. They must not drift: `vite preview` binds `localhost` by
// default, which in a container resolves to `::1` only, so a page navigating to the IPv4
// literal gets ECONNREFUSED against a server that is genuinely up. Binding explicitly to the
// same literal the tests navigate to removes the ambiguity in both directions.
const HOST = '127.0.0.1'
const PREVIEW_PORT = 4173
const SERVER_PORT = 9000
// The scenario runner's own default, deliberately not 9000 so a scenario can run beside a
// real server. `e2e/scenario.spec.ts` names the same address.
const SCENARIO_PORT = 9010
export const BASE_URL = `http://${HOST}:${PREVIEW_PORT}`

/**
 * The browser is **not resolved here**, deliberately.
 *
 * Playwright already resolves its own matched revision, honouring `PLAYWRIGHT_BROWSERS_PATH`
 * when the environment sets one — which the official container does. `make e2e-browser`
 * guarantees that revision exists everywhere else, so there is nothing left for this file to
 * decide and no `executablePath` to pin.
 *
 * Pinning one is what this config used to do, and it was wrong in a way worth recording: it
 * searched for `<root>/chromium-*\/chrome-linux/chrome`, the **headed** binary, while a headless
 * run launches the **headless shell** (`chromium_headless_shell-*`). In an image carrying both,
 * that pinned local runs to a different executable than CI used — the exact "silently tests a
 * different browser" failure the pin existed to prevent. Deleting it is the fix.
 */

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
  },
  projects: [
    // The views tier is more than one file — `views.spec.ts` and anything named `*.views.spec.ts`
    // beside it — because breadth grows and one spec file does not stay readable past a
    // thousand lines (`docs/coding-standards.md`). The suffix is the whole membership rule.
    { name: 'views', testMatch: /views\.spec\.ts$/ },
    { name: 'smoke', testMatch: /smoke\.spec\.ts/ },
    { name: 'scenario', testMatch: /scenario\.spec\.ts/ },
  ],
  webServer: [
    {
      // The production bundle, served statically — the same artifact CI ships.
      command: `npm run preview -- --host ${HOST} --port ${PREVIEW_PORT} --strictPort`,
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
    // Only the `scenario` tier needs the scenario runner, and it starts it the same way: on
    // the TCP port, since this too is a WebSocket that answers no plain GET. `--no-client`
    // because the preview server above is already serving the built client this points at.
    ...(process.env.SAGE_E2E_SCENARIO === '1'
      ? [
          {
            command:
              'cargo run -q -p sage-scenario -- scenarios/murder-the-dreadmaw.toml ' +
              `--no-client --addr ${HOST}:${SCENARIO_PORT}`,
            cwd: '../..',
            port: SCENARIO_PORT,
            reuseExistingServer: !process.env.CI,
            timeout: 180_000,
          },
        ]
      : []),
  ],
})
