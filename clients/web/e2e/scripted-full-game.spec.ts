/**
 * Scripted full-game tier (ADR 0011 / issue #145): the M2 exit criterion. Two real
 * Chromium contexts play a COMPLETE game against the real `rune-server` — from the
 * lobby, through mulligan keeps, land drops, a creature cast, and combat, until one
 * seat's life reaches zero — and both render the game-over screen with the correct
 * winner and reason (`LifeZero`), asserted in the DOM.
 *
 * Determinism (ADR 0014): the server is pinned to a fixed shuffle {@link SEED} and
 * both seats submit the same bundled starter deck, so the whole game replays
 * identically every run. Both seats choose only among the server-advertised
 * `valid_actions`/`requirements` (zero legality in the test) and submit by clicking
 * the real UI — see {@link ./scripted-game}. One seat is the aggressor (develops and
 * attacks); the other is passive (land drops + passes), so the aggressor swings at
 * an effectively empty board until the passive seat dies — the minimal script that
 * still exercises untap/draw/combat/cleanup and the lethal result.
 *
 * Flake posture (ADR 0011): auto-waiting and `expect.poll` only, no sleeps. The
 * server process is owned by this harness; on failure its logs and a Playwright
 * trace are attached for diagnosis.
 */
import { expect, test, type Browser, type Page } from '@playwright/test';
import { startRealServer, type RealServer } from './real-server';
import { playScriptedGame } from './scripted-game';

/** The pinned engine shuffle seed — fixed decks + fixed seed ⇒ a reproducible game. */
const SEED = 0x00c0ffee;

/**
 * A low pinned starting life so the aggressor's attackers reach lethal in a few
 * combat turns. A full 20-life game is a legal MTG game but a long one — hundreds of
 * priority passes, each a browser round trip — so it would not finish inside the CI
 * budget; a short game still exercises untap/draw/combat/cleanup and the lethal
 * `LifeZero` result, which is the point (issue #145).
 */
const STARTING_LIFE = 1;

let server: RealServer;

// Keep a Playwright trace when this heavier tier fails, for CI diagnosis. Bound
// each UI interaction so a mis-targeted click fails fast (with its selector) rather
// than hanging until the whole-test timeout.
test.use({ trace: 'retain-on-failure', actionTimeout: 20_000 });

test.beforeAll(async () => {
  server = await startRealServer({ seed: SEED, startingLife: STARTING_LIFE });
});

test.afterAll(async () => {
  await server?.close();
});

test.describe('real-server scripted full game', () => {
  // A whole game across two browsers and the real engine: give it room, while
  // staying inside the suite's timeout posture (the happy path finishes far sooner).
  test.setTimeout(120_000);

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

  test('two browsers play a full game to a rendered victory screen (LifeZero)', async ({
    browser,
  }, testInfo) => {
    let aggressor: Page | undefined;
    let passive: Page | undefined;
    try {
      // Two independent players. The creator (aggressor) develops and attacks; the
      // joiner (passive) only makes land drops and passes.
      aggressor = await openClient(browser);
      passive = await openClient(browser);

      // Creator opens a room and shares the id out-of-band, exactly like an invite.
      await aggressor.getByTestId('create-room-button').click();
      await expect(aggressor.getByTestId('room-panel')).toBeVisible();
      const roomId = (await aggressor.getByTestId('room-id').textContent())?.trim() ?? '';
      expect(roomId.length).toBeGreaterThan(0);

      await passive.getByTestId('join-room-input').fill(roomId);
      await passive.getByTestId('join-room-button').click();
      await expect(passive.getByTestId('room-panel')).toBeVisible();

      // Both seats filled on both rosters.
      for (const page of [aggressor, passive]) {
        await expect(page.getByTestId('seat-0')).toContainText('Filled');
        await expect(page.getByTestId('seat-1')).toContainText('Filled');
      }

      // Both submit the same bundled starter deck and ready up; the last ready
      // constructs the game and hands each seat its first GameView.
      for (const page of [aggressor, passive]) {
        await page.getByTestId('submit-deck-button').click();
      }
      await expect(aggressor.getByTestId('seat-0-decked')).toBeVisible();
      await expect(aggressor.getByTestId('seat-1-decked')).toBeVisible();
      for (const page of [aggressor, passive]) {
        await page.getByTestId('ready-button').click();
      }

      // Each client's battlefield mounts and paints its first frame.
      for (const page of [aggressor, passive]) {
        await expect(page.locator('canvas')).toBeVisible();
        await expect
          .poll(() => page.evaluate(() => window.__RUNE_TEST__?.view ?? null))
          .not.toBeNull();
      }

      // The two seats' identities (each seat's own `you`), captured before play.
      const aggressorId = await aggressor.evaluate(() => window.__RUNE_TEST__?.view?.you ?? '');
      const passiveId = await passive.evaluate(() => window.__RUNE_TEST__?.view?.you ?? '');
      expect(aggressorId.length).toBeGreaterThan(0);
      expect(passiveId).not.toBe(aggressorId);

      // Play the whole game from a single coordinator, each seat choosing only
      // advertised actions, until both views carry a terminal result.
      const [aggressorResult, passiveResult] = await playScriptedGame([
        { page: aggressor, role: 'aggressor' },
        { page: passive, role: 'passive' },
      ]);

      // The engine decided a game over by life reaching zero, with the aggressor the
      // winner and the passive seat the loser — and the two seats' views agree.
      expect(aggressorResult.reason).toBe('life_zero');
      expect(aggressorResult.winner).toBe(aggressorId);
      expect(aggressorResult.losers).toContain(passiveId);
      expect(passiveResult).toEqual(aggressorResult);

      // Both clients render the game-over overlay (React DOM, #141), phrased from
      // each seat's own vantage but agreeing on winner and reason.
      for (const page of [aggressor, passive]) {
        await expect(page.getByTestId('game-over-overlay')).toBeVisible();
        await expect(page.getByTestId('game-over-winner')).toHaveText(`${aggressorId} wins the game.`);
        await expect(page.getByTestId('game-over-reason')).toContainText('life total reached zero');
      }
      await expect(aggressor.getByTestId('game-over-headline')).toHaveText('Victory');
      await expect(passive.getByTestId('game-over-headline')).toHaveText('Defeat');
    } catch (error) {
      await testInfo.attach('rune-server.log', {
        body: server?.logs() ?? '(no server logs captured)',
        contentType: 'text/plain',
      });
      throw error;
    } finally {
      await aggressor?.context().close();
      await passive?.context().close();
    }
  });
});
