/**
 * A representative server→client `GameView` frame, mirroring the round-trip
 * fixture in `crates/rune-protocol/src/lib.rs`. Written as raw wire JSON — note
 * that empty collections (`exile`) are omitted, exactly as the server elides
 * them, so tests exercise the client's normalization of missing fields.
 */
import type { GameView } from './protocol';

/** The wire text a client would receive over the socket. */
export const SAMPLE_GAME_VIEW_JSON = JSON.stringify({
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
