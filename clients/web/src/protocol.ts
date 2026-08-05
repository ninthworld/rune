/**
 * The TypeScript mirror of `crates/sage-protocol`.
 *
 * The Rust types are the wire authority; this file and `docs/protocol.md` change with them in
 * the same PR (root `AGENTS.md`). Every shape here is defined once, as a schema, with the
 * static type inferred from it — so there is no second declaration that can drift from the
 * validator.
 *
 * Two rules govern how faithful this has to be, and they pull in opposite directions:
 *
 * 1. **Tolerate unknown fields.** A newer server may send fields this client does not know,
 *    and the client must keep working. `z.object` strips unknown keys rather than failing,
 *    which is exactly that tolerance.
 * 2. **Declare every field the server actually sends.** Rule 1 makes drift silent: a field the
 *    server sends and this file omits is discarded with no error. The parity test in
 *    `protocol.test.ts` closes that hole by asserting a parsed fixture is byte-identical to the
 *    fixture — anything stripped is a field this mirror is missing.
 *
 * Consequently **no schema here declares a default**. A `.default([])` would materialize a key
 * the wire did not carry and break that equality check, and it would also lie about what the
 * server said. Absent means absent; `normalize.ts` is where a UI turns absence into a default.
 */
import { z } from 'zod'

/** Opaque player identity (server-assigned). */
export const PlayerId = z.string()
/** Opaque per-game entity id: a card, permanent, or stack object. */
export const EntityId = z.string()

export const SessionToken = z.string()
export const RoomId = z.string()
export const GameSetupId = z.string()
export const CardIdentity = z.string()

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

/** A colour letter as it rides the wire — `Color` serializes to its single letter. */
export const Color = z.enum(['W', 'U', 'B', 'R', 'G'])
export type Color = z.infer<typeof Color>

export const COLORS: readonly Color[] = ['W', 'U', 'B', 'R', 'G'] as const

export const MatchFormat = z.object({
  id: GameSetupId.optional(),
  commander: z.boolean().optional(),
})
export type MatchFormat = z.infer<typeof MatchFormat>

export const CommanderIdentity = z.object({
  commander: PlayerId,
  name: z.string().optional(),
  color_identity: z.array(Color).optional(),
})
export type CommanderIdentity = z.infer<typeof CommanderIdentity>

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

export const GameOverReason = z.enum(['life_zero', 'decked', 'concede', 'commander_damage'])
export type GameOverReason = z.infer<typeof GameOverReason>

export const CommanderDamage = z.object({
  commander: PlayerId,
  damaged: PlayerId,
  amount: z.number(),
})
export type CommanderDamage = z.infer<typeof CommanderDamage>

export const CommanderTax = z.object({
  commander: PlayerId,
  casts: z.number().optional(),
  tax: z.number().optional(),
})
export type CommanderTax = z.infer<typeof CommanderTax>

export const GameResult = z.object({
  winner: PlayerId.optional(),
  losers: z.array(PlayerId).optional(),
  reason: GameOverReason,
})
export type GameResult = z.infer<typeof GameResult>

// ---------------------------------------------------------------------------
// Cards, board, zones
// ---------------------------------------------------------------------------

export const Phase = z.enum([
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
])
export type Phase = z.infer<typeof Phase>

/**
 * A card type (CR 300). A closed set, mirroring the engine's own — which is why
 * subtypes are not here: they are thousands, they belong to the printed sentence, and
 * nothing presentational keys off them.
 */
export const CardType = z.enum([
  'land',
  'creature',
  'artifact',
  'enchantment',
  'instant',
  'sorcery',
  'planeswalker',
  'battle',
])
export type CardType = z.infer<typeof CardType>

export const CardView = z.object({
  id: EntityId,
  name: z.string(),
  type_line: z.string(),
  mana_cost: z.string().optional(),
  rules_text: z.string().optional(),
  functional_id: z.string().optional(),
  /**
   * A token (CR 111): a permanent the game created, with no card behind it. Absent
   * means a card — the flag rides the wire only when `true`. A token's
   * `functional_id` is always empty, so anything keyed on card identity (a local
   * cache, a presentation lookup) must skip it rather than treat it as unresolved.
   */
  token: z.boolean().optional(),
  power: z.string().optional(),
  toughness: z.string().optional(),
  /**
   * Printed **starting** loyalty (CR 306.5b) — the number in a planeswalker's corner,
   * what it enters the battlefield with. Absent on every other card. This is *not* how
   * much loyalty a planeswalker on the battlefield has: that is its `loyalty` entry in
   * `Permanent.counters`, and rendering this one on the board would show the printed
   * number for a planeswalker that has already spent down.
   */
  loyalty: z.string().optional(),
  keywords: z.array(z.string()).optional(),
  /**
   * The card's types (CR 300), as the structured set `type_line` is rendered from.
   *
   * Stated so no surface has to parse the sentence. `type_line` is what to *print*;
   * this is what to group, arrange, and lay out by. Both come from one projection
   * server-side, so they cannot disagree — and an absent list is "not stated", never
   * "no types", which is why a permanent without one still renders.
   */
  card_types: z.array(CardType).optional(),
  /**
   * The card's **colour identity** (CR 903.4): its colours, the colours of the mana
   * symbols in its cost, and the colours of the mana symbols in its rules text.
   *
   * Stated because the printed cost — the one thing a client can read for itself —
   * is silent on exactly the cards a board is scanned by colour most: a Forest
   * prints no coloured pip and would otherwise draw as colourless. Not the card's
   * colour (CR 105), and never rendered as one. In WUBRG order.
   */
  color_identity: z.array(Color).optional(),
})
export type CardView = z.infer<typeof CardView>

export const OpponentView = z.object({
  player_id: PlayerId,
  hand_size: z.number(),
  life: z.number(),
  library_size: z.number(),
  graveyard_size: z.number(),
  statuses: z.array(z.string()).optional(),
  eliminated: z.boolean().optional(),
  /** Absent means **connected** — the flag rides the wire only when `false`. */
  connected: z.boolean().optional(),
  ai: z.boolean().optional(),
})
export type OpponentView = z.infer<typeof OpponentView>

export const SelfView = z.object({
  life: z.number(),
  library_size: z.number(),
  eliminated: z.boolean().optional(),
  /** Absent means **connected**, as on `OpponentView`. */
  connected: z.boolean().optional(),
  ai: z.boolean().optional(),
})
export type SelfView = z.infer<typeof SelfView>

export const Counter = z.object({
  kind: z.string(),
  count: z.number(),
})
export type Counter = z.infer<typeof Counter>

export const Permanent = z.object({
  id: EntityId,
  controller: PlayerId,
  owner: PlayerId,
  card: CardView,
  /**
   * The **physical card** (CR 108.1) this permanent is a projection of — the same id that
   * card carries as its `CardView.id` in a hand, on the stack, in a graveyard, in exile.
   *
   * **Not object identity.** CR 400.7: an object that changes zone becomes a *new object*
   * with no relation to its previous existence. The permanent that died and the card in
   * the graveyard are two objects with two ids, correctly. This says only that both are
   * projections of one physical card — enough to follow a card across the table, and never
   * enough to conclude that counters, damage, auras, control, or anything else came with
   * it. It addresses nothing: `id` stays the only handle for this permanent.
   *
   * Absent for a **token** (CR 111), which is not a card — and CR 111.7 means its instance
   * could never turn up in a zone to join to. Absent from an older server.
   */
  physical_card: EntityId.optional(),
  tapped: z.boolean().optional(),
  attacking: z.boolean().optional(),
  /**
   * The **defending player** this attacker is attacking — the seat that answers for the
   * attack. When a planeswalker is being attacked this is its controller, not the
   * planeswalker; `attacking_planeswalker` names the planeswalker itself.
   */
  attacking_player: EntityId.optional(),
  /**
   * The **planeswalker** this attacker is attacking (CR 508.1a), when it is attacking
   * one rather than a player. Absent otherwise. Server-stated: the client never works
   * out what is being attacked from which collection an id turns up in.
   */
  attacking_planeswalker: EntityId.optional(),
  blocking: EntityId.optional(),
  damage: z.number().optional(),
  attached_to: EntityId.optional(),
  is_commander: z.boolean().optional(),
  counters: z.array(Counter).optional(),
  /**
   * Whether **summoning sickness currently restricts this permanent** (CR 302.6):
   * it is a creature, its controller has not controlled it continuously since the
   * start of their most recent turn, and it does not have haste (CR 702.10b).
   *
   * A *restriction*, not a property: a summoning-sick creature with haste reports
   * `false`, because the restriction is what a player is looking at. No client can
   * work it out — continuous control is stored engine state and haste may be
   * granted — so it is stated, from the same predicate that gates attacking.
   */
  summoning_sick: z.boolean().optional(),
  /**
   * Whether this permanent will not untap in its controller's next untap step (CR 502.4).
   *
   * Stated for the reason `summoning_sick` is: the spell that imposed it is in a graveyard
   * and the permanent's own printed text says nothing, so a tapped creature that stays
   * tapped through an untap step would otherwise be inexplicable on the board.
   */
  skips_next_untap: z.boolean().optional(),
  /**
   * The keywords this permanent has that its **printed card does not** (CR 613 layer 6) —
   * the trample an until-end-of-turn pump gave it, the flying an Aura grants.
   *
   * `card.keywords` is what it has now and `card.rules_text` is what its card printed, so
   * neither says which words are *new*; working that out by matching prose against keyword
   * names would be the client reading rules text to learn a rules fact. These arrive as the
   * words a card prints them with — "Trample" — because they are drawn as text. Absent for
   * a permanent whose abilities are all printed.
   */
  granted_keywords: z.array(z.string()).optional(),
})
export type Permanent = z.infer<typeof Permanent>

/**
 * An **emblem** (CR 114): a marker one player has, whose only characteristics are its
 * abilities. It is in no zone, is never a permanent, and nothing in the game removes it —
 * so it is carried beside the battlefield rather than inside it, and none of a
 * `Permanent`'s fields would mean anything on one.
 *
 * Public information: every seat and every spectator sees the same list. The abilities
 * arrive as server-composed rules sentences, exactly as a card's do; the client renders
 * them and derives nothing.
 */
export const Emblem = z.object({
  id: EntityId,
  controller: PlayerId,
  abilities: z.array(z.string()).optional(),
})
export type Emblem = z.infer<typeof Emblem>

export const StackItemKind = z.enum(['spell', 'ability', 'activated', 'triggered'])
export type StackItemKind = z.infer<typeof StackItemKind>

export const StackTarget = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('player'), player: PlayerId }),
  z.object({ kind: z.literal('permanent'), id: EntityId }),
  z.object({ kind: z.literal('card'), id: EntityId }),
  z.object({ kind: z.literal('stack'), id: EntityId }),
])
export type StackTarget = z.infer<typeof StackTarget>

export const StackItem = z.object({
  id: EntityId,
  controller: PlayerId,
  description: z.string(),
  source: EntityId.optional(),
  /**
   * The **physical card** (CR 108.1) being cast — see `Permanent.physical_card`, whose
   * rules are these rules.
   *
   * Absent for an ability (CR 113.3), which has no card behind it: `source` names the
   * permanent it came from, which is a different question. Deliberately not `card.id` —
   * for an ability that is the *source permanent's* face, keyed by a `perm_` id, so a
   * join on it would silently mix id spaces on exactly the entries with no card.
   */
  physical_card: EntityId.optional(),
  kind: StackItemKind.optional(),
  targets: z.array(StackTarget).optional(),
  card: CardView.optional(),
})
export type StackItem = z.infer<typeof StackItem>

export const ZonePile = z.object({
  player_id: PlayerId,
  cards: z.array(CardView),
})
export type ZonePile = z.infer<typeof ZonePile>

// ---------------------------------------------------------------------------
// Actions, prompts, targeting
// ---------------------------------------------------------------------------

export const ActionDestination = z.object({
  type: z.string(),
  id: z.string(),
  owner: PlayerId.optional(),
  label: z.string().optional(),
})
export type ActionDestination = z.infer<typeof ActionDestination>

export const ActionAck = z.object({
  submission: z.string(),
  accepted: z.boolean(),
})
export type ActionAck = z.infer<typeof ActionAck>

export const TargetRequirement = z.object({
  slot: z.string(),
  prompt: z.string(),
  /**
   * Whether this slot may be left **unanswered** — the "up to" of "put a +1/+1 counter on
   * each of up to two target creatures". Absent (and read as `false`) for every slot of
   * an ordinary targeted spell or ability, which must be filled or the submission is
   * rejected. The client omits an optional slot from its answer, or sends it empty; the
   * server accepts either.
   */
  optional: z.boolean().optional(),
  candidates: z.array(EntityId).optional(),
  /**
   * The `candidates` that answering this slot with them would **tap** — the attackers in a
   * declaration that are not vigilant (CR 508.1f, CR 702.20b).
   *
   * A declaration is assembled a creature at a time and nothing is sent until it is
   * confirmed, so the board under the player's eye is one the server has not heard about.
   * This is what a card turning as it is chosen is drawn from: the server says which
   * candidates turn, the client turns those and turns them back when they come out, and no
   * keyword is judged here. Absent for every slot whose answer taps nothing.
   */
  taps: z.array(EntityId).optional(),
  /**
   * The entity this slot is **about**, when it is about one.
   *
   * A combat declaration is several slots that all list the same candidates and
   * differ only in whose choice they are: one slot per attacker naming what *that*
   * attacker attacks (CR 508.1a), one per attacker naming what blocks it
   * (CR 509.1a). The pairing is the server's, and this publishes it — so the
   * client asks one subject at a time and draws the arrow from the right card
   * without ever parsing a slot id, which its own contract forbids.
   *
   * Absent for a slot about the action as a whole: an ordinary spell's target, the
   * `attackers` multi-select.
   */
  subject: EntityId.optional(),
})
export type TargetRequirement = z.infer<typeof TargetRequirement>

export const PromptOption = z.object({
  id: z.string(),
  label: z.string(),
  requires: z.array(z.string()).optional(),
})
export type PromptOption = z.infer<typeof PromptOption>

/**
 * One way to pay one pip: the permanent to click (`source`) and the activation to send
 * back (`id`).
 *
 * They are separate fields on purpose. A permanent that can pay a pip more than one way —
 * a dual land — appears once per way, same `source`, different `id` and `label`. That is
 * how a client knows to ask "which?" without knowing what mana is: ask when the slot being
 * filled lists this `source` more than once.
 */
export const ManaOption = z.object({
  id: z.string(),
  source: EntityId,
  label: z.string().optional(),
  /**
   * Whether spending this option **taps** its `source` — the `{T}` in `{T}: Add {G}`.
   *
   * The payment's half of `TargetRequirement.taps`: a source picked for a pip is not spent
   * until the cast is confirmed, so the client draws the turn itself, and it must be told
   * which sources turn — a mana ability that sacrifices its source taps nothing, and the
   * cost is the only thing that says so. Absent means it taps nothing.
   */
  taps: z.boolean().optional(),
})
export type ManaOption = z.infer<typeof ManaOption>

export const Prompt = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('option'),
    slot: z.string(),
    prompt: z.string(),
    options: z.array(PromptOption).optional(),
  }),
  z.object({
    kind: z.literal('select_from_zone'),
    slot: z.string(),
    prompt: z.string(),
    zone: z.string(),
    owner: PlayerId,
    /** The most ids that may be chosen — and, with `min` absent, exactly how many must be. */
    count: z.number(),
    /** The fewest that may be chosen, when that differs from `count` (scry any number, fail to find). */
    min: z.number().optional(),
    candidates: z.array(EntityId).optional(),
  }),
  z.object({
    kind: z.literal('order'),
    slot: z.string(),
    prompt: z.string(),
    items: z.array(EntityId).optional(),
  }),
  z.object({
    kind: z.literal('number'),
    slot: z.string(),
    prompt: z.string(),
    min: z.number(),
    max: z.number(),
  }),
  z.object({
    kind: z.literal('pay_mana'),
    slot: z.string(),
    prompt: z.string(),
    /** The symbol this slot pays. The still-to-pay line is the unfilled slots' pips. */
    pip: z.string(),
    candidates: z.array(ManaOption).optional(),
  }),
])
export type Prompt = z.infer<typeof Prompt>

export const ValidAction = z.object({
  id: z.string(),
  type: z.string(),
  label: z.string(),
  subject: z.array(z.string()).optional(),
  mana_ability: z.boolean().optional(),
  requirements: z.array(TargetRequirement).optional(),
  prompts: z.array(Prompt).optional(),
  destinations: z.array(ActionDestination).optional(),
  token: z.string().optional(),
})
export type ValidAction = z.infer<typeof ValidAction>

// ---------------------------------------------------------------------------
// Game log
// ---------------------------------------------------------------------------

export const LogEntity = z.object({
  id: EntityId,
  name: z.string(),
})
export type LogEntity = z.infer<typeof LogEntity>

export const LogBlock = z.object({
  blocker: LogEntity,
  attacker: LogEntity,
})
export type LogBlock = z.infer<typeof LogBlock>

export const LogDamageTarget = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('player'), player: PlayerId }),
  z.object({ kind: z.literal('permanent'), permanent: LogEntity }),
])
export type LogDamageTarget = z.infer<typeof LogDamageTarget>

export const GameLogEvent = z.discriminatedUnion('type', [
  z.object({ type: z.literal('spell_cast'), player: PlayerId, card: LogEntity }),
  z.object({ type: z.literal('spell_resolved'), player: PlayerId, card: LogEntity }),
  z.object({ type: z.literal('spell_countered'), player: PlayerId, card: LogEntity }),
  z.object({ type: z.literal('spell_fizzled'), player: PlayerId, card: LogEntity }),
  z.object({
    type: z.literal('attackers_declared'),
    player: PlayerId,
    attackers: z.array(LogEntity),
  }),
  z.object({ type: z.literal('blockers_declared'), player: PlayerId, blocks: z.array(LogBlock) }),
  z.object({ type: z.literal('mulligan'), player: PlayerId }),
  z.object({ type: z.literal('hand_kept'), player: PlayerId }),
  z.object({ type: z.literal('life_changed'), player: PlayerId, amount: z.number() }),
  z.object({ type: z.literal('damage_dealt'), target: LogDamageTarget, amount: z.number() }),
  z.object({ type: z.literal('cards_drawn'), player: PlayerId, count: z.number() }),
  z.object({ type: z.literal('cards_milled'), player: PlayerId, count: z.number() }),
  z.object({ type: z.literal('cards_discarded'), player: PlayerId, count: z.number() }),
  z.object({ type: z.literal('library_searched'), player: PlayerId }),
  z.object({ type: z.literal('optional_applied'), player: PlayerId }),
  z.object({ type: z.literal('optional_declined'), player: PlayerId }),
  z.object({ type: z.literal('permanent_died'), permanent: LogEntity }),
  z.object({
    type: z.literal('step_changed'),
    turn: z.number(),
    active_player: PlayerId,
    phase: Phase,
  }),
  z.object({ type: z.literal('player_eliminated'), player: PlayerId, reason: GameOverReason }),
  z.object({
    type: z.literal('commander_returned_to_command_zone'),
    player: PlayerId,
    card: LogEntity,
  }),
  z.object({ type: z.literal('game_over'), result: GameResult }),
])
export type GameLogEvent = z.infer<typeof GameLogEvent>

export const GameLogEntry = z.object({
  sequence: z.number(),
  event: GameLogEvent,
})
export type GameLogEntry = z.infer<typeof GameLogEntry>

// ---------------------------------------------------------------------------
// The in-game views
// ---------------------------------------------------------------------------

export const AutoPassedStep = z.object({
  phase: Phase,
  turn: z.number(),
})
export type AutoPassedStep = z.infer<typeof AutoPassedStep>

export const GameView = z.object({
  you: PlayerId.optional(),
  my_hand: z.array(CardView).optional(),
  /** Cards from a hidden zone this receiver alone is being shown, while a choice asks about them. */
  revealed: z.array(CardView).optional(),
  me: SelfView.optional(),
  opponents: z.array(OpponentView).optional(),
  battlefield: z.array(Permanent).optional(),
  /** The emblems in the game (CR 114) — public, in no zone, and never removed. */
  emblems: z.array(Emblem).optional(),
  stack: z.array(StackItem).optional(),
  graveyards: z.array(ZonePile).optional(),
  exile: z.array(ZonePile).optional(),
  command: z.array(ZonePile).optional(),
  phase: Phase,
  turn: z.number().optional(),
  active_player: PlayerId.optional(),
  seat_order: z.array(PlayerId).optional(),
  mana_pool: z.array(z.string()).optional(),
  priority_player: PlayerId.optional(),
  valid_actions: z.array(ValidAction).optional(),
  action_deadline: z.number().optional(),
  result: GameResult.optional(),
  log: z.array(GameLogEntry).optional(),
  stops: z.array(Phase).optional(),
  own_turn_stops: z.array(Phase).optional(),
  auto_passed: z.boolean().optional(),
  auto_passed_steps: z.array(AutoPassedStep).optional(),
  /**
   * Where the receiver's unattended stretch began, as a `log` sequence.
   *
   * `auto_passed_steps` says *where* the settle acted and never *what happened there*,
   * which is the half a player reads: a spell cast, resolved, and killing a creature
   * inside one settle is three log events and no step anybody recognises. Every entry in
   * `log` at or after this sequence is something the receiver was not shown.
   */
  auto_passed_from: z.number().optional(),
  action_rejected: z.boolean().optional(),
  action_ack: ActionAck.optional(),
  player_names: z.record(PlayerId, z.string()).optional(),
  commander_damage: z.array(CommanderDamage).optional(),
  commander_tax: z.array(CommanderTax).optional(),
  format: MatchFormat.optional(),
  commander_identity: z.array(CommanderIdentity).optional(),
})
export type GameView = z.infer<typeof GameView>

export const SpectatorView = z.object({
  players: z.array(OpponentView).optional(),
  battlefield: z.array(Permanent).optional(),
  /** The same public emblem list a seated view carries. */
  emblems: z.array(Emblem).optional(),
  stack: z.array(StackItem).optional(),
  graveyards: z.array(ZonePile).optional(),
  exile: z.array(ZonePile).optional(),
  command: z.array(ZonePile).optional(),
  phase: Phase,
  turn: z.number().optional(),
  active_player: PlayerId.optional(),
  seat_order: z.array(PlayerId).optional(),
  priority_player: PlayerId.optional(),
  result: GameResult.optional(),
  log: z.array(GameLogEntry).optional(),
  player_names: z.record(PlayerId, z.string()).optional(),
  commander_damage: z.array(CommanderDamage).optional(),
  commander_tax: z.array(CommanderTax).optional(),
  format: MatchFormat.optional(),
  commander_identity: z.array(CommanderIdentity).optional(),
})
export type SpectatorView = z.infer<typeof SpectatorView>

// ---------------------------------------------------------------------------
// Client → server, in game
// ---------------------------------------------------------------------------

export const TargetChoice = z.object({
  slot: z.string(),
  chosen: z.array(EntityId).optional(),
})
export type TargetChoice = z.infer<typeof TargetChoice>

export const ChooseAction = z.object({
  action_id: z.string(),
  token: z.string().optional(),
  targets: z.array(TargetChoice).optional(),
  submission: z.string().optional(),
})
export type ChooseAction = z.infer<typeof ChooseAction>

export const SetStops = z.object({
  stops: z.array(Phase).optional(),
  own_turn: z.array(Phase).optional(),
})
export type SetStops = z.infer<typeof SetStops>

export const ClientMessage = z.discriminatedUnion('type', [
  ChooseAction.extend({ type: z.literal('choose_action') }),
  SetStops.extend({ type: z.literal('set_stops') }),
])
export type ClientMessage = z.infer<typeof ClientMessage>

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

export const CATALOG_VERSION = 1

export const CatalogCard = z.object({
  functional_id: CardIdentity,
  name: z.string(),
  type_line: z.string(),
  mana_cost: z.string().optional(),
  rules_text: z.string().optional(),
  power: z.string().optional(),
  toughness: z.string().optional(),
  /** Printed starting loyalty (CR 306.5b); present only for planeswalkers. */
  loyalty: z.string().optional(),
  keywords: z.array(z.string()).optional(),
  /**
   * The same types an in-game `CardView` states. A catalog entry and a card in hand describe
   * one printed card, so a builder and a table cannot end up with different faces for it.
   */
  card_types: z.array(CardType).optional(),
  /** The same colour identity an in-game `CardView` carries (CR 903.4), WUBRG order. */
  color_identity: z.array(Color).optional(),
})
export type CatalogCard = z.infer<typeof CatalogCard>

export const CatalogFormat = z.object({
  game_setup: GameSetupId,
  min_deck_size: z.number(),
  max_deck_size: z.number().optional(),
  max_copies: z.number().optional(),
  basic_land_exempt: z.boolean(),
  requires_commander: z.boolean().optional(),
  enforce_color_identity: z.boolean().optional(),
  min_seats: z.number(),
  max_seats: z.number(),
})
export type CatalogFormat = z.infer<typeof CatalogFormat>

export const AiOption = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().optional(),
})
export type AiOption = z.infer<typeof AiOption>

export const CatalogView = z.object({
  catalog_version: z.number(),
  cards: z.array(CatalogCard).optional(),
  formats: z.array(CatalogFormat).optional(),
  ai_opponents: z.array(AiOption).optional(),
})
export type CatalogView = z.infer<typeof CatalogView>

// ---------------------------------------------------------------------------
// Lobby
// ---------------------------------------------------------------------------

export const RoomVisibility = z.enum(['public', 'private'])
export type RoomVisibility = z.infer<typeof RoomVisibility>

export const RoomConfig = z.object({
  seats: z.number(),
  game_setup: GameSetupId,
  name: z.string().optional(),
  /** Absent means `public` — the field rides the wire only when private. */
  visibility: RoomVisibility.optional(),
})
export type RoomConfig = z.infer<typeof RoomConfig>

export const SeatView = z.object({
  seat: z.number(),
  occupied_by: PlayerId.optional(),
  name: z.string().optional(),
  decked: z.boolean().optional(),
  /**
   * The colours of the deck this seat submitted (CR 903.4), WUBRG order — a summary the
   * server computed, never the list it summarises.
   */
  colors: z.array(Color).optional(),
  /**
   * The commander this seat designated (CR 903.3), by `functional_id`. Public for the same
   * reason the card is: it begins the game face up in the command zone.
   */
  commander: CardIdentity.optional(),
  ready: z.boolean().optional(),
  ai: z.string().optional(),
})
export type SeatView = z.infer<typeof SeatView>

export const RoomView = z.object({
  room_id: RoomId,
  config: RoomConfig,
  seats: z.array(SeatView).optional(),
})
export type RoomView = z.infer<typeof RoomView>

export const RoomState = z.enum(['gathering', 'in_progress'])
export type RoomState = z.infer<typeof RoomState>

export const RoomSummary = z.object({
  room_id: RoomId,
  config: RoomConfig,
  filled: z.number(),
  spectators: z.number().optional(),
  state: RoomState,
})
export type RoomSummary = z.infer<typeof RoomSummary>

export const LobbyView = z.object({
  session: SessionToken.optional(),
  you: PlayerId.optional(),
  name: z.string().optional(),
  room: RoomView.optional(),
  directory: z.array(RoomSummary).optional(),
  valid_commands: z.array(z.string()).optional(),
})
export type LobbyView = z.infer<typeof LobbyView>

export const LobbyRejection = z.object({
  code: z.string(),
  reason: z.string(),
  card: CardIdentity.optional(),
})
export type LobbyRejection = z.infer<typeof LobbyRejection>

export const LobbyErrorFrame = z.object({
  lobby_error: LobbyRejection,
})
export type LobbyErrorFrame = z.infer<typeof LobbyErrorFrame>

export const LobbyCommand = z.discriminatedUnion('type', [
  z.object({ type: z.literal('hello'), token: SessionToken.optional() }),
  z.object({ type: z.literal('create_room'), config: RoomConfig }),
  z.object({ type: z.literal('update_room'), config: RoomConfig }),
  z.object({ type: z.literal('join_room'), room_id: RoomId }),
  z.object({
    type: z.literal('submit_deck'),
    cards: z.array(CardIdentity).optional(),
    commander: CardIdentity.optional(),
  }),
  z.object({
    type: z.literal('add_ai'),
    seat: z.number(),
    kind: z.string(),
    cards: z.array(CardIdentity).optional(),
    commander: CardIdentity.optional(),
  }),
  z.object({ type: z.literal('remove_ai'), seat: z.number() }),
  z.object({ type: z.literal('ready'), ready: z.boolean() }),
  z.object({ type: z.literal('set_name'), name: z.string() }),
  z.object({ type: z.literal('spectate_room'), room_id: RoomId }),
  z.object({ type: z.literal('request_catalog') }),
  z.object({ type: z.literal('leave') }),
])
export type LobbyCommand = z.infer<typeof LobbyCommand>
