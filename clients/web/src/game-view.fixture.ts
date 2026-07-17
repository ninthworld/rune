/**
 * A representative server→client `GameView` frame, mirroring the round-trip
 * fixture in `crates/rune-protocol/src/lib.rs`. Written as raw wire JSON — note
 * that empty collections (`exile`) are omitted, exactly as the server elides
 * them, so tests exercise the client's normalization of missing fields.
 */
import type { GameLogEntry, GameView } from './protocol';

/**
 * A representative slice of the structured game-history window (issue #259 / #260),
 * shared between the wire JSON and the normalized object below so the two can never
 * drift. It exercises the log panel's shapes: a collapsible run of consecutive step
 * changes, a lone step change, a draw, a spell cast, and damage to a permanent — with
 * references to players (`p1`/`p2`) and a live battlefield permanent (`perm_xyz`) that
 * the panel makes clickable for presentational highlighting.
 */
const SAMPLE_LOG: GameLogEntry[] = [
  { sequence: 30, event: { type: 'step_changed', turn: 4, active_player: 'p2', phase: 'end' } },
  { sequence: 31, event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'untap' } },
  { sequence: 32, event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'draw' } },
  { sequence: 33, event: { type: 'cards_drawn', player: 'p1', count: 1 } },
  {
    sequence: 34,
    event: { type: 'step_changed', turn: 5, active_player: 'p1', phase: 'precombat_main' },
  },
  {
    sequence: 35,
    event: { type: 'spell_cast', player: 'p2', card: { id: 's1', name: 'Lightning Bolt' } },
  },
  {
    sequence: 36,
    event: {
      type: 'damage_dealt',
      target: { kind: 'permanent', permanent: { id: 'perm_xyz', name: 'Grizzly Bears' } },
      amount: 3,
    },
  },
];

/** The wire text a client would receive over the socket. */
export const SAMPLE_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    {
      id: 'c1',
      name: 'Llanowar Elves',
      type_line: 'Creature — Elf Druid',
      mana_cost: '{G}',
      rules_text: '{T}: Add {G}.',
      functional_id: 'llanowar_elves',
      power: '1',
      toughness: '1',
    },
  ],
  me: { life: 18, library_size: 52 },
  opponents: [
    {
      player_id: 'p2',
      hand_size: 7,
      life: 20,
      library_size: 53,
      graveyard_size: 0,
      statuses: ['monarch'],
    },
  ],
  battlefield: [
    {
      id: 'perm_xyz',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'perm_xyz',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        mana_cost: '{1}{G}',
        power: '2',
        toughness: '2',
      },
      tapped: true,
      counters: [{ kind: '+1/+1', count: 2 }],
    },
  ],
  stack: [{ id: 's1', controller: 'p2', description: 'Lightning Bolt' }],
  graveyards: [{ player_id: 'p1', cards: [] }],
  // `exile` intentionally omitted — the server elides empty collections.
  phase: 'precombat_main',
  turn: 5,
  active_player: 'p1',
  seat_order: ['p1', 'p2'],
  mana_pool: ['{G}'],
  priority_player: 'p1',
  valid_actions: [
    { id: 'a2', type: 'activate_ability', label: 'Tap for mana', subject: ['perm_xyz'] },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
  action_deadline: 12.5,
  log: SAMPLE_LOG,
});

/** The fully-normalized {@link GameView} the client should hold after parsing. */
export const SAMPLE_GAME_VIEW: GameView = {
  you: 'p1',
  my_hand: [
    {
      id: 'c1',
      name: 'Llanowar Elves',
      type_line: 'Creature — Elf Druid',
      mana_cost: '{G}',
      rules_text: '{T}: Add {G}.',
      functional_id: 'llanowar_elves',
      power: '1',
      toughness: '1',
    },
  ],
  me: { life: 18, library_size: 52 },
  opponents: [
    {
      player_id: 'p2',
      hand_size: 7,
      life: 20,
      library_size: 53,
      graveyard_size: 0,
      statuses: ['monarch'],
    },
  ],
  battlefield: [
    {
      id: 'perm_xyz',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'perm_xyz',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        mana_cost: '{1}{G}',
        power: '2',
        toughness: '2',
      },
      tapped: true,
      counters: [{ kind: '+1/+1', count: 2 }],
    },
  ],
  stack: [{ id: 's1', controller: 'p2', description: 'Lightning Bolt' }],
  graveyards: [{ player_id: 'p1', cards: [] }],
  exile: [], // filled in by normalization from the omitted wire field
  phase: 'precombat_main',
  turn: 5,
  active_player: 'p1',
  seat_order: ['p1', 'p2'],
  mana_pool: ['{G}'],
  priority_player: 'p1',
  valid_actions: [
    { id: 'a2', type: 'activate_ability', label: 'Tap for mana', subject: ['perm_xyz'] },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
  action_deadline: 12.5,
  // The structured game-history window (issue #259 / #260); an older frame may omit it
  // and normalization defaults it to empty.
  log: SAMPLE_LOG,
  result: undefined,
  // No stops or auto-pass in the sample frame; normalization defaults them (issue #264).
  stops: [],
  auto_passed: false,
  // Not a rejection re-send; normalization defaults action_rejected to `false` (issue #265).
  action_rejected: false,
  // No names in the sample wire frame; normalization defaults player_names to `{}`.
  player_names: {},
};

/**
 * A wire frame in which the receiver can cast a targeted spell (issue #74 / ADR
 * 0009). The hand's Lightning Bolt (`c3`) carries a `cast_spell` action with a
 * single target requirement whose server-enumerated candidates are one permanent
 * (`perm_xyz`) and the opponent player (`p2`) — exercising both the on-card and
 * the player-portrait target affordances. The action carries a content-binding
 * `token` the client must echo verbatim. A global `Pass` (`a1`) is also offered.
 */
export const TARGETING_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    {
      id: 'c3',
      name: 'Lightning Bolt',
      type_line: 'Instant',
      mana_cost: '{R}',
      rules_text: 'Lightning Bolt deals 3 damage to any target.',
      functional_id: 'lightning_bolt',
    },
  ],
  opponents: [{ player_id: 'p2', hand_size: 5, life: 20, library_size: 50, graveyard_size: 0 }],
  battlefield: [
    {
      id: 'perm_xyz',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'perm_xyz',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        mana_cost: '{1}{G}',
        power: '2',
        toughness: '2',
      },
    },
  ],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a3',
      type: 'cast_spell',
      label: 'Cast Lightning Bolt',
      subject: ['c3'],
      token: 'h:9f2c',
      requirements: [
        { slot: 't0', prompt: 'target creature or player', candidates: ['perm_xyz', 'p2'] },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame in the declare-attackers step (issue #143 / protocol.md, CR 508.1a):
 * a subject-less `declare_attackers` action carrying ONE multi-select `requirements`
 * slot (`"attackers"`) whose server-enumerated candidates are the receiver's two
 * eligible creatures. The answer is any subset of those ids (optional — an empty set
 * declares no attackers). The action carries a content-binding `token`. A global
 * `Pass` (`a1`) is also offered.
 */
export const DECLARE_ATTACKERS_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 3, life: 20, library_size: 40, graveyard_size: 0 }],
  battlefield: [
    {
      id: 'atk_1',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'atk_1',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        mana_cost: '{1}{G}',
        power: '2',
        toughness: '2',
      },
    },
    {
      id: 'atk_2',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'atk_2',
        name: 'Craw Wurm',
        type_line: 'Creature — Wurm',
        mana_cost: '{4}{G}{G}',
        power: '6',
        toughness: '4',
      },
    },
  ],
  phase: 'declare_attackers',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a5',
      type: 'declare_attackers',
      label: 'Declare attackers',
      token: 'h:atk0',
      requirements: [
        { slot: 'attackers', prompt: 'Choose attackers', candidates: ['atk_1', 'atk_2'] },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame in the declare-blockers step (issue #143 / protocol.md, CR 509.1a):
 * a subject-less `declare_blockers` action with ONE `requirements` slot **per
 * declared attacker** (`"block_<id>"`), each listing the defender's eligible
 * blockers to assign to that attacker — the per-attacker two-level pick. Here the
 * first attacker can be blocked by either of the receiver's creatures and the
 * second only by one, so the two slots have different candidate sets.
 */
export const DECLARE_BLOCKERS_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 2, life: 20, library_size: 38, graveyard_size: 0 }],
  battlefield: [
    {
      id: 'atk_1',
      controller: 'p2',
      owner: 'p2',
      card: {
        id: 'atk_1',
        name: 'Verdant Scout',
        type_line: 'Creature — Elf Scout',
        power: '2',
        toughness: '2',
      },
    },
    {
      id: 'atk_2',
      controller: 'p2',
      owner: 'p2',
      card: {
        id: 'atk_2',
        name: 'Hill Giant',
        type_line: 'Creature — Giant',
        power: '3',
        toughness: '3',
      },
    },
    {
      id: 'blk_1',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'blk_1',
        name: 'Wall of Wood',
        type_line: 'Creature — Wall',
        power: '0',
        toughness: '3',
      },
    },
    {
      id: 'blk_2',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'blk_2',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        power: '2',
        toughness: '2',
      },
    },
  ],
  phase: 'declare_blockers',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a6',
      type: 'declare_blockers',
      label: 'Declare blockers',
      token: 'h:blk0',
      requirements: [
        {
          slot: 'block_atk_1',
          prompt: 'Choose blockers for Verdant Scout',
          candidates: ['blk_1', 'blk_2'],
        },
        { slot: 'block_atk_2', prompt: 'Choose blockers for Hill Giant', candidates: ['blk_1'] },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame mid-combat (issue #332, CR 508/509/510), after attackers and blockers
 * have been declared and combat damage marked — the state a client must reconstruct
 * from one `GameView`, whether it watched declaration or mounted just now. The
 * opponent (`p2`) is attacking with two creatures: `atk_1` is blocked by one of the
 * receiver's creatures (`blk_1`) and has taken 2 marked damage; `atk_2` is blocked by
 * two (`blk_2`, `blk_3`). Attackers are tapped (they attacked); blockers name the
 * attacker each blocks via `blocking`. No `valid_actions` beyond a pass — the point is
 * the rendered combat state, not interactivity.
 */
export const COMBAT_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  me: { life: 20, library_size: 40 },
  opponents: [{ player_id: 'p2', hand_size: 2, life: 20, library_size: 38, graveyard_size: 0 }],
  battlefield: [
    {
      id: 'atk_1',
      controller: 'p2',
      owner: 'p2',
      card: {
        id: 'atk_1',
        name: 'Hill Giant',
        type_line: 'Creature — Giant',
        power: '3',
        toughness: '3',
      },
      tapped: true,
      attacking: true,
      damage: 2,
    },
    {
      id: 'atk_2',
      controller: 'p2',
      owner: 'p2',
      card: {
        id: 'atk_2',
        name: 'Craw Wurm',
        type_line: 'Creature — Wurm',
        power: '6',
        toughness: '4',
      },
      tapped: true,
      attacking: true,
    },
    {
      id: 'blk_1',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'blk_1',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        power: '2',
        toughness: '2',
      },
      blocking: 'atk_1',
    },
    {
      id: 'blk_2',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'blk_2',
        name: 'Wall of Wood',
        type_line: 'Creature — Wall',
        power: '0',
        toughness: '3',
      },
      blocking: 'atk_2',
    },
    {
      id: 'blk_3',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'blk_3',
        name: 'Elvish Warrior',
        type_line: 'Creature — Elf Warrior',
        power: '2',
        toughness: '3',
      },
      blocking: 'atk_2',
    },
  ],
  phase: 'combat_damage',
  turn: 6,
  active_player: 'p2',
  seat_order: ['p1', 'p2'],
  priority_player: 'p1',
  valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }],
});

/**
 * A wire frame owing mulligan bottoming (issue #143/#156, CR 103.5 London): the
 * subject-less `mulligan_decision` action carries an `option` prompt (keep /
 * take-another) AND a `select_from_zone` bottoming prompt (`count: 1`) over the
 * receiver's hand. The client renders the option minimally as a submit trigger
 * (rich option UX is #157) and enforces the bottoming `count` client-side only as a
 * UX affordance — the option buttons are blocked while the bottom pick is partial.
 */
export const MULLIGAN_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    { id: 'card_a', name: 'Forest', type_line: 'Basic Land — Forest' },
    {
      id: 'card_b',
      name: 'Llanowar Elves',
      type_line: 'Creature — Elf Druid',
      mana_cost: '{G}',
      power: '1',
      toughness: '1',
    },
  ],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a0',
      type: 'mulligan_decision',
      label: 'Keep or mulligan',
      token: 'h:mull',
      prompts: [
        {
          kind: 'option',
          slot: 'decision',
          prompt: 'Keep this hand or take a mulligan?',
          options: [
            { id: 'keep', label: 'Keep this hand' },
            { id: 'mulligan', label: 'Mulligan' },
          ],
        },
        {
          kind: 'select_from_zone',
          slot: 'bottom',
          prompt: 'Put 1 card(s) on the bottom of your library',
          zone: 'hand',
          owner: 'p1',
          count: 1,
          candidates: ['card_a', 'card_b'],
        },
      ],
    },
  ],
});

/**
 * A wire frame posing a standalone `select_from_zone` bottoming choice (issue #143):
 * a `keep` action carrying only a `select_from_zone` prompt with `count: 2` over a
 * three-card hand, with no `option`. This drives the count-gated **Confirm** control
 * (disabled until exactly two cards are picked), the cleanest form of the
 * "enforce the advertised count as UX only" acceptance criterion.
 */
export const BOTTOM_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    { id: 'card_a', name: 'Forest', type_line: 'Basic Land — Forest' },
    { id: 'card_b', name: 'Mountain', type_line: 'Basic Land — Mountain' },
    { id: 'card_c', name: 'Shock', type_line: 'Instant', mana_cost: '{R}' },
  ],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a7',
      type: 'keep',
      label: 'Keep hand',
      token: 'h:keep',
      prompts: [
        {
          kind: 'select_from_zone',
          slot: 'bottom',
          prompt: 'Put 2 card(s) on the bottom of your library',
          zone: 'hand',
          owner: 'p1',
          count: 2,
          candidates: ['card_a', 'card_b', 'card_c'],
        },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame posing a standalone `option` decision (issue #157): a subject-less
 * `choose_mode` action carrying ONLY an `option` prompt (two named modal choices)
 * and no selection/order slot. It drives the banner's modal option picker — the
 * client renders exactly the two choices and answers with the chosen option id in
 * the decision slot. The action carries a content-binding `token`.
 */
export const OPTION_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a8',
      type: 'choose_mode',
      label: 'Fork in the Road',
      token: 'h:mode',
      prompts: [
        {
          kind: 'option',
          slot: 'mode',
          prompt: 'Choose a mode',
          options: [
            { id: 'draw', label: 'Draw a card' },
            { id: 'gain', label: 'Gain 3 life' },
          ],
        },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame posing an `order` arrange decision (issue #157, e.g. ordering
 * simultaneous triggers / scry): a subject-less `order_triggers` action carrying
 * one `order` prompt over three items. The items are cards in the receiver's
 * graveyard so the client can label each row by name; the answer is all three items
 * in the chosen order (a permutation). The action carries a content-binding `token`.
 */
export const ORDER_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  graveyards: [
    {
      player_id: 'p1',
      cards: [
        { id: 'trig_a', name: 'Soul Warden', type_line: 'Creature — Human Cleric' },
        { id: 'trig_b', name: 'Ajani’s Welcome', type_line: 'Enchantment' },
        { id: 'trig_c', name: 'Impassioned Orator', type_line: 'Creature — Human Cleric' },
      ],
    },
  ],
  phase: 'upkeep',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a9',
      type: 'order_triggers',
      label: 'Order triggers',
      token: 'h:ord0',
      prompts: [
        {
          kind: 'order',
          slot: 'order',
          prompt: 'Order these triggered abilities',
          items: ['trig_a', 'trig_b', 'trig_c'],
        },
      ],
    },
  ],
});

/**
 * A wire frame posing a `select_from_zone` over a zone that is NOT on the board
 * (issue #157): a `regrowth` action returning one card from the receiver's
 * graveyard. Because the graveyard is not laid out as canvas cards, the client
 * surfaces the candidates in the DOM prompt-surface overlay list (not in-place
 * highlighting). `count: 1` drives the count-gated Confirm; token bound as usual.
 */
export const ZONE_SELECT_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  graveyards: [
    {
      player_id: 'p1',
      cards: [
        { id: 'gy_1', name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' },
        { id: 'gy_2', name: 'Giant Growth', type_line: 'Instant' },
        { id: 'gy_3', name: 'Forest', type_line: 'Basic Land — Forest' },
      ],
    },
  ],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a10',
      type: 'regrowth',
      label: 'Return a card to hand',
      token: 'h:gy0',
      prompts: [
        {
          kind: 'select_from_zone',
          slot: 'return',
          prompt: 'Return 1 card from your graveyard to your hand',
          zone: 'graveyard',
          owner: 'p1',
          count: 1,
          candidates: ['gy_1', 'gy_2', 'gy_3'],
        },
      ],
    },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
});

/**
 * A wire frame for the cleanup discard-to-max flow (issue #156/#157, CR 514.1): the
 * receiver ended the turn holding 8 cards, so the server projects the discard as a
 * subject-less `discard` action with a `select_from_zone` prompt over the hand,
 * `count: 1` (8 → 7). The hand IS on the board, so the client highlights candidates
 * in place (canvas), count-gates Confirm, and submits the discarded id atomically.
 */
export const DISCARD_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    { id: 'h1', name: 'Forest', type_line: 'Basic Land — Forest' },
    { id: 'h2', name: 'Mountain', type_line: 'Basic Land — Mountain' },
    { id: 'h3', name: 'Island', type_line: 'Basic Land — Island' },
    { id: 'h4', name: 'Plains', type_line: 'Basic Land — Plains' },
    { id: 'h5', name: 'Swamp', type_line: 'Basic Land — Swamp' },
    { id: 'h6', name: 'Shock', type_line: 'Instant', mana_cost: '{R}' },
    { id: 'h7', name: 'Opt', type_line: 'Instant', mana_cost: '{U}' },
    { id: 'h8', name: 'Duress', type_line: 'Sorcery', mana_cost: '{B}' },
  ],
  opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
  battlefield: [],
  phase: 'cleanup',
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a11',
      type: 'discard',
      label: 'Discard to hand size',
      token: 'h:disc',
      prompts: [
        {
          kind: 'select_from_zone',
          slot: 'discard',
          prompt: 'Discard 1 card(s)',
          zone: 'hand',
          owner: 'p1',
          count: 1,
          candidates: ['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'h7', 'h8'],
        },
      ],
    },
  ],
});

/**
 * A wire frame carrying populated public zones (issue #262): the receiver's
 * graveyard holds two cards and their exile one; the opponent's graveyard holds one.
 * Used to exercise the graveyard/exile browsers opened from the player tiles — the
 * client browses whatever the view carries, deriving nothing.
 */
export const ZONES_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  me: { life: 20, library_size: 40 },
  opponents: [{ player_id: 'p2', hand_size: 3, life: 20, library_size: 40, graveyard_size: 1 }],
  battlefield: [],
  stack: [],
  graveyards: [
    {
      player_id: 'p1',
      cards: [
        { id: 'gy_p1_a', name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' },
        { id: 'gy_p1_b', name: 'Giant Growth', type_line: 'Instant', rules_text: '+3/+3.' },
      ],
    },
    {
      player_id: 'p2',
      cards: [{ id: 'gy_p2_a', name: 'Lightning Bolt', type_line: 'Instant' }],
    },
  ],
  exile: [
    {
      player_id: 'p1',
      cards: [{ id: 'ex_p1_a', name: 'Forest', type_line: 'Basic Land — Forest' }],
    },
  ],
  phase: 'precombat_main',
  priority_player: 'p1',
  valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }],
});

/**
 * Build a terminal server→client `GameView` frame (issue #141): the game is over,
 * so `result` is present and `valid_actions` is empty (CR 104.2a). The `you` seat
 * lets the client phrase the verdict from the receiver's perspective. Mirrors the
 * wire shape the server elides while live — `result` present is the game-over signal.
 */
export function gameOverViewJson(
  you: string,
  result: { winner?: string; losers: string[]; reason: 'life_zero' | 'decked' | 'concede' },
): string {
  return JSON.stringify({
    you,
    opponents: [{ player_id: you === 'p1' ? 'p2' : 'p1', hand_size: 3, life: 0, library_size: 40 }],
    phase: 'end',
    valid_actions: [],
    result,
  });
}

/** Terminal view where the receiver (`p1`) won by their opponent decking out. */
export const GAME_OVER_WIN_JSON = gameOverViewJson('p1', {
  winner: 'p1',
  losers: ['p2'],
  reason: 'decked',
});

/** Terminal view where the receiver (`p1`) lost to lethal damage (opponent won). */
export const GAME_OVER_LOSS_JSON = gameOverViewJson('p1', {
  winner: 'p2',
  losers: ['p1'],
  reason: 'life_zero',
});

/** Terminal view of a draw — no winner, every remaining player lost at once. */
export const GAME_OVER_DRAW_JSON = gameOverViewJson('p1', {
  losers: ['p1', 'p2'],
  reason: 'life_zero',
});
