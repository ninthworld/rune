/**
 * The bounded scenario driver (ADR 0011): the part of a multi-seat browser
 * scenario that is *travel*, not evidence.
 *
 * A vertical slice has to get a real game from the opening hand to a board with
 * creatures on it before the interesting legs — targeting, combat, damage —
 * exist at all. Every one of those intervening moves still has to be a click on
 * a rendered control (there is no test-only control path), so they need a
 * driver; but they are not what any assertion is about. This module is that
 * driver, and nothing else: it takes whichever move the server is offering the
 * seat that currently holds priority and makes it, until a caller-supplied
 * predicate over the authoritative views says the scenario has arrived.
 *
 * Three properties keep it honest:
 *
 * - **It decides nothing about legality.** Every branch reads `valid_actions`
 *   and picks *which offered action to take*. It never derives what is legal,
 *   what something costs, or what it would do.
 * - **It never plays the scenario's own moves.** The driver deliberately
 *   declares *no* attackers (an empty declaration is a legal answer): the
 *   attack, its defender, and its blockers are the spec's, made explicitly
 *   through {@link ./table}'s helpers so the assertions sit on them.
 * - **It stops before it spends a milestone.** The stop predicate is evaluated
 *   before each move, so the state the spec wants to act on is never consumed by
 *   the driver.
 *
 * No sleeps: each move waits for its own effect, or for the acting seat's view
 * to move on.
 */
import { expect, type Page } from '@playwright/test';
import {
  playLandThroughUi,
  readView,
  stampOf,
  waitForAnyViewChange,
  waitForStampChange,
} from './client';
import { castSpellThroughUi, tapForManaThroughUi } from './table';
import type { HookAction, HookView } from './hook';

/**
 * How many lands the driver will tap on one of its own turns before giving up on
 * casting. Kept low on purpose: this engine does not empty mana pools between
 * steps, so a seat left holding unspent mana never looks idle to ADR 0020's
 * auto-pass again and has to click through every window for the rest of the
 * game. Tapping only as far as a cast keeps the pod's travel cheap.
 */
const MAX_TAPS_PER_TURN = 2;

/** Per-seat driver bookkeeping (how far this seat has tapped this turn). */
interface SeatState {
  page: Page;
  taps: number;
  tapTurn: number;
}

/** The declarations a turn cannot get past without an answer. */
const DECLARATIONS = ['declare_attackers', 'declare_blockers', 'order_combat_damage'];

/**
 * The single, subject-less, choice-posing action a view *forces* — the same
 * shape the client itself treats as forced (`tableView.ts#forcedDecision`), so
 * the driver answers exactly the decisions the table auto-opens: the opening
 * mulligan and the cleanup discard.
 */
function forcedDecision(view: HookView): HookAction | null {
  const offered = view.valid_actions.filter((action) => action.type !== 'concede');
  const only = offered.length === 1 ? offered[0] : undefined;
  if (only === undefined || (only.subject?.length ?? 0) > 0) return null;
  const poses = (only.requirements?.length ?? 0) > 0 || (only.prompts?.length ?? 0) > 0;
  return poses ? only : null;
}

/**
 * Answer a forced decision through its rendered surfaces: satisfy any exact-count
 * zone pick by clicking that many advertised candidates, then take the named
 * option the server says is answerable (or Confirm when the decision poses none).
 */
async function answerForcedDecision(page: Page, view: HookView, action: HookAction): Promise<void> {
  const before = stampOf(view);
  for (const prompt of action.prompts ?? []) {
    if (prompt.kind !== 'select_from_zone') continue;
    const candidates = (prompt.candidates ?? []).slice(0, prompt.count ?? 0);
    for (const id of candidates) {
      // A hand pick is made on the card itself; a pick from a zone the board
      // does not show is made in the decision sheet's list.
      const inHand = page.getByTestId(`live-hand-card-${id}`);
      const inSheet = page.getByTestId(`zone-select-${id}`);
      const target = (await inHand.count()) > 0 ? inHand : inSheet;
      await target.click();
    }
  }
  const option = (action.prompts ?? []).find((prompt) => prompt.kind === 'option');
  if (option !== undefined) {
    // Take the choice the server left unencumbered — a mulligan's *keep* owes an
    // exact bottoming (issue #451), so at the opening hand it is `keep` and after
    // a mulligan it is whichever choice `requires` nothing further.
    const choice =
      (option.options ?? []).find((entry) => (entry.requires ?? []).length === 0) ??
      (option.options ?? [])[0];
    if (choice === undefined) throw new Error('a forced option decision offered no choice');
    const button = page.getByTestId(`multiselect-option-${choice.id}`);
    await expect(button, `the forced decision should offer "${choice.label}"`).toBeEnabled();
    await button.click();
  } else {
    await page.getByTestId('multiselect-confirm').click();
  }
  await waitForStampChange(page, before);
}

/**
 * Answer a combat declaration with the empty selection — legal, and the driver's
 * whole combat policy. Opening it from the dock and confirming are both real
 * clicks on rendered controls.
 */
async function confirmEmptyDeclaration(
  page: Page,
  view: HookView,
  action: HookAction,
): Promise<void> {
  const before = stampOf(view);
  await page.getByTestId('action-bar').getByRole('button', { name: action.label }).click();
  const confirm = page.getByTestId('multiselect-confirm');
  await expect(confirm, `${action.label} should be confirmable with nothing declared`).toBeEnabled();
  await confirm.click();
  await waitForStampChange(page, before);
}

/** Click the dock's pass control for this seat and wait for the game to move on. */
async function passThroughDock(page: Page, view: HookView, action: HookAction): Promise<void> {
  const before = stampOf(view);
  await page.getByTestId('action-bar').getByRole('button', { name: action.label }).click();
  await waitForStampChange(page, before);
}

/**
 * Make one move for `seat` and report what kind it was, or `null` when the seat
 * turned out to have nothing on offer (its view moved on underneath us).
 */
async function takeOneMove(seat: SeatState, view: HookView): Promise<string | null> {
  if (view.valid_actions.length === 0) return null;

  const forced = forcedDecision(view);
  if (forced !== null) {
    await answerForcedDecision(seat.page, view, forced);
    return forced.type;
  }

  const declaration = view.valid_actions.find((action) => DECLARATIONS.includes(action.type));
  if (declaration !== undefined) {
    await confirmEmptyDeclaration(seat.page, view, declaration);
    return declaration.type;
  }

  // Spend a pool the moment anything is castable: unspent mana is what makes a
  // seat chatty for the rest of the game (see MAX_TAPS_PER_TURN).
  const casts = view.valid_actions.filter((action) => action.type === 'cast_spell');
  if (casts.length > 0) {
    const plain = casts.find((action) => (action.requirements?.length ?? 0) === 0);
    await castSpellThroughUi(seat.page, {
      known: view,
      accept: (action) => (plain === undefined ? true : action.id === plain.id),
      pickTarget: (candidates) =>
        candidates.find((id) =>
          view.battlefield.some(
            (permanent) =>
              permanent.id === id &&
              permanent.card.power !== undefined &&
              permanent.controller !== view.you,
          ),
        ) ?? candidates[0],
    });
    return 'cast_spell';
  }

  if (view.active_player === view.you && view.phase === 'precombat_main') {
    if (view.valid_actions.some((action) => action.type === 'play_land')) {
      await playLandThroughUi(seat.page, view);
      return 'play_land';
    }
    if (seat.tapTurn !== view.turn) {
      seat.tapTurn = view.turn;
      seat.taps = 0;
    }
    if (seat.taps < MAX_TAPS_PER_TURN && view.valid_actions.some((a) => a.mana_ability === true)) {
      seat.taps += 1;
      await tapForManaThroughUi(seat.page, view);
      return 'activate_ability';
    }
  }

  const pass = view.valid_actions.find((action) => action.type === 'pass_priority');
  if (pass !== undefined) {
    await passThroughDock(seat.page, view, pass);
    return 'pass_priority';
  }
  return null;
}

/** What a {@link driveUntil} run cost. */
export interface DriveReport<T> {
  /** Whatever the stop predicate returned. */
  found: T;
  /** How many moves the driver made getting there. */
  moves: number;
  /** How many of those moves were each kind. */
  byType: Record<string, number>;
}

/**
 * Drive the pod until `stop` returns a non-null value, and report the cost.
 *
 * `stop` is evaluated over every seat's published view **before** each move, so
 * the state it names is never spent by the driver itself. Bounded twice over — a
 * move budget and a wall-clock deadline — so a scenario that cannot arrive fails
 * loudly with a diagnosis instead of hanging.
 */
export async function driveUntil<T>(
  pages: Page[],
  stop: (views: Array<HookView | null>) => T | null,
  options: { moveBudget?: number; deadlineMs?: number; what?: string } = {},
): Promise<DriveReport<T>> {
  const seats: SeatState[] = pages.map((page) => ({ page, taps: 0, tapTurn: -1 }));
  const budget = options.moveBudget ?? 400;
  const deadline = Date.now() + (options.deadlineMs ?? 240_000);
  const byType: Record<string, number> = {};
  let moves = 0;

  while (Date.now() < deadline && moves < budget) {
    const views = await Promise.all(pages.map(readView));
    const found = stop(views);
    if (found !== null) return { found, moves, byType };

    const terminal = views.find((view) => view?.result !== undefined);
    if (terminal !== undefined) {
      throw new Error(`the game ended before ${options.what ?? 'the scenario'} was reached`);
    }

    let acted = false;
    for (const [index, seat] of seats.entries()) {
      if ((views[index]?.valid_actions.length ?? 0) === 0) continue;
      const kind = await takeOneMove(seat, views[index]!);
      if (kind === null) continue;
      byType[kind] = (byType[kind] ?? 0) + 1;
      moves += 1;
      acted = true;
      break;
    }
    if (!acted) await waitForAnyViewChange(pages);
  }

  const where = (await Promise.all(pages.map(readView)))
    .map((view, index) =>
      view === null
        ? `${index}: no view`
        : `${index}=${view.you} t${view.turn}/${view.phase} ` +
          `active=${view.active_player} offers=[${view.valid_actions.map((a) => a.type).join(',')}]`,
    )
    .join(' | ');
  throw new Error(
    `never reached ${options.what ?? 'the scenario state'} in ${moves} moves ` +
      `(${JSON.stringify(byType)}) before the budget ran out. Seats: ${where}`,
  );
}
