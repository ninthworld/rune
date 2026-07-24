/**
 * In-match gestures for the browser suite (ADR 0011): the moves a player makes
 * once the table is up — tapping a land for mana, casting a spell (targeted or
 * not), declaring attackers with a defending player per attacker, and assigning
 * blockers.
 *
 * The two rules of `client.ts` bind every helper here just as hard:
 *
 * 1. **Every move is a click on a rendered control.** Selecting a permanent is a
 *    click on its plane control; firing its action is a click on the dock chip
 *    the selection echoes; picking a defending player is a click on that seat's
 *    crest. Nothing is submitted through `window.__RUNE_TEST__`, and the client
 *    is given no test-only control path.
 * 2. **Legality is never computed here.** Which permanents may be tapped, which
 *    spells may be cast, who may be attacked and what may block are read off the
 *    server's `valid_actions` / `requirements` purely to decide *which rendered
 *    control to click*. The suite derives none of it.
 *
 * Every wait is a wait on a condition — usually the specific effect the gesture
 * should have (mana in the pool, an object on the stack, an attacker recorded) —
 * so a gesture that silently did nothing fails where it happened.
 */
import { expect, type Page } from '@playwright/test';
import { clickReachablePoint, readView, stampOf, viewStamp, waitForStampChange } from './client';
import type { HookAction, HookRequirement, HookView } from './hook';

/** The requirement-slot id carrying the attacker subset of a declaration. */
const ATTACKERS_SLOT = 'attackers';
/** The prefix of a per-attacker defending-player slot (`defend_<permanentId>`). */
const DEFEND_SLOT_PREFIX = 'defend_';

/** Read every action of `type` the server currently offers this seat. */
export async function offeredActions(page: Page, type: string): Promise<HookAction[]> {
  return page.evaluate(
    (kind) => window.__RUNE_TEST__?.view?.valid_actions.filter((a) => a.type === kind) ?? [],
    type,
  );
}

/** The first action of `type` on offer, or `null`. */
export async function offeredAction(page: Page, type: string): Promise<HookAction | null> {
  return (await offeredActions(page, type))[0] ?? null;
}

/**
 * Click a permanent's control on the scene plane, then the dock chip the
 * selection echoes for `label`. This is the shipped two-gesture shape of ADR
 * 0025-as-superseded-by-#463: activation selects first, and the dock is the one
 * action home.
 */
async function fireOnPermanent(page: Page, entityId: string, label: string): Promise<void> {
  await clickReachablePoint(page, page.getByTestId(`entity-${entityId}`));
  const echo = page.getByTestId('selection-echo');
  await expect(echo, `selecting ${entityId} should echo its actions in the dock`).toBeVisible();
  await echo.getByRole('button', { name: label, exact: true }).click();
}

/**
 * Tap one land for mana through the rendered plane control and the dock chip —
 * the deliberate mana activation of issue #463 (no one-click shortcut).
 *
 * Resolves once the receiver's own `mana_pool` has grown, so a tap that produced
 * nothing fails here rather than confusingly at the cast that follows. Returns
 * the pool size before and after.
 */
export async function tapForManaThroughUi(
  page: Page,
  known?: HookView,
): Promise<{ permanentId: string; before: number; after: number }> {
  const action =
    known === undefined
      ? await page.evaluate(() => {
          const view = window.__RUNE_TEST__?.view;
          const mana = view?.valid_actions.find((a) => a.mana_ability === true);
          return mana === undefined
            ? null
            : {
                id: mana.subject?.[0] ?? null,
                label: mana.label,
                pool: view?.mana_pool.length ?? 0,
              };
        })
      : (() => {
          const mana = known.valid_actions.find((a) => a.mana_ability === true);
          return mana === undefined
            ? null
            : { id: mana.subject?.[0] ?? null, label: mana.label, pool: known.mana_pool.length };
        })();
  if (action === null) throw new Error('no mana ability is on offer to this seat');
  if (action.id === null) throw new Error('a mana ability was offered without a subject permanent');

  await fireOnPermanent(page, action.id, action.label);
  await page
    .waitForFunction(
      (before) => (window.__RUNE_TEST__?.view?.mana_pool.length ?? 0) > before,
      action.pool,
      { timeout: 20_000 },
    )
    .catch(() => {
      throw new Error(`tapping ${action.id} through the dock added no mana to the pool`);
    });
  const after = await page.evaluate(() => window.__RUNE_TEST__?.view?.mana_pool.length ?? 0);
  return { permanentId: action.id, before: action.pool, after };
}

/** What a cast put on the stack. */
export interface CastSpell {
  /** The hand card that was cast. */
  handCardId: string;
  /** Its face name. */
  name: string;
  /** The server's label for the cast action. */
  label: string;
  /** The target the suite picked, when the spell required one. */
  targetId?: string;
}

/**
 * Cast a spell from hand through the rendered affordances: click the hand card,
 * click the dock chip, and — when the server's first requirement slot asks for a
 * target — click the rendered candidate control the caller chose.
 *
 * `pickTarget` receives the server's own candidate list and returns the one to
 * click; it is a *choice among what the server offered*, never a legality
 * decision. Resolves once the spell is on the stack (or, for a spell that
 * resolves without ever being seen there, once the view has moved on).
 */
export async function castSpellThroughUi(
  page: Page,
  options: {
    /** Choose among the server's advertised targets; omitted picks the first. */
    pickTarget?: (candidates: string[], view: HookView) => string | undefined;
    /** Only consider casts whose requirement count matches this predicate. */
    accept?: (action: HookAction) => boolean;
    /** A view the caller already read, to save a round trip. */
    known?: HookView;
  } = {},
): Promise<CastSpell> {
  const view = options.known ?? (await readView(page));
  if (view === null) throw new Error('no view is published yet');
  const accept = options.accept ?? (() => true);
  const action = view.valid_actions.find((a) => a.type === 'cast_spell' && accept(a));
  if (action === undefined) throw new Error('no acceptable cast_spell is on offer to this seat');
  const handCardId = action.subject?.[0];
  if (handCardId === undefined) throw new Error('cast_spell was offered without a subject card');
  const name = view.my_hand.find((card) => card.id === handCardId)?.name;
  if (name === undefined) throw new Error(`the cast_spell subject ${handCardId} is not in hand`);

  const before = stampOf(view);
  await clickReachablePoint(page, page.getByTestId(`live-hand-card-${handCardId}`));
  const echo = page.getByTestId('selection-echo');
  await expect(echo, 'selecting a castable card should echo its actions in the dock').toBeVisible();
  await echo.getByRole('button', { name: action.label, exact: true }).click();

  let targetId: string | undefined;
  const slots = action.requirements ?? [];
  if (slots.length > 1) {
    // Nothing in the bundled catalog casts with two target slots yet; walking a
    // longer queue would need a per-slot answer, so fail loudly rather than hang
    // half-way through one.
    throw new Error(`${action.label} asks for ${slots.length} target slots; this helper fills one`);
  }
  const slot = slots[0];
  if (slot !== undefined) {
    const candidates = slot.candidates ?? [];
    if (candidates.length === 0) throw new Error(`${action.label} offered a slot with no candidate`);
    // The targeting strip must say, in words, what is being chosen — the
    // "target path visible" half of the loop, in the channel that survives
    // reduced motion.
    await expect(
      page.getByTestId('targeting-prompt'),
      'casting a targeted spell should put the target question in the prompt strip',
    ).toBeVisible();
    targetId = options.pickTarget?.(candidates, view) ?? candidates[0]!;
    // A player target is picked from that seat's crest, a permanent from its own
    // control on the plane. Which of the two a candidate is comes from the view's
    // seat list, not from parsing the id.
    const isSeat = view.seat_order.includes(targetId);
    await page.getByTestId(isSeat ? `target-player-${targetId}` : `target-${targetId}`).click();
  }

  await waitForStampChange(page, before);
  return { handCardId, name, label: action.label, targetId };
}

/** Everything a declared attack recorded, read back off the authoritative view. */
export interface DeclaredAttack {
  /** The attacking permanent's entity id. */
  attackerId: string;
  /** The defending player the suite clicked. */
  defenderId: string;
  /** Every defending player the server offered for that attacker. */
  offeredDefenders: string[];
  /** Whether Confirm was disabled while the defender slot was unanswered. */
  confirmBlockedWithoutDefender: boolean;
}

/** The `defend_<id>` slots of a declaration, paired with their attacker id. */
function defenderSlots(action: HookAction): Array<{ slot: HookRequirement; attackerId: string }> {
  return (action.requirements ?? [])
    .filter((slot) => slot.slot.startsWith(DEFEND_SLOT_PREFIX))
    .map((slot) => ({ slot, attackerId: `perm_${slot.slot.slice(DEFEND_SLOT_PREFIX.length)}` }));
}

/**
 * Open the declare-attackers decision from the dock, toggle one attacker, and
 * route it at a defending player — the multiplayer shape of issue #341/#347 and
 * the browser half of #457's criterion.
 *
 * The walk is deliberately explicit, because *the gating is the point*:
 *
 * 1. toggle the attacker on the plane (its own rendered control);
 * 2. read whether Confirm is still disabled — a declared attacker owes a
 *    defending player, so it must be;
 * 3. step to the per-attacker defender slot with the dock's own Next control;
 * 4. click one defending seat's crest, then a **different** one, so the recorded
 *    assignment can only be the seat that was actually clicked last;
 * 5. Confirm.
 *
 * Resolves once the authoritative view records the attacker with that defending
 * player. Which seats may be attacked is entirely the server's `candidates`.
 */
export async function declareAttackThroughUi(
  page: Page,
  options: {
    /** Run once the defending seat is chosen and before Confirm — the load-bearing
     * action state a screenshot wants to pin. */
    onDefenderChosen?: () => Promise<void>;
  } = {},
): Promise<DeclaredAttack> {
  const action = await offeredAction(page, 'declare_attackers');
  if (action === null) throw new Error('declare_attackers is not on offer to this seat');
  const attackers = (action.requirements ?? []).find((slot) => slot.slot === ATTACKERS_SLOT);
  const attackerId = attackers?.candidates?.[0];
  if (attackerId === undefined) throw new Error('declare_attackers offered no attacker candidate');

  await page.getByTestId('action-bar').getByRole('button', { name: action.label }).click();
  await expect(
    page.getByTestId('multiselect-prompt'),
    'opening the declaration should state the question in the prompt strip',
  ).toBeVisible();

  await page.getByTestId(`target-${attackerId}`).click();
  await expect(page.getByTestId('multiselect-count')).toContainText('1 selected');

  const confirm = page.getByTestId('multiselect-confirm');
  const confirmBlockedWithoutDefender = !(await confirm.isEnabled());

  // The defender slot only exists once its attacker is declared, so it is read
  // after the toggle — from the server's own requirement list, not derived.
  const slots = defenderSlots(action).filter((entry) => entry.attackerId === attackerId);
  const offeredDefenders = slots[0]?.slot.candidates ?? [];
  if (offeredDefenders.length < 2) {
    throw new Error(
      `expected several legal defending players, the server offered ${offeredDefenders.length}`,
    );
  }
  // Step to that slot through the dock's own control: the defender question is
  // a separate step of the same declaration, and it only exists at all because
  // an attacker is declared.
  await page.getByTestId('action-bar').getByRole('button', { name: 'Next', exact: true }).click();
  // The seat crests only *become* pick surfaces while the defender slot is the
  // active one, so their appearance is the proof that the step advanced.
  await expect(
    page.getByTestId(`target-player-${offeredDefenders[0]}`),
    'on the defender step every legal defending seat should be pickable at its crest',
  ).toBeVisible();

  // Pick one, then a different one: a defender the suite never clicked can then
  // never be what the view records.
  const [first, chosen] = [offeredDefenders[0]!, offeredDefenders[1]!];
  await page.getByTestId(`target-player-${first}`).click();
  await expect(confirm, 'a declared attacker with a defender should be confirmable').toBeEnabled();
  await page.getByTestId(`target-player-${chosen}`).click();
  await options.onDefenderChosen?.();
  await confirm.click();

  await page
    .waitForFunction(
      (expected) =>
        window.__RUNE_TEST__?.view?.battlefield.some(
          (permanent) =>
            permanent.id === expected.attackerId &&
            permanent.attacking === true &&
            permanent.attacking_player === expected.chosen,
        ) === true,
      { attackerId, chosen },
      { timeout: 20_000 },
    )
    .catch(() => {
      throw new Error(
        `the declaration never recorded ${attackerId} as attacking ${chosen} through the rendered controls`,
      );
    });

  return { attackerId, defenderId: chosen, offeredDefenders, confirmBlockedWithoutDefender };
}

/**
 * Answer a declare-blockers decision through the rendered controls: open it from
 * the dock, and for each per-attacker slot the server poses, toggle the first
 * candidate blocker it lists, then confirm.
 *
 * Returns the `(blocker, attacker)` pairs it declared — empty when the server
 * offered a declaration with no candidate, which is a legal answer (block
 * nothing) and is confirmed as such.
 */
export async function declareBlockersThroughUi(
  page: Page,
): Promise<Array<{ blockerId: string; attackerId: string }>> {
  const action = await offeredAction(page, 'declare_blockers');
  if (action === null) throw new Error('declare_blockers is not on offer to this seat');
  await page.getByTestId('action-bar').getByRole('button', { name: action.label }).click();

  const declared: Array<{ blockerId: string; attackerId: string }> = [];
  const used = new Set<string>();
  const slots = action.requirements ?? [];
  for (const [index, slot] of slots.entries()) {
    if (index > 0) {
      await page.getByTestId('action-bar').getByRole('button', { name: 'Next', exact: true }).click();
    }
    const blockerId = (slot.candidates ?? []).find((id) => !used.has(id));
    if (blockerId === undefined) continue;
    used.add(blockerId);
    await page.getByTestId(`target-${blockerId}`).click();
    declared.push({ blockerId, attackerId: slot.slot.replace(/^block_/, 'perm_') });
  }

  const before = await viewStamp(page);
  await page.getByTestId('multiselect-confirm').click();
  await waitForStampChange(page, before);
  return declared;
}
