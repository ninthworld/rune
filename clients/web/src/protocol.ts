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
 * One entry of {@link GameView.valid_actions}: the only source of interactivity.
 * `subject` names the entities an action belongs to so the client can render it
 * ON the entity rather than in a global bar (ADR 0004).
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
 * The client's chosen action: just the `id` of one issued `valid_actions` entry.
 * Serializes with a `type` discriminator (`{"type":"choose_action", ...}`),
 * matching the `ClientMessage::ChooseAction` variant.
 */
export interface ChooseAction {
  /** Discriminator for the client→server message envelope. */
  type: 'choose_action';
  /** The `id` of the chosen {@link ValidAction}. */
  action_id: string;
}

/** Build the client→server envelope for choosing an issued action. */
export function chooseAction(actionId: string): ChooseAction {
  return { type: 'choose_action', action_id: actionId };
}
