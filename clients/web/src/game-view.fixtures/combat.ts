/**
 * Combat and target-declaration wire frames: single-target casting, attacker and
 * blocker declaration (two-player and multiplayer), mid-combat render state, and the
 * four-player free-for-all split-attack scene. All are raw wire JSON exercising the
 * client's reconstruction of combat from a single `GameView`.
 */

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
 * A multiplayer declare-attackers frame (issue #341/#347): `p1` declaring attackers
 * in a three-player game. The subject-less `declare_attackers` action carries the
 * `attackers` subset slot PLUS one `defend_<permId>` slot per attacker candidate,
 * each listing the defending players that attacker may be assigned to (server
 * `attacker_requirements`, offered only with more than one opponent). The client
 * walks the attacker pick, then — for each declared attacker — a single defending-
 * player pick from that opponent's HUD tile, and submits one atomic answer.
 */
export const DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  me: { life: 20, library_size: 40 },
  opponents: [
    { player_id: 'p2', hand_size: 3, life: 20, library_size: 40, graveyard_size: 0 },
    { player_id: 'p3', hand_size: 2, life: 15, library_size: 39, graveyard_size: 1 },
  ],
  battlefield: [
    {
      id: 'perm_1',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'perm_1',
        name: 'Charging Rhino',
        type_line: 'Creature — Rhino',
        power: '4',
        toughness: '4',
      },
    },
    {
      id: 'perm_2',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'perm_2',
        name: 'Skyshroud Falcon',
        type_line: 'Creature — Bird',
        power: '2',
        toughness: '1',
        keywords: ['flying'],
      },
    },
  ],
  phase: 'declare_attackers',
  turn: 7,
  active_player: 'p1',
  seat_order: ['p1', 'p2', 'p3'],
  priority_player: 'p1',
  valid_actions: [
    {
      id: 'a5',
      type: 'declare_attackers',
      label: 'Declare attackers',
      token: 'h:atk0',
      requirements: [
        { slot: 'attackers', prompt: 'Choose attackers', candidates: ['perm_1', 'perm_2'] },
        {
          slot: 'defend_1',
          prompt: 'Choose whom Charging Rhino attacks',
          candidates: ['p2', 'p3'],
        },
        {
          slot: 'defend_2',
          prompt: 'Choose whom Skyshroud Falcon attacks',
          candidates: ['p2', 'p3'],
        },
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
 * A four-player free-for-all mid-combat frame (issue #348, built on the #345
 * multiplayer shapes). `p1` is the receiver; the table seats `p1..p4` in
 * `seat_order`. `p1` (the active player) has declared a **split attack** — one
 * attacker at `p2`, one at `p4` (`attacking_player`) — so the fixture exercises
 * combat treatments and links spanning more than one opponent area. `p3` has been
 * **eliminated** (CR 800.4a, issue #342): it is still in `seat_order` and carries
 * the `eliminated` flag, but its permanents have left the game, so its band is
 * empty. Every seat has public zone piles (graveyard sizes) so each band's pile
 * counts render. This is the shared fixture the multiplayer scene and layout tests
 * build on; it is a render fixture (no interaction slots) — the declaration UI has
 * its own fixtures (issue #347).
 */
export const FOUR_PLAYER_GAME_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [{ id: 'hand_1', name: 'Forest', type_line: 'Basic Land — Forest' }],
  me: { life: 18, library_size: 33 },
  opponents: [
    { player_id: 'p2', hand_size: 3, life: 14, library_size: 31, graveyard_size: 2 },
    {
      player_id: 'p3',
      hand_size: 0,
      life: 0,
      library_size: 0,
      graveyard_size: 3,
      eliminated: true,
    },
    { player_id: 'p4', hand_size: 5, life: 20, library_size: 34, graveyard_size: 1 },
  ],
  graveyards: [
    {
      player_id: 'p2',
      cards: [
        { id: 'p2_gy_0', name: 'Shock', type_line: 'Instant' },
        { id: 'p2_gy_1', name: 'Goblin', type_line: 'Creature — Goblin' },
      ],
    },
    {
      player_id: 'p3',
      cards: [
        { id: 'p3_gy_0', name: 'Bear', type_line: 'Creature — Bear' },
        { id: 'p3_gy_1', name: 'Forest', type_line: 'Basic Land — Forest' },
        { id: 'p3_gy_2', name: 'Elf', type_line: 'Creature — Elf' },
      ],
    },
    {
      player_id: 'p4',
      cards: [{ id: 'p4_gy_0', name: 'Island', type_line: 'Basic Land — Island' }],
    },
  ],
  battlefield: [
    // p1 (local): a split attack — one attacker at p2, one at p4 — plus a land.
    {
      id: 'p1_atk_a',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'p1_atk_a',
        name: 'Charging Rhino',
        type_line: 'Creature — Rhino',
        power: '4',
        toughness: '4',
      },
      tapped: true,
      attacking: true,
      attacking_player: 'p2',
    },
    {
      id: 'p1_atk_b',
      controller: 'p1',
      owner: 'p1',
      card: {
        id: 'p1_atk_b',
        name: 'Skyshroud Falcon',
        type_line: 'Creature — Bird',
        power: '2',
        toughness: '1',
        keywords: ['flying'],
      },
      tapped: true,
      attacking: true,
      attacking_player: 'p4',
    },
    {
      id: 'p1_land',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'p1_land', name: 'Forest', type_line: 'Basic Land — Forest' },
      tapped: true,
    },
    // p2: a blocker facing p1's rhino, plus a land.
    {
      id: 'p2_blk',
      controller: 'p2',
      owner: 'p2',
      card: {
        id: 'p2_blk',
        name: 'Stone Golem',
        type_line: 'Artifact Creature — Golem',
        power: '3',
        toughness: '4',
      },
      blocking: 'p1_atk_a',
    },
    {
      id: 'p2_land',
      controller: 'p2',
      owner: 'p2',
      card: { id: 'p2_land', name: 'Mountain', type_line: 'Basic Land — Mountain' },
    },
    // p3 is eliminated: no permanents remain (they left the game, CR 800.4a).
    // p4: an untapped creature (not yet blocking) and a land.
    {
      id: 'p4_crt',
      controller: 'p4',
      owner: 'p4',
      card: {
        id: 'p4_crt',
        name: 'Air Elemental',
        type_line: 'Creature — Elemental',
        power: '4',
        toughness: '4',
        keywords: ['flying'],
      },
    },
    {
      id: 'p4_land',
      controller: 'p4',
      owner: 'p4',
      card: { id: 'p4_land', name: 'Island', type_line: 'Basic Land — Island' },
    },
  ],
  phase: 'declare_blockers',
  turn: 9,
  active_player: 'p1',
  seat_order: ['p1', 'p2', 'p3', 'p4'],
  priority_player: 'p2',
  valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }],
});
