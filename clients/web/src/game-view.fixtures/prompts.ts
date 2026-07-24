/**
 * Decision/prompt wire frames: the structured `prompts` (option / select_from_zone /
 * order) and `requirements` shapes the banner and prompt-surface drive from — mulligan
 * bottoming, standalone bottoming, modal option, trigger ordering, non-board zone
 * selection, and cleanup discard. All are raw wire JSON.
 */

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
