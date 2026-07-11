/**
 * A representative server→client `GameView` frame, mirroring the round-trip
 * fixture in `crates/rune-protocol/src/lib.rs`. Written as raw wire JSON — note
 * that empty collections (`exile`) are omitted, exactly as the server elides
 * them, so tests exercise the client's normalization of missing fields.
 */
import type { GameView } from './protocol';

/** The wire text a client would receive over the socket. */
export const SAMPLE_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [
    {
      id: 'c1',
      name: 'Llanowar Elves',
      type_line: 'Creature — Elf Druid',
      mana_cost: '{G}',
      oracle_text: '{T}: Add {G}.',
      power: '1',
      toughness: '1',
    },
  ],
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
  mana_pool: ['{G}'],
  priority_player: 'p1',
  valid_actions: [
    { id: 'a2', type: 'activate_ability', label: 'Tap for mana', subject: ['perm_xyz'] },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
  action_deadline: 12.5,
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
      oracle_text: '{T}: Add {G}.',
      power: '1',
      toughness: '1',
    },
  ],
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
  mana_pool: ['{G}'],
  priority_player: 'p1',
  valid_actions: [
    { id: 'a2', type: 'activate_ability', label: 'Tap for mana', subject: ['perm_xyz'] },
    { id: 'a1', type: 'pass_priority', label: 'Pass' },
  ],
  action_deadline: 12.5,
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
      oracle_text: 'Lightning Bolt deals 3 damage to any target.',
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
