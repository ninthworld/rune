/**
 * Page helpers for the browser smoke suite (ADR 0011, issue #279).
 *
 * Two rules shape every helper here, and they are the reason the suite is worth
 * having at all:
 *
 * 1. **Every move is made by clicking rendered UI.** Nothing is submitted
 *    through the test hook, and the client is given no test-only control path.
 *    If a control is not on screen and clickable, the suite cannot use it —
 *    which is exactly the property a browser canary must have.
 * 2. **Legality is never computed here.** The suite reads the server's
 *    `valid_actions` off the read-only hook purely to *decide which rendered
 *    control to click*. It never derives what is legal, what a cost is, or what
 *    an action would do.
 *
 * There are no sleeps. Every wait is a wait on a condition: a locator becoming
 * visible, or a predicate over the published view.
 */
import { expect, type Locator, type Page } from '@playwright/test';
import type { HookAction, HookPlane, HookView } from './hook';

/** Read the latest published `GameView`, or `null` before the first frame. */
export async function readView(page: Page): Promise<HookView | null> {
  return page.evaluate(() => window.__RUNE_TEST__?.view ?? null);
}

/** Read the latest published scene plane, or `null` when none is staged. */
export async function readPlane(page: Page): Promise<HookPlane | null> {
  return page.evaluate(() => window.__RUNE_TEST__?.plane ?? null);
}

/** The server-owned turn number, or `0` before the first frame. */
export async function turnOf(page: Page): Promise<number> {
  return page.evaluate(() => window.__RUNE_TEST__?.view?.turn ?? 0);
}

/**
 * Wait until the server offers an action of `type` to this seat, and return it.
 * A wait on a condition — the published view — never on a timer.
 */
export async function waitForOfferedAction(
  page: Page,
  type: string,
  timeout = 60_000,
): Promise<HookAction> {
  await page.waitForFunction(
    (kind) => window.__RUNE_TEST__?.view?.valid_actions.some((a) => a.type === kind) === true,
    type,
    { timeout },
  );
  const action = await page.evaluate(
    (kind) => window.__RUNE_TEST__?.view?.valid_actions.find((a) => a.type === kind) ?? null,
    type,
  );
  if (action === null) throw new Error(`no ${type} action offered after the wait resolved`);
  return action;
}

/**
 * Connect through the front door, typing the address a player would type.
 * `origin` overrides the config's `baseURL` — the four-player slice runs against
 * the preview build rather than the canary's dev server (`support/targets.ts`).
 */
export async function connect(page: Page, serverUrl: string, origin?: string): Promise<void> {
  await page.goto(origin === undefined ? '/' : `${origin}/`);
  await expect(page.getByTestId('connection-screen')).toBeVisible();
  // The address lives behind the "Server settings" disclosure — open it the way
  // a player pointing at another server would.
  await page.getByTestId('server-settings').locator('summary').click();
  await page.getByTestId('server-url').fill(serverUrl);
  await page.getByTestId('connect-button').click();
  await expect(page.getByTestId('lobby-screen')).toBeVisible();
}

/**
 * Create a room from the lobby's Start-a-game card the way a host does — pick
 * the game-type tile, pick the seat count, press Create — and return the room id
 * shown in its header.
 *
 * `setup` is the opaque `game_setup` id the tile carries; the suite never
 * interprets it, and the server is the only thing that validates it against a
 * seat count (`crates/rune-server/src/format.rs`).
 */
export async function createRoom(page: Page, setup: string, seats: number): Promise<string> {
  await page.getByTestId(`game-setup-${setup}`).click();
  await page.getByTestId(`seat-count-${seats}`).click();
  await page.getByTestId('create-room-button').click();
  await expect(page.getByTestId('room-panel')).toBeVisible();
  const roomId = (await page.getByTestId('room-id').innerText()).trim();
  expect(roomId, 'the room header should show a joinable room id').not.toBe('');
  return roomId;
}

/** Create a two-seat duel room and return the room id shown in its header. */
export async function createDuelRoom(page: Page): Promise<string> {
  return createRoom(page, '1v1', 2);
}

/** Join an existing room by pasting its id, as an invited player would. */
export async function joinRoom(page: Page, roomId: string): Promise<void> {
  await page.getByTestId('join-room-input').fill(roomId);
  await page.getByTestId('join-room-button').click();
  await expect(page.getByTestId('room-panel')).toBeVisible();
}

/** This connection's own roster row (the one carrying the "You" tag). */
function ownSeatRow(page: Page): Locator {
  return page.getByTestId('seat-list').locator('li').filter({ hasText: 'You' });
}

/**
 * Submit a starter deck and ready up. Waits for the server to acknowledge the
 * deck (the roster's own row flips to "Deck submitted") before readying, so the
 * two commands can never race.
 *
 * `deckId` names one of the bundled starter tiles; omitted, whichever tile the
 * room pre-selects is submitted. Naming it keeps a scenario from silently
 * depending on the order of `starter-decks.json`.
 */
export async function submitDeckAndReady(page: Page, deckId?: string): Promise<void> {
  if (deckId !== undefined) await page.getByTestId(`deck-tile-${deckId}`).click();
  await page.getByTestId('submit-deck-button').click();
  await expect(ownSeatRow(page)).toContainText('Deck submitted');
  await page.getByTestId('ready-button').click();
  // Readying either leaves this seat waiting on the others, or — for the last
  // seat — trips the ready gate and the table replaces the lobby outright.
  await expect(
    page.locator('[data-testid="ready-waiting"], [data-testid="live-match-table"]').first(),
  ).toBeVisible();
}

/** Wait for the first `GameView` to mount the match table. */
export async function waitForTable(page: Page): Promise<void> {
  await expect(page.getByTestId('live-match-table')).toBeVisible({ timeout: 60_000 });
}

/** Answer the pre-game mulligan by keeping the opening hand. */
export async function keepOpeningHand(page: Page): Promise<void> {
  const keep = page.getByTestId('multiselect-option-keep');
  await expect(keep).toBeVisible();
  await expect(keep).toBeEnabled();
  await keep.click();
  await expect(page.getByTestId('decision-sheet')).toBeHidden();
}

/**
 * Click a point on `target` that a player could actually hit — the hand fans its
 * cards, so each card's centre may sit under its neighbour. Scans the element's
 * own box for a spot that hit-tests to it and clicks there with a real mouse.
 *
 * Deliberately not `{ force: true }`: forcing would also "pass" on a card that
 * is completely buried, which is a genuine UI bug this suite should report.
 */
export async function clickReachablePoint(page: Page, target: Locator): Promise<void> {
  await target.waitFor({ state: 'visible' });
  const point = await target.evaluate((el) => {
    const rect = el.getBoundingClientRect();
    for (let fy = 0.2; fy <= 0.85; fy += 0.15) {
      for (let fx = 0.05; fx <= 0.95; fx += 0.05) {
        const x = rect.x + rect.width * fx;
        const y = rect.y + rect.height * fy;
        const hit = document.elementFromPoint(x, y);
        if (hit !== null && el.contains(hit)) return { x, y };
      }
    }
    return null;
  });
  if (point === null) {
    throw new Error('no part of the target is reachable by a click — it is fully occluded');
  }
  await page.mouse.click(point.x, point.y);
}

/** A land that was played through the UI, on both sides of the zone change. */
export interface PlayedLand {
  /** The card's entity id while it was in hand. */
  handCardId: string;
  /** Its face name — the identity that survives the zone change. */
  name: string;
  /** The permanent's entity id on the battlefield (a *different* object). */
  permanentId: string;
}

/**
 * Play a land through the rendered affordances: click the hand card (which
 * selects it) and then the action chip the dock echoes for that selection.
 *
 * Resolves once the authoritative view shows the new permanent under this
 * seat's control, and reports both ids — the battlefield object is a new
 * entity, so the hand id must not be carried across the move.
 */
export async function playLandThroughUi(page: Page, known?: HookView): Promise<PlayedLand> {
  const action =
    known?.valid_actions.find((candidate) => candidate.type === 'play_land') ??
    (await waitForOfferedAction(page, 'play_land'));
  const handCardId = action.subject?.[0];
  if (handCardId === undefined) throw new Error('play_land was offered without a subject card');
  const before =
    known === undefined
      ? await page.evaluate((id) => {
          const view = window.__RUNE_TEST__?.view;
          return {
            name: view?.my_hand.find((card) => card.id === id)?.name ?? null,
            battlefield: view?.battlefield.map((permanent) => permanent.id) ?? [],
          };
        }, handCardId)
      : {
          name: known.my_hand.find((card) => card.id === handCardId)?.name ?? null,
          battlefield: known.battlefield.map((permanent) => permanent.id),
        };
  if (before.name === null) throw new Error(`the play_land subject ${handCardId} is not in hand`);

  await clickReachablePoint(page, page.getByTestId(`live-hand-card-${handCardId}`));
  const echo = page.getByTestId('selection-echo');
  await expect(echo, 'selecting a playable card should echo its actions in the dock').toBeVisible();
  await echo.getByRole('button', { name: action.label }).click();

  const handle = await page
    .waitForFunction(
      (seen) => {
        const view = window.__RUNE_TEST__?.view;
        const played = view?.battlefield.find(
          (permanent) =>
            permanent.card.name === seen.name &&
            permanent.controller === view.you &&
            !seen.battlefield.includes(permanent.id),
        );
        return played?.id ?? null;
      },
      before,
      { timeout: 20_000 },
    )
    .catch(() => {
      throw new Error(
        `clicking the dock chip for "${action.label}" never put ${before.name} on the battlefield`,
      );
    });
  const permanentId = await handle.jsonValue();
  if (permanentId === null) throw new Error('the played land never reached the battlefield');
  return { handCardId, name: before.name, permanentId };
}

/**
 * Click the dock's pass control when the server offers it to this seat.
 * Returns whether a pass was actually submitted.
 */
export async function passPriorityIfOffered(page: Page): Promise<boolean> {
  const label = await page.evaluate(
    () =>
      window.__RUNE_TEST__?.view?.valid_actions.find((a) => a.type === 'pass_priority')?.label ??
      null,
  );
  if (label === null) return false;
  // The pass button carries its keyboard hint inside its accessible name, so
  // match on the server's label as a substring rather than exactly.
  const button = page.getByTestId('action-bar').getByRole('button', { name: label });
  try {
    await button.click({ timeout: 5_000 });
    return true;
  } catch {
    // The view moved on between the read and the click; the caller re-loops.
    return false;
  }
}

/**
 * Confirm an open declaration (e.g. "declare attackers" with nothing to
 * declare) when the dock offers it. Returns whether a confirm was submitted.
 */
export async function confirmDeclarationIfOffered(page: Page): Promise<boolean> {
  const confirm = page.getByTestId('multiselect-confirm');
  try {
    if (!(await confirm.isEnabled({ timeout: 1_000 }))) return false;
    await confirm.click({ timeout: 5_000 });
    return true;
  } catch {
    return false;
  }
}

/**
 * The combat declarations a turn cannot get past without an answer. Opening one
 * from the dock is still just clicking a control the server offered; the suite
 * chooses nothing (it confirms the empty declaration on the next pass).
 */
const DECLARATION_TYPES = ['declare_attackers', 'declare_blockers', 'order_combat_damage'];

/** Open a combat declaration from the dock when the server offers one. */
export async function openDeclarationIfOffered(page: Page): Promise<boolean> {
  const label = await page.evaluate(
    (types) =>
      window.__RUNE_TEST__?.view?.valid_actions.find((a) => types.includes(a.type))?.label ?? null,
    DECLARATION_TYPES,
  );
  if (label === null) return false;
  try {
    await page.getByTestId('action-bar').getByRole('button', { name: label }).click({
      timeout: 5_000,
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * A cheap identity for "the view has moved on", used to wait on server progress.
 * Pure, so a caller that already holds a view spends no round trip on it.
 */
export function stampOf(view: HookView | null): string {
  if (view === null) return 'none';
  return `${view.turn}|${view.phase}|${view.valid_actions.map((a) => a.id).join(',')}`;
}

/** {@link stampOf} for the view a page has published right now. */
export async function viewStamp(page: Page): Promise<string> {
  return page.evaluate(() => {
    const view = window.__RUNE_TEST__?.view;
    if (!view) return 'none';
    return `${view.turn}|${view.phase}|${view.valid_actions.map((a) => a.id).join(',')}`;
  });
}

/**
 * Wait until this seat's published view moves past `previous` (a {@link viewStamp}).
 * The generic "my move landed" wait, for a gesture with no more specific effect
 * to watch for. A condition wait, never a sleep.
 */
export async function waitForStampChange(
  page: Page,
  previous: string,
  timeout = 20_000,
): Promise<void> {
  await page.waitForFunction(
    (before) => {
      const view = window.__RUNE_TEST__?.view;
      const stamp = view
        ? `${view.turn}|${view.phase}|${view.valid_actions.map((a) => a.id).join(',')}`
        : 'none';
      return stamp !== before;
    },
    previous,
    { timeout },
  );
}

/** Wait until any seat's published view changes; a condition wait, not a sleep. */
export async function waitForAnyViewChange(pages: Page[], timeout = 10_000): Promise<void> {
  const stamps = await Promise.all(pages.map(viewStamp));
  await Promise.race(
    pages.map((page, index) =>
      page
        .waitForFunction(
          (previous) => {
            const view = window.__RUNE_TEST__?.view;
            const stamp = view
              ? `${view.turn}|${view.phase}|${view.valid_actions.map((a) => a.id).join(',')}`
              : 'none';
            return stamp !== previous;
          },
          stamps[index],
          { timeout },
        )
        .catch(() => undefined),
    ),
  );
}

/**
 * Wait until the staged scene puts `entityId` in the receiver's own band — the
 * "did the board actually change for this player" assertion. Reports what *is*
 * staged when it does not, so a blank plane reads as a blank plane rather than
 * as an anonymous timeout.
 */
export async function waitForReceiverBandRender(
  page: Page,
  entityId: string,
  timeout = 20_000,
): Promise<void> {
  try {
    await page.waitForFunction(
      (id) =>
        window.__RUNE_TEST__?.plane?.receiver?.renders.some((render) =>
          render.memberIds.includes(id),
        ) === true,
      entityId,
      { timeout },
    );
  } catch {
    const plane = await readPlane(page);
    const staged = plane?.receiver?.renders.flatMap((render) => render.memberIds) ?? [];
    throw new Error(
      `${entityId} never staged in the receiver band (staged: ${staged.join(', ') || 'nothing'})`,
    );
  }
}

/**
 * Wait until the server offers an action of `type` to *some* seat, and return
 * that seat's page. Which seat acts is the server's call, never the suite's.
 */
export async function seatOfferedAction(
  pages: Page[],
  type: string,
  deadlineMs = 60_000,
): Promise<Page> {
  const deadline = Date.now() + deadlineMs;
  while (Date.now() < deadline) {
    for (const page of pages) {
      const offered = await page.evaluate(
        (kind) => window.__RUNE_TEST__?.view?.valid_actions.some((a) => a.type === kind) === true,
        type,
      );
      if (offered) return page;
    }
    await waitForAnyViewChange(pages);
  }
  throw new Error(`no seat was offered a ${type} action before the deadline`);
}

/**
 * Submit one priority pass through the dock's own pass control, on whichever
 * seat the server currently offers it to, and wait for the game to move on.
 *
 * Kept as its own step because ADR 0020 priority automation settles idle
 * windows server-side: a turn can cross its boundary without any seat ever
 * being *offered* a pass. This asserts the rendered control itself works.
 */
export async function passPriorityThroughDock(pages: Page[]): Promise<void> {
  const page = await seatOfferedAction(pages, 'pass_priority');
  const before = await viewStamp(page);
  const clicked = await passPriorityIfOffered(page);
  expect(clicked, "the dock should render the server's pass action as a clickable control").toBe(
    true,
  );
  await waitForStampChange(page, before);
}

/** What crossing a turn boundary cost, in real clicks. */
export interface TurnBoundary {
  /** The turn number the server reports afterwards. */
  turn: number;
  /** How many passes were submitted through the dock's pass control. */
  passes: number;
  /** Every rendered dock control clicked to get here (passes + declarations). */
  clicks: number;
}

/**
 * Pass priority around the table until the server advances the turn number past
 * `from`. Every pass is a real dock click; the loop is bounded by a deadline so
 * a stuck game fails loudly instead of hanging.
 */
export async function advancePastTurnBoundary(
  pages: Page[],
  from: number,
  deadlineMs = 90_000,
): Promise<TurnBoundary> {
  const deadline = Date.now() + deadlineMs;
  let passes = 0;
  let clicks = 0;
  while (Date.now() < deadline) {
    const turns = await Promise.all(pages.map(turnOf));
    const highest = Math.max(...turns);
    if (highest > from) return { turn: highest, passes, clicks };

    let acted = false;
    for (const page of pages) {
      if (await confirmDeclarationIfOffered(page)) {
        acted = true;
        clicks += 1;
      } else if (await passPriorityIfOffered(page)) {
        acted = true;
        clicks += 1;
        passes += 1;
      } else if (await openDeclarationIfOffered(page)) {
        acted = true;
        clicks += 1;
      }
    }
    if (!acted) await waitForAnyViewChange(pages);
  }
  throw new Error(
    `the turn never advanced past ${from} (${clicks} dock clicks submitted before the deadline)`,
  );
}
