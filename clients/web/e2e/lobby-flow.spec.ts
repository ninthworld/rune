/**
 * Lobby e2e (ADR 0011 / issues #104, #114): drive the real production client
 * through the whole never-a-dead-screen flow — address → lobby → game — in real
 * Chromium against a scripted mock backend.
 *
 * The flow is exactly a player's: open the app, connect, land in the lobby, create
 * a room, submit a bundled starter deck, ready up, and — when the scripted server
 * responds to `ready` with the fixture `GameView` (the game is constructed) —
 * assert the table renders it. Everything waits on observable state (Playwright
 * auto-waiting + `expect.poll`); there are no sleeps.
 */
import { expect, test } from '@playwright/test';
import { startLobbyMockServer, type MockServer } from './mock-server';

let server: MockServer;

test.beforeAll(async () => {
  server = await startLobbyMockServer();
});

test.afterAll(async () => {
  await server.close();
});

test('walks the address → lobby → game flow and renders the game on ready', async ({ page }) => {
  await page.goto('/');

  // Address: connect to the scripted mock backend.
  await expect(page.getByTestId('connection-screen')).toBeVisible();
  await page.getByTestId('server-url').fill(server.url);
  await page.getByTestId('connect-button').click();

  // Lobby: the first LobbyView (after the client's Hello) offers create/join.
  await expect(page.getByTestId('lobby-screen')).toBeVisible();
  await expect(page.getByTestId('create-room-button')).toBeVisible();

  // Create a room: the room panel appears with a copyable room id.
  await page.getByTestId('create-room-button').click();
  await expect(page.getByTestId('room-panel')).toBeVisible();
  await expect(page.getByTestId('room-id')).toHaveText('r:7f3');

  // Submit a bundled deck: the seat reads "decked".
  await page.getByTestId('submit-deck-button').click();
  await expect(page.getByTestId('seat-0-decked')).toBeVisible();

  // Ready up: the scripted server constructs the game and the table mounts.
  await page.getByTestId('ready-button').click();
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();

  // The game is live: the derived scene reaches the browser's test hook.
  await expect
    .poll(() => page.evaluate(() => window.__RUNE_TEST__?.scene?.localPlayerId ?? null))
    .toBe('p1');
});
