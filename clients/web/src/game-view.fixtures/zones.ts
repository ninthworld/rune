/**
 * Public-zone and commander-format wire frames: populated graveyard/exile piles for
 * the zone browsers, and the command zone with recast tax and commander-damage tally.
 * Both are raw wire JSON — the client browses whatever the view carries, deriving
 * nothing.
 */

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
 * A commander-format frame (issue #372): the receiver `p1` holds their commander in
 * the command zone; the opponent `p2` has recast theirs once (so it carries a `{2}`
 * tax and its command-zone entry is momentarily empty) and has dealt `p1` 7 combat
 * damage with it. Exercises the command-zone piles, the recast-tax caption, and the
 * `amount/21` commander-damage tally — all public, straight from the view.
 */
export const COMMANDER_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  me: { life: 34, library_size: 90 },
  opponents: [{ player_id: 'p2', hand_size: 4, life: 40, library_size: 88, graveyard_size: 0 }],
  battlefield: [],
  phase: 'precombat_main',
  turn: 6,
  active_player: 'p1',
  seat_order: ['p1', 'p2'],
  priority_player: 'p1',
  command: [
    {
      player_id: 'p1',
      cards: [
        { id: 'cmd_p1', name: 'Jedit Ojanen', type_line: 'Legendary Creature — Cat Warrior' },
      ],
    },
    // p2's commander is on the battlefield now, so its command-zone entry is empty —
    // the pile still shows (count 0) because a tax is owed on the next recast.
    { player_id: 'p2', cards: [] },
  ],
  commander_tax: [{ commander: 'p2', casts: 1, tax: 2 }],
  commander_damage: [{ commander: 'p2', damaged: 'p1', amount: 7 }],
  valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }],
});
