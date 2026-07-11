/**
 * Smoke test (ADR 0011 / issue #104): drive the real production client in real
 * Chromium from the connection screen to a rendered battlefield.
 *
 * The flow is exactly a player's: open the app, type the address of a mock WS
 * backend into the connection screen, connect, and — once the fixture `GameView`
 * lands — assert the battlefield renders it. The canvas assertion uses ADR 0011's
 * scene-hook strategy (`window.__RUNE_TEST__.scene`), so the Pixi path is genuinely
 * exercised in a WebGL context, not the jsdom no-op the unit suite falls back to.
 *
 * Everything waits on observable state (Playwright auto-waiting + `expect.poll`);
 * there are no sleeps, so the test is deterministic and fast.
 */
import { expect, test } from '@playwright/test';
import { SAMPLE_GAME_VIEW_JSON } from '../src/game-view.fixture';
import { startMockServer, type MockServer } from './mock-server';

let server: MockServer;

test.beforeAll(async () => {
  server = await startMockServer(SAMPLE_GAME_VIEW_JSON);
});

test.afterAll(async () => {
  await server.close();
});

test('connects to a mock server and renders the fixture GameView on the battlefield', async ({
  page,
}) => {
  await page.goto('/');

  // The connection screen is the only pre-game UI: enter the mock backend's
  // address and connect (the first production caller of store.connect()).
  await expect(page.getByTestId('connection-screen')).toBeVisible();
  await page.getByTestId('server-url').fill(server.url);
  await page.getByTestId('connect-button').click();

  // Once the socket is open and the first frame lands, the table mounts and the
  // Pixi battlefield canvas appears.
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();

  // Wait for the derived scene to reach the browser's test hook, then assert on
  // the structured facts the fixture describes — this is what the canvas draws.
  await expect
    .poll(() => page.evaluate(() => window.__RUNE_TEST__?.scene?.localPlayerId ?? null))
    .toBe('p1');

  const scene = await page.evaluate(() => window.__RUNE_TEST__?.scene ?? null);
  expect(scene).not.toBeNull();

  // The local player's battlefield band holds the fixture's Grizzly Bears, tapped,
  // with its two +1/+1 counters — rendered verbatim from the server's values.
  const localBand = scene!.bands.find((band) => band.isLocal);
  expect(localBand).toBeDefined();
  const bears = localBand!.cards.find((card) => card.name === 'Grizzly Bears');
  expect(bears).toBeDefined();
  expect(bears!.zone).toBe('battlefield');
  expect(bears!.tier).toBe('field');
  expect(bears!.data.tapped).toBe(true);
  expect(bears!.data.counters).toEqual([{ kind: '+1/+1', count: 2 }]);

  // The hand holds Llanowar Elves.
  expect(scene!.hand.map((card) => card.name)).toContain('Llanowar Elves');

  // Prove the Pixi path actually ran (not the headless no-op): the real canvas was
  // resized by the renderer to the scene's logical size × the device pixel ratio.
  const pixiSizedCanvas = await page.evaluate(() => {
    const el = document.querySelector('canvas');
    const width = window.__RUNE_TEST__?.scene?.width;
    if (!el || width === undefined) return false;
    return el.width === Math.round(width * (window.devicePixelRatio || 1));
  });
  expect(pixiSizedCanvas).toBe(true);
});
