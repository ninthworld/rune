/**
 * RUNE protocol — the TypeScript mirror of the `rune-protocol` crate.
 *
 * These are the wire shapes the server and every client share (see
 * `docs/protocol.md` and `crates/rune-protocol/src/lib.rs`). The client is a dumb
 * renderer: it displays a {@link GameView} and echoes back a `ChooseAction` with
 * the `id` of one issued {@link ValidAction}. It never computes legality, cost,
 * or effect — all displayed characteristics are server-computed.
 *
 * Any change to these shapes is a contract change: it must update the Rust crate
 * and `docs/protocol.md` in the same PR.
 */

/** Opaque player identity (server-assigned). */
export type PlayerId = string;

/** Opaque per-game entity id: a card, permanent, or stack object. */
export type EntityId = string;

/**
 * A card object, shown only to a player entitled to see it. All characteristics
 * are server-computed; the client never derives them.
 */
export interface CardView {
  /** Entity id of this card instance. */
  id: EntityId;
  /** Display name. */
  name: string;
  /** e.g. `"Creature — Elf Warrior"`. */
  type_line: string;
  /** Displayed mana cost string, e.g. `"{1}{G}"`. Absent for cards without one. */
  mana_cost?: string;
  /** Rules text as displayed. Omitted (defaults to `""`) when empty. */
  oracle_text?: string;
  /** Displayed power (a string so `*` round-trips). Present only for creatures. */
  power?: string;
  /** Displayed toughness; see {@link CardView.power}. Present only for creatures. */
  toughness?: string;
}

/**
 * What the receiving player may know about an opponent: hidden zones are reduced
 * to counts, public state is exact.
 */
export interface OpponentView {
  /** Which opponent this describes. */
  player_id: PlayerId;
  /** Number of cards in hand (contents hidden). */
  hand_size: number;
  /** Current life total. */
  life: number;
  /** Number of cards left in library. */
  library_size: number;
  /** Number of cards in the graveyard. */
  graveyard_size: number;
  /** Free-form status labels (e.g. `"monarch"`) for display only. */
  statuses?: string[];
}

/** A named counter on a permanent. */
export interface Counter {
  /** Counter name, e.g. `"+1/+1"` or `"loyalty"`. */
  kind: string;
  /** How many of this counter are present. */
  count: number;
}

/** A permanent on the battlefield with its server-computed characteristics. */
export interface Permanent {
  /** Entity id of this permanent. */
  id: EntityId;
  /** Player who currently controls it. */
  controller: PlayerId;
  /** Player who owns it (matters when control changes). */
  owner: PlayerId;
  /** The permanent's current (computed) card face. */
  card: CardView;
  /** Whether the permanent is tapped. Omitted (defaults to `false`) when untapped. */
  tapped?: boolean;
  /** Named counters and their quantities. */
  counters?: Counter[];
}

/**
 * One object on the stack — a spell or an ability. Ability entries carry their
 * source permanent so the client can point back at it.
 */
export interface StackItem {
  /** Entity id of this stack object. */
  id: EntityId;
  /** Player who controls it (chooses targets/resolution). */
  controller: PlayerId;
  /** Spell name or ability text as it should be displayed. */
  description: string;
  /** Source permanent for an ability; absent for a spell. */
  source?: EntityId;
}

/** A public, ordered pile owned by one player (graveyard or exile). */
export interface ZonePile {
  /** Player who owns the pile. */
  player_id: PlayerId;
  /** Cards in zone order (top last). */
  cards: CardView[];
}

/**
 * Every `Phase` value in turn order, for runtime validation of the wire. This is
 * the single source of truth: the {@link Phase} union is derived from it, so a
 * value added here can never drift out of the type (and vice versa). Mirrors the
 * `Phase` enum's snake_case serde encoding in `crates/rune-protocol/src/lib.rs`.
 */
export const PHASES = [
  'untap',
  'upkeep',
  'draw',
  'precombat_main',
  'begin_combat',
  'declare_attackers',
  'declare_blockers',
  'combat_damage',
  'end_combat',
  'postcombat_main',
  'end',
  'cleanup',
] as const;

/** The current turn step; one of {@link PHASES}. */
export type Phase = (typeof PHASES)[number];

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

/**
 * One entry of {@link GameView.valid_actions}: the only source of interactivity.
 * `subject` names the entities an action belongs to so the client can render it
 * ON the entity rather than in a global bar (ADR 0004).
 *
 * A multi-step action (a targeted spell/ability) also carries an ordered
 * {@link ValidAction.requirements} list the client walks as a prompt queue, plus
 * a content-binding {@link ValidAction.token} it echoes verbatim in the answer
 * (ADR 0009 §Protocol). Both are absent for a plain, no-choice action.
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
   * Ordered choice steps this action requires before it can be taken — one per
   * target slot. The client walks them as a prompt queue and answers every slot
   * **atomically** in a single {@link ChooseAction}, never a stateful
   * multi-message handshake. Absent/empty for a plain action.
   */
  requirements?: TargetRequirement[];
  /**
   * Content-binding token: an opaque server-issued value bound to this action's
   * exact content. The client echoes it back **verbatim** in
   * {@link ChooseAction.token} and never parses or derives it; the server rejects
   * an answer whose token does not match. Absent only for a legacy unbound action.
   */
  token?: string;
}

/**
 * The personalized state the server sends after every change. A client must be
 * able to fully reconstruct its UI from a single `GameView` — no client state is
 * load-bearing across messages. Optional collections may be omitted on the wire
 * and MUST be treated as their empty default (see {@link normalizeGameView}).
 */
export interface GameView {
  /**
   * The receiver's own seat entity id (the `p{N}` form used for players
   * throughout the view). The client uses this to identify itself rather than
   * inferring it from the zones. An older server may omit it on the wire, in
   * which case {@link normalizeGameView} defaults it to `''`.
   */
  you: PlayerId;
  /** Full card objects for the receiving player only. */
  my_hand: CardView[];
  /** Redacted views of every other player. */
  opponents: OpponentView[];
  /** All permanents in play. */
  battlefield: Permanent[];
  /** The stack, bottom first. */
  stack: StackItem[];
  /** Each player's graveyard. */
  graveyards: ZonePile[];
  /** Each player's exile zone. */
  exile: ZonePile[];
  /** The current turn step. */
  phase: Phase;
  /** The receiving player's unspent mana, as pip strings (e.g. `["{G}"]`). */
  mana_pool: string[];
  /** The player who currently holds priority, if any. */
  priority_player?: PlayerId;
  /** The only source of interactivity: what the receiving player may do now. */
  valid_actions: ValidAction[];
  /** Seconds remaining for the pending decision, if a clock is running. */
  action_deadline?: number;
}

/**
 * The player's answer to one {@link TargetRequirement} slot: the selected entity
 * ids, keyed back to the slot by `slot`. Each id must be one of that slot's
 * advertised {@link TargetRequirement.candidates} or the server treats the action
 * as a no-op.
 */
export interface TargetChoice {
  /** The {@link TargetRequirement.slot} this answers. */
  slot: string;
  /**
   * The entity ids chosen for this slot (one for a single-target slot; the list
   * generalizes to multi-select choices the model defers for now).
   */
  chosen?: EntityId[];
}

/**
 * The client's chosen action, answered atomically: the `id` of one issued
 * `valid_actions` entry, its content-binding {@link ChooseAction.token} echoed
 * verbatim, and one {@link ChooseAction.targets} entry per requirement slot.
 * Serializes with a `type` discriminator (`{"type":"choose_action", ...}`),
 * matching the `ClientMessage::ChooseAction` variant. `token` and `targets` are
 * omitted when empty, so a plain action's message is just the id.
 */
export interface ChooseAction {
  /** Discriminator for the client→server message envelope. */
  type: 'choose_action';
  /** The `id` of the chosen {@link ValidAction}. */
  action_id: string;
  /** The chosen action's {@link ValidAction.token}, echoed verbatim (or omitted). */
  token?: string;
  /** One entry per {@link ValidAction.requirements} slot; omitted when empty. */
  targets?: TargetChoice[];
}

/**
 * Build the client→server envelope for choosing an issued action. `token` (the
 * chosen action's content-binding token) and `targets` (the assembled per-slot
 * selection) are included only when present, so a plain action stays a bare id —
 * this is pure envelope assembly, never legality or effect computation.
 */
export function chooseAction(
  actionId: string,
  token?: string,
  targets?: TargetChoice[],
): ChooseAction {
  const message: ChooseAction = { type: 'choose_action', action_id: actionId };
  if (token !== undefined && token !== '') message.token = token;
  if (targets !== undefined && targets.length > 0) message.targets = targets;
  return message;
}
