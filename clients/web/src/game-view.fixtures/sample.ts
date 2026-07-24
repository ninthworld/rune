/**
 * A representative server→client `GameView` frame, mirroring the round-trip
 * fixture in `crates/rune-protocol/src/lib.rs`. Written as raw wire JSON — note
 * that empty collections (`exile`) are omitted, exactly as the server elides
 * them, so tests exercise the client's normalization of missing fields.
 */
import type { GameLogEntry, GameView } from '../protocol';

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
  command: [], // no command zone in the sample frame; normalization defaults it (issue #372)
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
  // No commander damage in the sample frame; normalization defaults it to `[]` (issue #371).
  commander_damage: [],
  // No commander tax in the sample frame; normalization defaults it to `[]` (issue #372).
  commander_tax: [],
};
