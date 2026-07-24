/**
 * Playwright fixtures for the smoke suite: one real `rune-server` per test,
 * torn down whatever the outcome, with its log attached on failure.
 */
import { test as base } from '@playwright/test';
import { startRuneServer, type RuneServerProcess } from './runeServer';

/** Fixtures this suite adds on top of Playwright's own. */
interface RuneFixtures {
  /** A freshly launched, seeded `rune-server` on an OS-assigned port. */
  runeServer: RuneServerProcess;
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
});

export { expect } from '@playwright/test';
