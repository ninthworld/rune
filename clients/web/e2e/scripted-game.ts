/**
 * Scripted-game driver for the real-server e2e tier (issue #145).
 *
 * This drives a *real* browser client through a *complete* game by choosing among
 * the server-advertised `valid_actions`/`requirements` ONLY — it computes no
 * legality of its own, exactly like the CLI rule-based agent (`crates/rune-cli`).
 * It reads the client's current {@link GameView} from the read-only test hook
 * (`window.__RUNE_TEST__.view`, ADR 0011) to decide *which* offered action to take,
 * then **submits by clicking the real UI** (the action bar, the on-entity action
 * chips, and the multi-select toggle/confirm controls) — so the hook stays a
 * read-only observation surface, never a control channel, and the test genuinely
 * exercises the production interaction path (#143 multi-select, #141 game-over).
 *
 * Two roles keep the game minimal and fast (the issue's design): the **aggressor**
 * develops lands and creatures and attacks every turn, while the **passive** seat
 * only makes land drops and passes — never casting a creature, so its board stays
 * effectively empty and the aggressor's attackers connect until the passive seat's
 * life hits zero. Both seats choose only advertised ids; declining to attack/cast
 * is itself a choice among the offered actions, not legality the test invented.
 */
import { type Page } from '@playwright/test';
import type {
  CardView,
  GameResult,
  GameView,
  Permanent,
  Prompt,
  TargetRequirement,
  ValidAction,
} from '../src/protocol';

/** Which policy a seat plays. */
export type Role = 'aggressor' | 'passive';

/** The special action `type` the server uses for passing priority. */
const PASS_PRIORITY = 'pass_priority';

/** A snapshot of the client's current view: the parsed value plus its JSON form
 * (used for O(1) change detection), read in a single round trip. */
interface ViewSnapshot {
  view: GameView | null;
  json: string;
}

/** Read the client's current personalized view (and its JSON) from the read-only
 * hook in one `page.evaluate`, to minimise Node↔browser round trips per step. */
async function readState(page: Page): Promise<ViewSnapshot> {
  return page.evaluate(() => {
    const view = window.__RUNE_TEST__?.view ?? null;
    return { view, json: JSON.stringify(view) };
  });
}

// --- Pure policy (a TypeScript port of the CLI rule-based agent) ------------

/** Converted mana value of a cost string like `"{2}{G}"` (→ 3); `{N}` adds N, any
 * other symbol adds 1. A purely lexical count — no color logic. */
function manaValueOf(cost: string): number {
  return cost
    .split('}')
    .map((segment) => segment.replace('{', ''))
    .filter((symbol) => symbol.length > 0)
    .reduce((sum, symbol) => sum + (Number.isNaN(Number(symbol)) ? 1 : Number(symbol)), 0);
}

/** The hand card the viewer holds with entity id `id`, if any. */
function cardInHand(view: GameView, id: string): CardView | undefined {
  return view.my_hand.find((card) => card.id === id);
}

/** The permanent in play with entity id `id`, if any. */
function permInPlay(view: GameView, id: string): Permanent | undefined {
  return view.battlefield.find((perm) => perm.id === id);
}

/** Whether a card is a creature / land, by its server-computed type line. */
function isCreature(card: CardView): boolean {
  return card.type_line.toLowerCase().includes('creature');
}
function isLand(card: CardView): boolean {
  return card.type_line.toLowerCase().includes('land');
}

/** A permanent's numeric power, or 0 when absent/non-numeric. */
function powerOf(perm: Permanent): number {
  const value = Number(perm.card.power);
  return Number.isFinite(value) ? value : 0;
}

/** The greatest hand mana value an action's subject names (0 if none). */
function actionManaValue(view: GameView, action: ValidAction): number {
  let best = 0;
  for (const id of action.subject ?? []) {
    const card = cardInHand(view, id);
    if (card?.mana_cost) best = Math.max(best, manaValueOf(card.mana_cost));
  }
  return best;
}

/** The first offered action of `type`, or undefined. */
function firstOfType(actions: ValidAction[], type: string): ValidAction | undefined {
  return actions.find((action) => action.type === type);
}

/** Whether the hand holds any non-land card worth building mana for. */
function wantsToCast(view: GameView): boolean {
  return view.my_hand.some((card) => !isLand(card));
}

/** Whether an action is a land's (target-less) mana ability. */
function isManaSource(view: GameView, action: ValidAction): boolean {
  return (
    action.type === 'activate_ability' &&
    (action.requirements?.length ?? 0) === 0 &&
    (action.subject ?? []).some((id) => {
      const perm = permInPlay(view, id);
      return perm !== undefined && isLand(perm.card);
    })
  );
}

/** The offered `cast_spell` with the greatest mana value satisfying `pred`. */
function highestCast(
  view: GameView,
  actions: ValidAction[],
  pred: (view: GameView, action: ValidAction) => boolean,
): ValidAction | undefined {
  let best: ValidAction | undefined;
  for (const action of actions) {
    if (action.type !== 'cast_spell' || !pred(view, action)) continue;
    if (best === undefined || actionManaValue(view, action) > actionManaValue(view, best)) {
      best = action;
    }
  }
  return best;
}

/** Whether a cast action's subject names a creature card in hand. */
function subjectIsCreature(view: GameView, action: ValidAction): boolean {
  return (action.subject ?? []).some((id) => {
    const card = cardInHand(view, id);
    return card !== undefined && isCreature(card);
  });
}

/**
 * The action a seat takes for `view`, or `null` when nothing is offered. A pure
 * function of the view (mirrors the CLI agent's `choose_action`). The passive role
 * never develops creatures or attacks — it makes land drops and passes — so the
 * aggressor's board grows unopposed and the game ends quickly and deterministically.
 */
export function chooseAction(view: GameView, role: Role): ValidAction | null {
  const actions = view.valid_actions;
  if (actions.length === 0) return null;

  // Special windows the server opens on their own (no pass alongside).
  const mulligan = firstOfType(actions, 'mulligan_decision');
  if (mulligan) return mulligan;
  const discard = firstOfType(actions, 'discard');
  if (discard) return discard;
  const attack = firstOfType(actions, 'declare_attackers');
  if (attack) return attack; // filled empty for the passive role
  const block = firstOfType(actions, 'declare_blockers');
  if (block) return block; // filled empty for the passive role

  // Main-phase development: land first, for both roles.
  const land = firstOfType(actions, 'play_land');
  if (land) return land;

  if (role === 'aggressor') {
    const creature = highestCast(view, actions, subjectIsCreature);
    if (creature) return creature;
    const spell = highestCast(view, actions, () => true);
    if (spell) return spell;
    if (wantsToCast(view)) {
      const mana = actions.find((action) => isManaSource(view, action));
      if (mana) return mana;
    }
  }

  const pass = firstOfType(actions, PASS_PRIORITY);
  if (pass) return pass;
  return actions.find((action) => action.type !== 'concede') ?? actions[0];
}

/** The `count` candidate ids of greatest hand mana value (ties keep advertised
 * order) — for shedding the costliest cards on a forced discard. */
function highestManaValueIds(view: GameView, candidates: string[], count: number): string[] {
  return [...candidates]
    .map((id, index) => ({ id, index }))
    .sort((a, b) => {
      const av = manaValueOf(cardInHand(view, a.id)?.mana_cost ?? '');
      const bv = manaValueOf(cardInHand(view, b.id)?.mana_cost ?? '');
      return bv - av || a.index - b.index;
    })
    .slice(0, count)
    .map((entry) => entry.id);
}

/**
 * The entity ids to select in one walked multi-select slot, per role. Attackers:
 * the aggressor swings with every candidate that can deal damage (power ≥ 1), the
 * passive seat declares none. Blockers: none (the passive seat is the only
 * defender and never blocks; the aggressor is never attacked). Discard/bottom:
 * shed the costliest cards to satisfy the exact count.
 */
function chosenForSlot(
  view: GameView,
  role: Role,
  slot: { slot: string; candidates: string[]; count?: number },
): string[] {
  if (slot.slot === 'attackers') {
    if (role !== 'aggressor') return [];
    return slot.candidates.filter((id) => {
      const perm = permInPlay(view, id);
      return perm === undefined || powerOf(perm) >= 1;
    });
  }
  if (slot.slot.startsWith('block_')) return [];
  if (slot.count !== undefined) return highestManaValueIds(view, slot.candidates, slot.count);
  return [];
}

// --- DOM submission (clicks the real client UI) ----------------------------

/** Whether a prompt is a `select_from_zone` (count-bounded pick). */
function isSelectFromZone(
  prompt: Prompt,
): prompt is Extract<Prompt, { kind: 'select_from_zone' }> {
  return prompt.kind === 'select_from_zone';
}

/** Whether a prompt is an `option` (mulligan keep/take-another). */
function isOption(prompt: Prompt): prompt is Extract<Prompt, { kind: 'option' }> {
  return prompt.kind === 'option';
}

/**
 * Click a real `<button>` selected by `data-testid`. The interactive surfaces are
 * all real DOM buttons (the action bar, on-entity chips, and canvas-overlay
 * hotspots, ADR 0003), so an in-browser `.click()` drives the production `onClick`
 * exactly as a user tap would. Doing the find-and-click in a single `waitForFunction`
 * keeps it self-waiting (the button may render a frame after the prior action) yet
 * costs just one CDP round trip — Playwright's own locator + actionability pipeline
 * is ~1s per click on the animated Pixi surface, which across a whole game's clicks
 * blows the timeout. Not a sleep: it returns the instant the button exists.
 */
async function clickTestId(page: Page, testid: string): Promise<void> {
  // Fast path: one `evaluate` (~a few ms). The element is normally already present
  // (we act only while holding priority, and the view is stable). If it has not
  // rendered yet (e.g. a chip that appears a frame after selecting its entity), fall
  // back to a self-waiting `waitForFunction`.
  const clicked = await page.evaluate((id) => {
    const el = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    if (!el) return false;
    el.click();
    return true;
  }, testid);
  if (clicked) return;
  await page.waitForFunction(
    (id) => {
      const el = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
      if (!el) return false;
      el.click();
      return true;
    },
    testid,
    { timeout: 15_000, polling: 20 },
  );
}

/** Click the `<button>` inside `containerTestId` whose text is exactly `text` (the
 * action bar's global buttons and on-entity chips carry no per-id testid — they
 * render by label). Same fast-path-then-self-wait strategy as {@link clickTestId}. */
async function clickButtonByText(page: Page, containerTestId: string, text: string): Promise<void> {
  const find = ({ id, label }: { id: string; label: string }): boolean => {
    const container = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    if (!container) return false;
    const button = Array.from(container.querySelectorAll('button')).find(
      (b) => (b.textContent ?? '').trim() === label,
    );
    if (!button) return false;
    button.click();
    return true;
  };
  const arg = { id: containerTestId, label: text };
  if (await page.evaluate(find, arg)) return;
  await page.waitForFunction(find, arg, { timeout: 15_000, polling: 20 });
}

/** Click a global (subject-less) action's button in the action bar, by its label. */
async function clickGlobalAction(page: Page, action: ValidAction): Promise<void> {
  await clickButtonByText(page, 'action-bar', action.label);
}

/** Fire a subject-owned action: select the entity, then click its labelled chip. */
async function clickEntityAction(page: Page, entityId: string, action: ValidAction): Promise<void> {
  await clickTestId(page, `entity-${entityId}`);
  await clickButtonByText(page, `entity-actions-${entityId}`, action.label);
}

/**
 * Drive an open multi-select declaration to submission (issue #143 UX). Walks the
 * requirement slots then any select-from-zone slots in order — toggling each slot's
 * chosen candidates (advancing with "Next" between slots) — then submits: an option
 * button (mulligan keep) when the action poses one, else "Confirm".
 */
async function driveMultiSelect(page: Page, view: GameView, action: ValidAction, role: Role): Promise<void> {
  const reqSlots = (action.requirements ?? []).map((req: TargetRequirement) => ({
    slot: req.slot,
    candidates: req.candidates ?? [],
  }));
  const zoneSlots = (action.prompts ?? []).filter(isSelectFromZone).map((prompt) => ({
    slot: prompt.slot,
    candidates: prompt.candidates ?? [],
    count: prompt.count,
  }));
  const walked = [...reqSlots, ...zoneSlots];

  for (let i = 0; i < walked.length; i += 1) {
    if (i > 0) {
      // Advance to this slot so its candidates become the toggleable set.
      await clickButtonByText(page, 'action-bar', 'Next');
    }
    for (const id of chosenForSlot(view, role, walked[i])) {
      await clickTestId(page, `target-${id}`);
    }
  }

  const option = (action.prompts ?? []).find(isOption);
  if (option) {
    const options = option.options ?? [];
    const picked =
      options.find((o) => o.id.toLowerCase() === 'keep' || o.label.toLowerCase().includes('keep')) ??
      options[0];
    await clickTestId(page, `multiselect-option-${picked.id}`);
  } else {
    await clickTestId(page, 'multiselect-confirm');
  }
}

/**
 * Submit the chosen `action` by clicking the real UI. A combat declaration or any
 * action carrying prompts opens the multi-select flow; a subject-owned action fires
 * from its entity; a plain global action is a single action-bar click.
 */
async function submitAction(page: Page, view: GameView, action: ValidAction, role: Role): Promise<void> {
  const isCombat = action.type === 'declare_attackers' || action.type === 'declare_blockers';
  const isMulti = (action.prompts?.length ?? 0) > 0 || isCombat;
  const subject = action.subject ?? [];

  if (isMulti) {
    await clickGlobalAction(page, action);
    await driveMultiSelect(page, view, action, role);
    return;
  }
  if (subject.length > 0) {
    await clickEntityAction(page, subject[0], action);
    return;
  }
  await clickGlobalAction(page, action);
}

// --- Driver loop -----------------------------------------------------------

/** Safety cap on decisions per seat: a policy/wiring bug fails bounded, never hangs. */
const MAX_STEPS = 2000;

/**
 * Wait in-browser until the view differs from `before`, then return the fresh
 * snapshot. Folds "wait for the server's response" and "read the new view" into a
 * **single** CDP round trip (the browser main thread is busy driving two Pixi
 * canvases, so each round trip is comparatively costly — halving them per action
 * roughly halves the game's wall-clock).
 */
async function readChangedState(page: Page, before: string): Promise<ViewSnapshot> {
  const handle = await page.waitForFunction(
    (b) => {
      const view = window.__RUNE_TEST__?.view ?? null;
      const json = JSON.stringify(view);
      return json !== b ? { view, json } : false;
    },
    before,
    { timeout: 30_000, polling: 30 },
  );
  return handle.jsonValue() as Promise<ViewSnapshot>;
}

/**
 * Wait (auto-waiting, no sleeps) until at least one of `pages` can act again — its
 * view offers an action or carries a terminal result. Used only in the rare gap
 * where no seat holds priority (a transient between turns): it sleeps in-browser
 * until a seat becomes actionable, so the coordinator never busy-loops.
 */
async function waitForAnyActionable(pages: Page[]): Promise<void> {
  await Promise.race(
    pages.map((page) =>
      page.waitForFunction(
        () => {
          const view = window.__RUNE_TEST__?.view ?? null;
          return view !== null && (view.result !== undefined || view.valid_actions.length > 0);
        },
        undefined,
        { timeout: 30_000, polling: 30 },
      ),
    ),
  );
}

/** One seat the coordinator drives: its browser page and its policy role. */
export interface Seat {
  /** The seat's browser page. */
  page: Page;
  /** The policy this seat plays. */
  role: Role;
}

/**
 * Play a whole scripted game across all `seats` from a **single** coordinator loop,
 * returning each seat's terminal {@link GameResult} in seat order. Only one seat
 * holds priority at a time (bar the simultaneous mulligan), so a single loop that
 * acts on whichever seat is actionable — never two concurrent driver loops — keeps
 * exactly one browser operation in flight, avoiding the CDP contention that two
 * parallel `waitForFunction` loops on one connection would create (and which makes a
 * whole game far too slow). Each acted-upon seat's post-action wait is bounded to
 * that seat's own view changing, so no seat's action is read as stale.
 */
export async function playScriptedGame(seats: Seat[]): Promise<GameResult[]> {
  const debug = Boolean(process.env.SCRIPT_DEBUG);
  const started = Date.now();
  const results: (GameResult | null)[] = seats.map(() => null);
  // Cached latest snapshot per seat; `undefined` means "unknown, must re-read". A
  // seat's snapshot is invalidated whenever any seat acts (the shared game changed).
  const cached: (ViewSnapshot | undefined)[] = seats.map(() => undefined);
  // Each seat's own player id (`view.you`), learned on first sight — lets us skip
  // reading a seat the latest frame says does not hold priority (saving a CDP round
  // trip per action, the dominant cost under two live Pixi canvases).
  const seatYou: (string | undefined)[] = seats.map(() => undefined);
  // The player id that currently holds priority, per the most recent frame, or
  // `undefined` when unknown / nobody does (a between-turns gap, or game over — in
  // which case every seat is read so its terminal result is picked up).
  let priorityHolder: string | undefined;

  for (let step = 0; step < MAX_STEPS; step += 1) {
    if (results.every((r) => r !== null)) break;
    let acted = false;

    for (let i = 0; i < seats.length; i += 1) {
      if (results[i] !== null) continue;
      // Skip the round trip when we already know this seat lacks priority.
      if (
        cached[i] === undefined &&
        priorityHolder !== undefined &&
        seatYou[i] !== undefined &&
        seatYou[i] !== priorityHolder
      ) {
        continue;
      }
      const { page, role } = seats[i];
      const snapshot = cached[i] ?? (await readState(page));
      cached[i] = snapshot;
      const { view, json } = snapshot;
      if (view === null) continue;
      if (view.you) seatYou[i] = view.you;
      if (view.result) {
        results[i] = view.result;
        continue;
      }
      if (view.valid_actions.length === 0) continue;
      const action = chooseAction(view, role);
      if (action === null) continue;
      if (debug) {
        const lives = view.opponents.map((o) => `${o.player_id}:${o.life}`).join(' ');
        // eslint-disable-next-line no-console
        console.log(
          `[${role}] t=${((Date.now() - started) / 1000).toFixed(1)}s step=${step} ` +
            `phase=${view.phase} bf=${view.battlefield.filter((p) => p.controller === view.you).length} ` +
            `opp[${lives}] -> ${action.type} "${action.label}"`,
        );
      }
      await submitAction(page, view, action, role);
      // Fold the response-wait and the next read into one round trip; the acting
      // seat's fresh snapshot is kept (it may still hold priority — tap, tap, cast).
      const next = await readChangedState(page, json);
      cached[i] = next;
      // Once a terminal result lands, priority is nobody's — clear the holder so the
      // next pass reads every seat and records each one's terminal result.
      priorityHolder = next.view?.result ? undefined : next.view?.priority_player;
      // The shared game advanced: every other seat's cached snapshot is now stale.
      for (let j = 0; j < seats.length; j += 1) if (j !== i) cached[j] = undefined;
      acted = true;
    }

    // No seat could act this pass (a transient no-priority gap): wait for a frame,
    // then force a re-read of the pending seats (priority is now unknown).
    if (!acted && results.some((r) => r === null)) {
      const pending = seats.filter((_, i) => results[i] === null).map((s) => s.page);
      await waitForAnyActionable(pending);
      priorityHolder = undefined;
      for (let i = 0; i < seats.length; i += 1) if (results[i] === null) cached[i] = undefined;
    }
  }

  return results.map((result, i) => {
    if (result === null) {
      throw new Error(`seat ${i} did not reach a terminal result within ${MAX_STEPS} steps`);
    }
    return result;
  });
}
