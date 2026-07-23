/**
 * Action targeting and prompt protocol: valid actions, target requirements, and choice prompts.
 */

import type { EntityId, PlayerId } from './index.js';

/**
 * One choice step of a multi-step {@link ValidAction}: a single target slot the
 * player must fill, listing exactly the legal candidates the server computed. The
 * client renders the prompt, highlights the candidates, and computes NO legality
 * of its own (ADR 0009 §Client) — it only ever offers these ids.
 */
export interface TargetRequirement {
  /** Opaque slot id the client echoes back as {@link TargetChoice.slot}. */
  slot: string;
  /** Human-readable prompt describing what to choose, e.g. `"target creature"`. */
  prompt: string;
  /**
   * The legal candidate entity ids for this slot — the **only** choices the
   * client may offer. Enumerated O(N) per slot by the server; the client never
   * derives or filters them. Omitted (treated as empty) when there is none.
   */
  candidates?: EntityId[];
}

/** One named choice of a {@link OptionPrompt} slot: an opaque `id` the client
 * echoes back and a human-readable `label` to render. */
export interface PromptOption {
  /** Opaque id echoed back as the slot's chosen value. Never parsed. */
  id: string;
  /** Human-readable label to render for this choice. */
  label: string;
}

/**
 * Pick exactly one of N named choices — the clean shape for a modal choice or a
 * yes/no (e.g. the mulligan keep/take-another decision). Answered with the chosen
 * {@link PromptOption.id} as the slot's single {@link TargetChoice.chosen} entry.
 */
export interface OptionPrompt {
  /** Discriminator. */
  kind: 'option';
  /** Opaque slot id the client echoes back as {@link TargetChoice.slot}. */
  slot: string;
  /** Human-readable prompt describing the decision. */
  prompt: string;
  /** The named choices — the only answers the client may submit. */
  options?: PromptOption[];
}

/**
 * Pick {@link SelectFromZonePrompt.count} entity ids from a zone (cleanup
 * discard-to-max, mulligan bottoming, future tutors). Answered with the selected
 * ids in {@link TargetChoice.chosen}; each must be one of `candidates`.
 */
export interface SelectFromZonePrompt {
  /** Discriminator. */
  kind: 'select_from_zone';
  /** Opaque slot id the client echoes back as {@link TargetChoice.slot}. */
  slot: string;
  /** Human-readable prompt describing what to select. */
  prompt: string;
  /** The zone the cards come from, e.g. `"hand"` (display context; free-form). */
  zone: string;
  /** The player who owns the zone being selected from. */
  owner: PlayerId;
  /** Exactly how many ids must be chosen. */
  count: number;
  /** The legal candidate entity ids — the only ids the client may pick. */
  candidates?: EntityId[];
}

/**
 * Arrange N items into an order (ordering simultaneous triggers, scry). Answered
 * with all of {@link OrderPrompt.items} in the chosen order in
 * {@link TargetChoice.chosen} (a permutation of `items`).
 */
export interface OrderPrompt {
  /** Discriminator. */
  kind: 'order';
  /** Opaque slot id the client echoes back as {@link TargetChoice.slot}. */
  slot: string;
  /** Human-readable prompt describing what to order. */
  prompt: string;
  /** The items to arrange, in their initial order. */
  items?: EntityId[];
}

/**
 * A non-target choice slot a {@link ValidAction} may pose, a generalization of the
 * {@link TargetRequirement} slot pattern (slot + prompt + candidates, bound by the
 * action's content {@link ValidAction.token}, ADR 0009) to three richer shapes.
 * The `kind` tag discriminates the shape on the wire; every shape is answered by a
 * {@link TargetChoice} keyed by `slot` and submitted atomically. Clients tolerate
 * an unknown future `kind`.
 */
export type Prompt = OptionPrompt | SelectFromZonePrompt | OrderPrompt;

/**
 * One entry of {@link GameView.valid_actions}: the only source of interactivity.
 * `subject` names the entities an action belongs to so the client can render it
 * ON the entity rather than in a global bar (ADR 0004).
 *
 * A multi-step action (a targeted spell/ability) also carries an ordered
 * {@link ValidAction.requirements} list, and/or a {@link ValidAction.prompts} list
 * of the non-target choice shapes, that the client walks as one prompt queue, plus
 * a content-binding {@link ValidAction.token} it echoes verbatim in the answer
 * (ADR 0009 §Protocol). All are absent for a plain, no-choice action.
 */
export interface ValidAction {
  /** Opaque id the client echoes back in a {@link ChooseAction} to take it. */
  id: string;
  /**
   * Coarse action category (e.g. `"pass_priority"`, `"activate_ability"`). A
   * free-form string, not a union, so new kinds do not break older clients.
   */
  type: string;
  /** Human-readable label to render for this action. */
  label: string;
  /** Entity ids this action belongs to; empty for global actions. */
  subject?: EntityId[];
  /**
   * Whether this action activates a **mana ability** (CR 605): it targets
   * nothing, does not use the stack, and only produces mana. Server-computed so
   * the client can offer a lighter gesture — one-click tap-for-mana (ADR 0025)
   * — for exactly these actions without ever classifying abilities itself.
   * Omitted when false.
   */
  mana_ability?: boolean;
  /**
   * Ordered choice steps this action requires before it can be taken — one per
   * target slot. The client walks them as a prompt queue and answers every slot
   * **atomically** in a single {@link ChooseAction}, never a stateful
   * multi-message handshake. Absent/empty for a plain action.
   */
  requirements?: TargetRequirement[];
  /**
   * Non-target choice slots this action poses (option / select_from_zone / order,
   * issue #156), a generalization of {@link ValidAction.requirements}. The client
   * walks them as part of the same prompt queue and answers each slot with a
   * {@link TargetChoice} keyed by `slot`. Absent for a plain action.
   */
  prompts?: Prompt[];
  /**
   * Content-binding token: an opaque server-issued value bound to this action's
   * exact content (subject + requirements + prompts). The client echoes it back
   * **verbatim** in {@link ChooseAction.token} and never parses or derives it; the
   * server rejects an answer whose token does not match. Absent only for a legacy
   * unbound action.
   */
  token?: string;
}
