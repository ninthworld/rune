/**
 * RUNE browser **smoke canary** (issue #279, ADR 0011).
 *
 * One spec, two real browser contexts, one real seeded `rune-server`. It plays real
 * turns through the actually-rendered UI — never by sending protocol directly — far
 * enough to guard the seams the headless unit gate cannot:
 *
 *  1. The battlefield `<canvas>` stays **attached and non-blank** under React
 *     StrictMode. This is the exact regression guard for the #276 detach bug:
 *     reverting that fix (Pixi `destroy(true)`) detaches the canvas on StrictMode's
 *     remount, and this assertion goes red.
 *  2. A land is played by **clicking the rendered card hotspot → its confirm chip**
 *     (not the protocol), and the permanent then appears in the scene's local band
 *     (read from the read-only `window.__RUNE_TEST__` scene hook).
 *  3. Priority passes around one full turn boundary and the turn number advances.
 *
 * It runs against the dev server (StrictMode on) — the path that regressed — and is
 * intentionally minimal (not the full ADR 0011 matrix): fast, deterministic (pinned
 * server seed), one file, well under a minute.
 */
import { expect, test, type Page } from '@playwright/test';
import { startServer, type RunningServer } from './harness';
import { countDistinctColors } from './png';

/**
 * Pinned shuffle seed (ADR 0014). Chosen so the starting player's deterministic
 * opening hand contains at least one land, so step 2 can play one on turn 1.
 */
const SEED = 7;

/** The minimal shape of the read-only test-hook view/scene this spec reads. */
interface TestAction {
  id: string;
  type: string;
  label: string;
  subject?: string[];
}
interface TestCard {
  entityId: string;
  name: string;
}
interface TestScene {
  bands: { playerId: string; isLocal: boolean; cards: TestCard[] }[];
  hand: TestCard[];
}
interface TestView {
  you: string;
  turn: number;
  phase: string;
  valid_actions: TestAction[];
  my_hand: { id: string; name: string }[];
}

/** Read the published `GameView` from the test hook, or null before the first frame. */
function readView(page: Page): Promise<TestView | null> {
  return page.evaluate(
    () =>
      (window as unknown as { __RUNE_TEST__?: { view: TestView | null } }).__RUNE_TEST__?.view ??
      null,
  );
}

/** Read the published `TableScene` from the test hook, or null before the first frame. */
function readScene(page: Page): Promise<TestScene | null> {
  return page.evaluate(
    () =>
      (window as unknown as { __RUNE_TEST__?: { scene: TestScene | null } }).__RUNE_TEST__?.scene ??
      null,
  );
}

/** The actions the server currently offers this page (empty when it is not to move). */
async function actionsOf(page: Page): Promise<TestAction[]> {
  const view = await readView(page);
  return view?.valid_actions ?? [];
}

/** The local (this-player) battlefield band's cards from the derived scene. */
async function localBand(page: Page): Promise<TestCard[]> {
  const scene = await readScene(page);
  return scene?.bands.find((band) => band.isLocal)?.cards ?? [];
}

/** Click a global action bar button by its rendered label. */
function clickGlobal(page: Page, label: string): Promise<void> {
  return page.getByTestId('action-bar').getByRole('button', { name: label }).click();
}

/** Wait until the first GameView has been published (the table has mounted). */
function waitForTable(page: Page): Promise<unknown> {
  return page.waitForFunction(
    () => !!(window as unknown as { __RUNE_TEST__?: { view: unknown } }).__RUNE_TEST__?.view,
    undefined,
    { timeout: 20_000 },
  );
}

/**
 * Assert the battlefield board is attached and showing content — the #276 guard.
 *
 * The load-bearing half is **attachment**: with the fix (Pixi `destroy(false)`) the
 * `<canvas>` stays in the DOM across StrictMode's mount→cleanup→mount; reverting to
 * `destroy(true)` detaches it and `toBeAttached()` fails. On top of that we prove the
 * board is not a *silent* blank: real canvas pixels where the GL stack can paint, or
 * — where a headless software renderer cannot recreate the WebGL context under
 * StrictMode — the loud DOM fallback that #276 also added, listing the cards in play.
 * Either way the board shows content; only a detached/void board fails.
 */
async function assertBoardRendered(page: Page): Promise<void> {
  const canvas = page.locator('canvas');
  await expect(canvas).toBeAttached();
  const connected = await canvas.evaluate(
    (el) => el.isConnected && (el as HTMLCanvasElement).width > 0,
  );
  expect(connected).toBe(true);

  const fallback = page.getByTestId('board-render-fallback');
  if (await fallback.isVisible()) {
    // Loud "not blank" stand-in (#276): the board is visibly listed, never a void.
    await expect(fallback).toContainText(/Battlefield:/);
  } else {
    // Real render: give Pixi a frame, then prove the pixels are not a flat fill.
    await page.waitForTimeout(400);
    const shot = await canvas.screenshot();
    expect(countDistinctColors(shot)).toBeGreaterThan(1);
  }
}

/** Connect a page to the server through the connection screen and reach the lobby. */
async function enterLobby(page: Page, serverUrl: string): Promise<void> {
  await page.goto('/');
  await page.getByTestId('server-url').fill(serverUrl);
  await page.getByTestId('connect-button').click();
  await expect(page.getByTestId('lobby-screen')).toBeVisible();
}

/** Submit the default starter deck and ready up (both offered from the room panel). */
async function submitDeckAndReady(page: Page): Promise<void> {
  await page.getByTestId('submit-deck-button').click();
  await expect(page.getByTestId('ready-button')).toBeVisible();
  await page.getByTestId('ready-button').click();
}

/**
 * Keep the opening hand at the London mulligan (CR 103.5), driven purely through the
 * UI: click the subject-less "Keep or mulligan" bar action, then the "keep" option in
 * the banner's modal picker. A no-op if this page has no mulligan pending.
 */
async function keepIfMulligan(page: Page): Promise<boolean> {
  const decision = (await actionsOf(page)).find((a) => a.type === 'mulligan_decision');
  if (!decision) return false;
  await clickGlobal(page, decision.label);
  await page.getByTestId('multiselect-option-keep').click();
  return true;
}

/**
 * Perform the minimal turn-advancing move for whichever page currently has priority:
 * pass priority when offered, else confirm an empty combat declaration, else keep at a
 * mulligan. Returns true if it acted. All through the rendered UI.
 */
async function advanceOne(page: Page): Promise<boolean> {
  const actions = await actionsOf(page);
  if (actions.length === 0) return false;

  const pass = actions.find((a) => a.type === 'pass_priority');
  if (pass) {
    await clickGlobal(page, pass.label).catch(() => {});
    return true;
  }
  const declare = actions.find(
    (a) => a.type === 'declare_attackers' || a.type === 'declare_blockers',
  );
  if (declare) {
    await clickGlobal(page, declare.label).catch(() => {});
    // An empty declaration is legal; confirm it.
    await page
      .getByTestId('multiselect-confirm')
      .click({ timeout: 5000 })
      .catch(() => {});
    return true;
  }
  return keepIfMulligan(page);
}

test('smoke: two clients play real turns, canvas renders, a land hits the battlefield', async ({
  browser,
}) => {
  let server: RunningServer | undefined;
  const contexts = [await browser.newContext(), await browser.newContext()];
  try {
    server = await startServer(SEED);
    const [alice, bob] = [await contexts[0].newPage(), await contexts[1].newPage()];

    // --- Lobby: create / join / deck / ready ---
    await enterLobby(alice, server.url);
    await alice.getByTestId('create-room-button').click();
    await expect(alice.getByTestId('room-id')).toBeVisible();
    const roomId = (await alice.getByTestId('room-id').textContent())?.trim() ?? '';
    expect(roomId.length).toBeGreaterThan(0);

    await enterLobby(bob, server.url);
    // Join straight from the room directory (#280) — the real zero-copy-paste flow:
    // Alice's room appears in Bob's lobby list and Bob clicks its Join button.
    await bob.getByTestId(`join-directory-${roomId}`).click();

    await submitDeckAndReady(alice);
    await submitDeckAndReady(bob);

    // Both hand-offs land on the table: wait for the first GameView to render.
    await waitForTable(alice);
    await waitForTable(bob);

    // --- Assertion 1: canvas attached AND showing content (the #276 StrictMode guard) ---
    await assertBoardRendered(alice);
    await assertBoardRendered(bob);

    // --- Keep both opening hands, advance to the main phase, find the active player ---
    // Keep the London mulligan on each seat, then pass priority through untap/upkeep
    // until the starting player is offered `play_land` for a land in its opening hand.
    // Everything here goes through the rendered UI (never protocol directly).
    let activePage: Page | undefined;
    let landAction: TestAction | undefined;
    await expect
      .poll(
        async () => {
          for (const page of [alice, bob]) {
            const land = (await actionsOf(page)).find((a) => a.type === 'play_land');
            if (land) {
              activePage = page;
              landAction = land;
              return true;
            }
          }
          // No land drop offered yet: advance whichever seat holds priority (keep a
          // mulligan, else pass, else confirm an empty combat step). Never concede.
          for (const page of [alice, bob]) {
            if (await advanceOne(page)) break;
          }
          return false;
        },
        { timeout: 30_000 },
      )
      .toBeTruthy();
    if (!activePage || !landAction) throw new Error('no play_land was offered on turn 1');

    const landId = landAction.subject?.[0] ?? '';
    const landName = landAction.label.replace(/^Play\s+/, '');
    const before = await localBand(activePage);
    expect(before.some((c) => c.name === landName)).toBe(false);

    // Select the card (its on-card hotspot), then confirm via its action chip.
    await activePage.getByTestId(`entity-${landId}`).click();
    await activePage
      .getByTestId(`entity-actions-${landId}`)
      .getByRole('button', { name: landAction.label })
      .click();

    // The permanent now appears in the local band of the derived scene.
    await expect
      .poll(async () => (await localBand(activePage!)).some((c) => c.name === landName), {
        timeout: 10_000,
      })
      .toBe(true);
    expect((await localBand(activePage)).length).toBe(before.length + 1);

    // --- Assertion 3: pass priority around one turn boundary; the turn advances ---
    const startTurn = (await readView(activePage))?.turn ?? 0;
    expect(startTurn).toBeGreaterThan(0);

    const deadline = Date.now() + 40_000;
    let advanced = false;
    while (Date.now() < deadline) {
      for (const page of [alice, bob]) {
        const view = await readView(page);
        if (view && view.turn > startTurn) {
          advanced = true;
          break;
        }
      }
      if (advanced) break;
      let acted = false;
      for (const page of [alice, bob]) {
        if (await advanceOne(page)) {
          acted = true;
          break;
        }
      }
      await alice.waitForTimeout(acted ? 120 : 250);
    }
    expect(advanced).toBe(true);
  } finally {
    await Promise.all(contexts.map((c) => c.close()));
    if (server) await server.close();
  }
});
