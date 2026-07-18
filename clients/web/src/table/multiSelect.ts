/**
 * Multi-select session logic — a small **pure** state machine over an action's
 * server-supplied `requirements` and `prompts` (ADR 0009; issue #143).
 *
 * This generalizes the single-target flow in {@link ./targeting} from "pick
 * exactly one candidate per slot, then auto-submit" to "toggle a *set* of
 * candidates across one or more slots, then confirm". It backs the three combat /
 * cleanup declarations that are all subsets of a server-advertised candidate set:
 *
 * - **Declare attackers** — one `requirements` slot (`"attackers"`); the answer is
 *   any subset of the eligible attackers (optional: the empty set declares none).
 * - **Declare blockers** — one `requirements` slot **per declared attacker**
 *   (`"block_<id>"`); each answered with the subset of blockers assigned to that
 *   attacker (the per-attacker "two-level" pick: which attacker (slot) → which
 *   blockers (toggled candidates)).
 * - **Mulligan bottoming / discard / tutor** — a `select_from_zone` prompt slot
 *   with an exact `count`; the answer is exactly `count` cards from the zone.
 * - **Ordering (issue #157)** — an `order` prompt slot; the answer is all of its
 *   `items` arranged into the chosen order (a permutation), reordered rather than
 *   toggled. Every item is always included; only its position changes.
 *
 * As with targeting, **zero legality lives here**: candidate ids, counts, options,
 * and order items all come straight off the {@link ValidAction} the server issued;
 * the session only records which advertised candidates the player toggled (or the
 * order they arranged) per slot and assembles one atomic answer (`token` + one
 * {@link TargetChoice} per slot). A session is ephemeral UI state, reconstructable
 * from the current view plus the player's in-progress input, and discarded whenever
 * a fresh view arrives — nothing here is load-bearing across messages (hard rule:
 * zero game logic in the client).
 *
 * The keep/take-another **`option`** prompt is collected separately and answered by
 * the caller with the chosen option id; issue #157 renders it as a modal picker in
 * the prompt banner (the richer UX #143 deferred).
 */
import type { EntityId, Prompt, PromptOption, TargetChoice, ValidAction } from '../protocol';

/** How an action routes: a plain submit, single-target targeting, or multi-select. */
export type ActionMode = 'plain' | 'target' | 'multi';

/** The requirement-slot id of the attacker subset in a declare-attackers action. */
const ATTACKERS_SLOT = 'attackers';
/** The prefix of a per-attacker defender-choice slot (`defend_<permId>`, #347). */
const DEFEND_SLOT_PREFIX = 'defend_';

/**
 * Classify how firing an action is handled. `multi` covers the toggle-and-confirm
 * declarations (combat by `type`, plus any action carrying non-target `prompts`);
 * `target` is the single-target select-then-submit flow (see {@link ./targeting});
 * `plain` submits immediately. Keying combat off `type` is the documented contract
 * (protocol.md: "clients key off `type`/`subject`/`label`") — no legality is derived.
 */
export function classifyAction(action: ValidAction): ActionMode {
  const hasPrompts = (action.prompts?.length ?? 0) > 0;
  const isCombat = action.type === 'declare_attackers' || action.type === 'declare_blockers';
  if (hasPrompts || isCombat) return 'multi';
  if ((action.requirements?.length ?? 0) > 0) return 'target';
  return 'plain';
}

/** Whether an action should open the multi-select flow. */
export function isMultiSelect(action: ValidAction): boolean {
  return classifyAction(action) === 'multi';
}

/**
 * A walked slot: a subset requirement, a count-bounded zone pick, or an arrange
 * (`order`) list. All three are answered by one {@link TargetChoice} keyed by
 * `slot`; they differ only in how the player edits the chosen ids.
 */
export interface MultiSelectSlot {
  /** Opaque slot id the answer keys back to. */
  slot: string;
  /** Human-readable prompt describing what to select or arrange. */
  prompt: string;
  /**
   * `subset` — any number of the candidates (attackers/blockers, optional).
   * `count` — exactly {@link MultiSelectSlot.count} of them (bottoming/discard).
   * `order` — all of the candidates, arranged into the chosen order (a permutation).
   * `defender` — exactly one candidate (a defending player), for the multiplayer
   *   per-attacker attack-target choice (`defend_<id>`, issue #341/#347). Active only
   *   while its {@link MultiSelectSlot.attacker} is chosen in the `attackers` slot.
   */
  kind: 'subset' | 'count' | 'order' | 'defender';
  /**
   * The server-listed candidate entity ids — for `subset`/`count` the only ids the
   * client may toggle; for `order` the full item set the player arranges; for
   * `defender` the defending-player ids the attacker may be assigned to.
   */
  candidates: EntityId[];
  /** For a `count` slot, exactly how many must be chosen. */
  count?: number;
  /**
   * For a `defender` slot, the attacker (permanent entity id) whose defending player
   * this slot chooses. The slot is walked only while that attacker is in the
   * `attackers` selection — the client never asks whom a *non-attacking* creature
   * attacks (issue #347). Absent for every other slot kind.
   */
  attacker?: EntityId;
  /**
   * For a `select_from_zone` slot, the zone the cards come from (`"hand"`,
   * `"graveyard"`, …). Display context only — the client renders candidates in
   * place when the zone is on the board (hand) and in an overlay list when it is
   * not (graveyard/library). Absent for a `subset` (combat) or `order` slot.
   */
  zone?: string;
}

/** A named-option prompt (mulligan keep/take-another) carried as a submit trigger. */
export interface MultiSelectOption {
  /** Opaque slot id the chosen option id keys back to. */
  slot: string;
  /** Human-readable prompt describing the decision. */
  prompt: string;
  /** The named choices; each answered as the slot's single chosen id. */
  options: PromptOption[];
}

/**
 * An in-progress multi-select: the action being answered, its walked candidate
 * slots and any option slots, the index of the slot currently being toggled, and
 * the chosen ids per walked slot (aligned with {@link MultiSelectSession.slots}).
 */
export interface MultiSelectSession {
  /** The action being resolved (carries `requirements`/`prompts` and `token`). */
  action: ValidAction;
  /** The walked candidate slots, in requirement-then-prompt order. */
  slots: MultiSelectSlot[];
  /** Option prompts (rendered as submit triggers; full UX is #157). */
  options: MultiSelectOption[];
  /** Index of the slot the player is currently toggling. */
  active: number;
  /** Chosen ids per walked slot, aligned with {@link MultiSelectSession.slots}. */
  chosen: EntityId[][];
}

/** Whether a prompt is a `select_from_zone` (has a `kind` discriminator + count). */
function isSelectFromZone(prompt: Prompt): prompt is Extract<Prompt, { kind: 'select_from_zone' }> {
  return prompt.kind === 'select_from_zone';
}

/** Whether a prompt is an `option` slot. */
function isOption(prompt: Prompt): prompt is Extract<Prompt, { kind: 'option' }> {
  return prompt.kind === 'option';
}

/** Whether a prompt is an `order` slot (arrange N items). */
function isOrder(prompt: Prompt): prompt is Extract<Prompt, { kind: 'order' }> {
  return prompt.kind === 'order';
}

/**
 * Begin a multi-select over an action. Walked slots are its target `requirements`
 * (subset) followed by any `select_from_zone` (count) and `order` prompts;
 * `option` prompts are collected separately (answered by the caller). An `order`
 * slot starts pre-filled with its items in the server's initial order, since every
 * item is included and only its position changes. Callers gate this on
 * {@link isMultiSelect}.
 */
export function beginMultiSelect(action: ValidAction): MultiSelectSession {
  const slots: MultiSelectSlot[] = (action.requirements ?? []).map((req) => {
    // A multiplayer per-attacker attack-target slot (`defend_<permId>`, issue #347):
    // a single defending-player pick, walked only while its attacker is declared.
    // The attacker's entity id is `perm_<permId>` — the same `PermanentId` the slot
    // is keyed by (server `defender_slot`/`permanent_entity_id`).
    if (req.slot.startsWith(DEFEND_SLOT_PREFIX)) {
      return {
        slot: req.slot,
        prompt: req.prompt,
        kind: 'defender' as const,
        candidates: req.candidates ?? [],
        attacker: `perm_${req.slot.slice(DEFEND_SLOT_PREFIX.length)}`,
      };
    }
    return {
      slot: req.slot,
      prompt: req.prompt,
      kind: 'subset' as const,
      candidates: req.candidates ?? [],
    };
  });
  const options: MultiSelectOption[] = [];
  for (const prompt of action.prompts ?? []) {
    if (isSelectFromZone(prompt)) {
      slots.push({
        slot: prompt.slot,
        prompt: prompt.prompt,
        kind: 'count',
        candidates: prompt.candidates ?? [],
        count: prompt.count,
        zone: prompt.zone,
      });
    } else if (isOrder(prompt)) {
      slots.push({
        slot: prompt.slot,
        prompt: prompt.prompt,
        kind: 'order',
        candidates: prompt.items ?? [],
      });
    } else if (isOption(prompt)) {
      options.push({ slot: prompt.slot, prompt: prompt.prompt, options: prompt.options ?? [] });
    }
  }
  // An `order` slot is answered with all its items, so it starts pre-filled in the
  // server's initial order; every other slot starts empty (nothing chosen yet).
  const chosen = slots.map((slot) => (slot.kind === 'order' ? [...slot.candidates] : []));
  return { action, slots, options, active: 0, chosen };
}

/** The attacker permanent ids currently chosen in the `attackers` slot — the set a
 * `defender` slot is gated on (issue #347). Empty when there is no attackers slot. */
function chosenAttackers(session: MultiSelectSession): EntityId[] {
  const i = session.slots.findIndex((slot) => slot.slot === ATTACKERS_SLOT);
  return i < 0 ? [] : (session.chosen[i] ?? []);
}

/**
 * Whether slot `i` is *in play* right now. Every slot is in play except a `defender`
 * slot whose attacker is not currently declared: the client never asks whom a
 * creature that is not attacking attacks (issue #347). A two-player action (no
 * `defender` slots) has every slot always in play, so this is a no-op there.
 */
function isSlotInPlay(session: MultiSelectSession, i: number): boolean {
  const slot = session.slots[i];
  if (!slot) return false;
  if (slot.kind !== 'defender') return true;
  return slot.attacker !== undefined && chosenAttackers(session).includes(slot.attacker);
}

/** The slot the player is currently toggling, or `null` if there are none. */
export function activeSlot(session: MultiSelectSession): MultiSelectSlot | null {
  return session.slots[session.active] ?? null;
}

/** The active slot's server candidates — the only ids the UI may make toggleable. */
export function activeCandidates(session: MultiSelectSession): EntityId[] {
  return activeSlot(session)?.candidates ?? [];
}

/** Whether the active slot is a per-attacker defender pick (its candidates are
 * defending players, picked from the player HUD rather than the board, #347). */
export function activeIsDefender(session: MultiSelectSession): boolean {
  return activeSlot(session)?.kind === 'defender';
}

/** The attacker permanent the active `defender` slot is assigning, so the board can
 * highlight which creature the player is routing; `null` outside a defender slot. */
export function activeAttacker(session: MultiSelectSession): EntityId | null {
  const slot = activeSlot(session);
  return slot?.kind === 'defender' ? (slot.attacker ?? null) : null;
}

/** The ids already chosen in the active slot (for the pressed/selected affordance). */
export function activeChosen(session: MultiSelectSession): EntityId[] {
  return session.chosen[session.active] ?? [];
}

/**
 * Toggle a candidate in the active slot: add it if absent, remove it if present.
 * A no-op for an id the active slot did not advertise (the UI only offers listed
 * candidates, so this never invents a target). Returns the advanced session.
 */
export function toggle(session: MultiSelectSession, entityId: EntityId): MultiSelectSession {
  const slot = activeSlot(session);
  if (!slot || !slot.candidates.includes(entityId)) return session;
  const current = session.chosen[session.active] ?? [];
  // A `defender` slot is a single choice (whom this attacker attacks): picking a
  // candidate replaces any prior pick rather than accumulating (issue #347). Every
  // other slot is a toggling subset.
  const next =
    slot.kind === 'defender'
      ? [entityId]
      : current.includes(entityId)
        ? current.filter((id) => id !== entityId)
        : [...current, entityId];
  const chosen = session.chosen.map((ids, i) => (i === session.active ? next : ids));
  return { ...session, chosen };
}

/**
 * Toggle a candidate is not how an `order` slot is edited — its items are
 * rearranged. Move `entityId` one step within the active slot's order (`-1` earlier,
 * `+1` later), returning the advanced session. A no-op for a non-order slot, an id
 * not present, or a move off either end (the UI disables those controls anyway).
 */
export function moveInActiveSlot(
  session: MultiSelectSession,
  entityId: EntityId,
  direction: -1 | 1,
): MultiSelectSession {
  const slot = activeSlot(session);
  if (!slot || slot.kind !== 'order') return session;
  const current = session.chosen[session.active] ?? [];
  const from = current.indexOf(entityId);
  const to = from + direction;
  if (from < 0 || to < 0 || to >= current.length) return session;
  const next = [...current];
  [next[from], next[to]] = [next[to], next[from]];
  const chosen = session.chosen.map((ids, i) => (i === session.active ? next : ids));
  return { ...session, chosen };
}

/**
 * Whether one slot's current selection meets its constraint: a `count` slot needs
 * exactly `count`; an `order` slot needs all its items present (always true after a
 * pre-fill + reorders); a `subset` slot is always satisfied (even the empty set).
 */
function slotSatisfied(slot: MultiSelectSlot, chosen: EntityId[]): boolean {
  if (slot.kind === 'count') return chosen.length === (slot.count ?? 0);
  if (slot.kind === 'order') return chosen.length === slot.candidates.length;
  // A declared attacker must be assigned exactly one defending player (issue #347).
  if (slot.kind === 'defender') return chosen.length === 1;
  return true;
}

/** Whether every in-play slot's selection meets its constraint (drives Confirm). A
 * `defender` slot for an undeclared attacker is not in play, so it never blocks the
 * confirm — declaring no attackers, or attackers whose defenders are all chosen, is
 * submittable (issue #347). */
export function allSlotsSatisfied(session: MultiSelectSession): boolean {
  return session.slots.every(
    (slot, i) => !isSlotInPlay(session, i) || slotSatisfied(slot, session.chosen[i] ?? []),
  );
}

/**
 * Whether an option submit is allowed: no `count` slot is left **partially** filled
 * (each is either untouched or exactly satisfied). This is the client-side count
 * affordance for mulligan bottoming — a partial bottom pick blocks the keep/take-
 * another buttons — without encoding which option means "keep" (that gating is #157).
 */
export function optionsSubmittable(session: MultiSelectSession): boolean {
  return session.slots.every((slot, i) => {
    if (slot.kind !== 'count') return true;
    const n = (session.chosen[i] ?? []).length;
    return n === 0 || n === (slot.count ?? 0);
  });
}

/** Whether the active slot is the last *in-play* slot — i.e. there is no later slot
 * still to walk (a `defender` slot for an undeclared attacker is skipped, #347). */
export function isLastSlot(session: MultiSelectSession): boolean {
  for (let i = session.active + 1; i < session.slots.length; i += 1) {
    if (isSlotInPlay(session, i)) return false;
  }
  return true;
}

/** Advance to the next *in-play* walked slot (clamped to the current when there is
 * none later), keeping selections. Skips `defender` slots whose attacker is not
 * declared, so the player only assigns targets for the creatures actually attacking
 * (issue #347). */
export function advance(session: MultiSelectSession): MultiSelectSession {
  for (let i = session.active + 1; i < session.slots.length; i += 1) {
    if (isSlotInPlay(session, i)) return { ...session, active: i };
  }
  return session;
}

/** Whether this action carries an option prompt (mulligan keep/take-another). */
export function hasOptions(session: MultiSelectSession): boolean {
  return session.options.length > 0;
}

/**
 * Assemble the atomic answer: one {@link TargetChoice} per walked slot (keyed by
 * `slot`, carrying its chosen ids — an empty subset legally declares none), plus
 * any option decisions supplied by the caller (e.g. the chosen keep/mulligan id).
 * The store submits this together with the action's content-binding token.
 */
export function assembleChoices(
  session: MultiSelectSession,
  optionChoices: TargetChoice[] = [],
): TargetChoice[] {
  const slotChoices = session.slots
    // A `defender` slot for an undeclared attacker is not in play — omit it so a stale
    // target pick for a creature the player is not attacking with is never submitted
    // (issue #347). Every in-play slot (attackers, each declared attacker's defender,
    // and non-combat slots) is included, an empty subset legally declaring none.
    .map((slot, i) => ({ slot, i }))
    .filter(({ i }) => isSlotInPlay(session, i))
    .map(({ slot, i }) => ({ slot: slot.slot, chosen: session.chosen[i] ?? [] }));
  return [...optionChoices, ...slotChoices];
}
