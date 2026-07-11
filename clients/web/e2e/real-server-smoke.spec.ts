/**
 * Real-server smoke tier (ADR 0011 / issue #144): drive the true end-to-end path
 * with the **actual** `rune-server` and **two** browser clients, through the whole
 * lobby to the first `GameView` rendered on both battlefields.
 *
 * The mock tiers ({@link ./connect-to-battlefield.spec}, {@link ./lobby-flow.spec})
 * replay canned frames — fast, deterministic, the default. This tier is the M1
 * exit-criterion smoke test the ADR calls for: it launches the real server binary
 * and connects two real Chromium contexts, so it proves the built client, the
 * socket, and the real protocol implementation agree — catching mock-vs-reality
 * drift the canned tiers structurally cannot. It is deliberately coarse and few.
 *
 * The flow is exactly two players': client A connects, lands in the lobby, and
 * creates a room; client B connects and joins by the shared room id; both submit a
 * bundled starter deck and ready up; the moment the last seat readies, the real
 * server constructs the game and pushes each seat its own first `GameView`, and
 * each client's battlefield mounts and renders it (asserted via the read-only
 * `window.__RUNE_TEST__.scene` hook, so the Pixi path genuinely runs in WebGL).
 *
 * Flake posture (ADR 0011): auto-waiting and `expect.poll` only — no sleeps. The
 * server process is owned by this harness; on failure its logs are attached for
 * diagnosis.
 */
import { expect, test, type Browser, type Page } from '@playwright/test';
import { startRealServer, type RealServer } from './real-server';

let server: RealServer;

test.beforeAll(async () => {
  server = await startRealServer();
});

test.afterAll(async () => {
  await server?.close();
});

// This smoke test drives two clients and the real engine start-up; give it more
// room than the fast mock specs, while staying well inside the suite budget.
test.describe('real-server two-client smoke', () => {
  test.setTimeout(90_000);

  /** Open a fresh, isolated browser context on the app's connection screen. */
  async function openClient(browser: Browser): Promise<Page> {
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto('/');
    await expect(page.getByTestId('connection-screen')).toBeVisible();
    await page.getByTestId('server-url').fill(server.url);
    await page.getByTestId('connect-button').click();
    await expect(page.getByTestId('lobby-screen')).toBeVisible();
    return page;
  }

  /** Read the local player's rendered scene once the first GameView has landed. */
  async function localPlayerId(page: Page): Promise<string | null> {
    return page.evaluate(() => window.__RUNE_TEST__?.scene?.localPlayerId ?? null);
  }

  test('two browsers walk the full lobby to a rendered first GameView on both', async ({
    browser,
  }, testInfo) => {
    let alice: Page | undefined;
    let bob: Page | undefined;
    try {
      // Two independent players, each in their own browser context.
      alice = await openClient(browser);
      bob = await openClient(browser);

      // Alice creates a room; the room panel surfaces the shareable room id.
      await alice.getByTestId('create-room-button').click();
      await expect(alice.getByTestId('room-panel')).toBeVisible();
      const roomId = (await alice.getByTestId('room-id').textContent())?.trim() ?? '';
      expect(roomId.length).toBeGreaterThan(0);

      // Bob joins by that id — shared out-of-band, exactly as a real invite is.
      await bob.getByTestId('join-room-input').fill(roomId);
      await bob.getByTestId('join-room-button').click();
      await expect(bob.getByTestId('room-panel')).toBeVisible();

      // Both seats now show filled on both rosters (each rebuilt from one LobbyView).
      for (const page of [alice, bob]) {
        await expect(page.getByTestId('seat-0')).toContainText('Filled');
        await expect(page.getByTestId('seat-1')).toContainText('Filled');
      }

      // Both submit a bundled starter deck; the server validates it authoritatively
      // and each seat reads "decked".
      for (const page of [alice, bob]) {
        await page.getByTestId('submit-deck-button').click();
      }
      await expect(alice.getByTestId('seat-0-decked')).toBeVisible();
      await expect(alice.getByTestId('seat-1-decked')).toBeVisible();

      // Both ready up. When the last seat readies, the real server constructs the
      // game and hands each connection off to the in-game GameView contract.
      for (const page of [alice, bob]) {
        await page.getByTestId('ready-button').click();
      }

      // Each client's battlefield mounts and paints its own first GameView.
      for (const page of [alice, bob]) {
        await expect(page.locator('canvas')).toBeVisible();
        await expect.poll(() => localPlayerId(page)).not.toBeNull();
      }

      const aliceId = await localPlayerId(alice);
      const bobId = await localPlayerId(bob);

      // Each derived scene reached the browser's test hook — the real Pixi render
      // path ran (not the jsdom no-op) — with a non-empty opening hand and a local
      // battlefield band. This is the first GameView rendered on both battlefields.
      for (const page of [alice, bob]) {
        const scene = await page.evaluate(() => window.__RUNE_TEST__?.scene ?? null);
        expect(scene).not.toBeNull();
        expect(scene!.hand.length).toBeGreaterThan(0);
        expect(scene!.bands.some((band) => band.isLocal)).toBe(true);
      }

      // The two views are genuinely personalized: the real server seated the two
      // connections at distinct player identities (creator first), so their local
      // scenes disagree about who "you" are — proof this is two real seats in one
      // real game, not one client echoed twice.
      expect(aliceId).not.toBeNull();
      expect(bobId).not.toBeNull();
      expect(aliceId).not.toBe(bobId);
    } catch (error) {
      // Capture the real server's logs on any failure, per ADR 0011's requirement
      // that smoke-tier failures surface server output.
      await testInfo.attach('rune-server.log', {
        body: server?.logs() ?? '(no server logs captured)',
        contentType: 'text/plain',
      });
      throw error;
    } finally {
      await alice?.context().close();
      await bob?.context().close();
    }
  });
});
