/**
 * Playwright fixtures for the browser suite: real `rune-server` processes, torn
 * down whatever the outcome, with their logs attached on failure.
 *
 * Two fixtures, because the two specs need different lifetimes:
 *
 * - `runeServer` — one pre-launched, default-configured server (the canary).
 * - `launchRuneServer` — a factory, for a spec that runs the same scenario more
 *   than once and wants each pass on its own fresh, identically seeded server
 *   (the four-player slice's reduced-motion re-run). Every server it hands out
 *   is stopped at teardown.
 */
import { test as base } from '@playwright/test';
import { startRuneServer, type RuneServerOptions, type RuneServerProcess } from './runeServer';

/** Fixtures this suite adds on top of Playwright's own. */
interface RuneFixtures {
  /** A freshly launched, seeded `rune-server` on an OS-assigned port. */
  runeServer: RuneServerProcess;
  /** Launch another seeded `rune-server`; all of them are torn down after. */
  launchRuneServer: (options?: RuneServerOptions) => Promise<RuneServerProcess>;
}

export const test = base.extend<RuneFixtures>({
  runeServer: async ({}, use, testInfo) => {
    const server = await startRuneServer();
    await use(server);
    if (testInfo.status !== testInfo.expectedStatus) {
      await testInfo.attach('rune-server.log', {
        body: server.log(),
        contentType: 'text/plain',
      });
    }
    await server.stop();
  },

  launchRuneServer: async ({}, use, testInfo) => {
    const started: RuneServerProcess[] = [];
    await use(async (options) => {
      const server = await startRuneServer(options);
      started.push(server);
      return server;
    });
    for (const [index, server] of started.entries()) {
      if (testInfo.status !== testInfo.expectedStatus) {
        await testInfo.attach(`rune-server-${index}.log`, {
          body: server.log(),
          contentType: 'text/plain',
        });
      }
      await server.stop();
    }
  },
});

export { expect } from '@playwright/test';
