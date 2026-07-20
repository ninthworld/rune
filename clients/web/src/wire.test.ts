import { describe, expect, it } from 'vitest';
import {
  normalizeCatalogView,
  normalizeGameView,
  normalizeLobbyView,
  normalizeSpectatorView,
  parseGameView,
  parseServerFrame,
  ProtocolError,
} from './wire';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';
import {
  LOBBY_DIRECTORY_JSON,
  LOBBY_ROOM_UNDECKED_JSON,
  LOBBY_ROOMLESS_JSON,
} from './lobby-view.fixture';
// The single canonical cross-language fixture, owned by the `rune-protocol` crate
// and round-tripped by its Rust test. Resolved via a path alias (tsconfig.json +
// vitest.config.ts) so there is exactly one copy of the JSON in the repo.
import CONTRACT_FIXTURE from '@protocol-fixtures/gameview.json';
// The terminal counterpart of the canonical fixture: a finished game carrying a
// `result` and no `valid_actions` (issue #141). Same dual-consumed shape as above.
import CONTRACT_FIXTURE_OVER from '@protocol-fixtures/gameview-over.json';
// The prompt-shapes fixture (issue #156): a mulligan frame carrying `option` +
// `select_from_zone` prompts, round-tripped by the Rust crate and asserted here.
import CONTRACT_FIXTURE_PROMPTS from '@protocol-fixtures/gameview-prompts.json';

describe('parseGameView', () => {
  it('decodes a representative wire frame into the expected GameView', () => {
    expect(parseGameView(SAMPLE_GAME_VIEW_JSON)).toEqual(SAMPLE_GAME_VIEW);
  });

  it('treats omitted collections as their empty default', () => {
    // Only `phase` on the wire — every collection must default to [] and the
    // absent `you` (older server) must normalize to '' rather than crashing.
    const view = parseGameView('{"phase":"upkeep"}');
    expect(view).toEqual({
      you: '',
      my_hand: [],
      me: { life: 0, library_size: 0 },
      opponents: [],
      battlefield: [],
      stack: [],
      graveyards: [],
      exile: [],
      phase: 'upkeep',
      turn: 0,
      active_player: '',
      seat_order: [],
      mana_pool: [],
      priority_player: undefined,
      valid_actions: [],
      action_deadline: undefined,
      result: undefined,
      log: [],
      stops: [],
      auto_passed: false,
      action_rejected: false,
      player_names: {},
      commander_damage: [],
    });
  });

  it('normalizes the public commander-damage tally, dropping malformed entries (issue #371)', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'combat_damage',
        commander_damage: [
          { commander: 'p0', damaged: 'p1', amount: 14 },
          { commander: 'p2', damaged: 'p1' }, // missing amount — dropped
          'garbage', // not an object — dropped
        ],
      }),
    );
    expect(view.commander_damage).toEqual([{ commander: 'p0', damaged: 'p1', amount: 14 }]);

    // Omitted entirely (a non-commander game / older server) defaults to `[]`.
    expect(parseGameView('{"phase":"upkeep"}').commander_damage).toEqual([]);
  });

  it('omits result while the game is live', () => {
    // The empty-optional convention: a live view has no `result` field, so its
    // presence alone signals game over. A malformed result (no string reason) is
    // likewise dropped rather than crashing the whole view.
    expect(parseGameView('{"phase":"upkeep"}').result).toBeUndefined();
    expect(parseGameView('{"phase":"end","result":{}}').result).toBeUndefined();
  });

  it('carries the terminal result through, defaulting losers and eliding a draw’s winner', () => {
    // Game over (issue #141): winner + losers + reason round-trip verbatim.
    const won = parseGameView(
      '{"phase":"end","valid_actions":[],"result":{"winner":"p0","losers":["p1"],"reason":"decked"}}',
    );
    expect(won.result).toEqual({ winner: 'p0', losers: ['p1'], reason: 'decked' });

    // A draw omits `winner`; `losers` present in seat order.
    const drawn = parseGameView(
      '{"phase":"end","result":{"losers":["p0","p1"],"reason":"life_zero"}}',
    );
    expect(drawn.result?.winner).toBeUndefined();
    expect(drawn.result).toEqual({ losers: ['p0', 'p1'], reason: 'life_zero' });

    // Absent `losers` defaults to the empty array (still a valid game-over signal).
    const sparse = parseGameView('{"phase":"end","result":{"reason":"concede"}}');
    expect(sparse.result).toEqual({ losers: [], reason: 'concede' });

    // An unrecognized future reason is tolerated (forward compat), carried verbatim.
    const future = parseGameView('{"phase":"end","result":{"reason":"some_future_reason"}}');
    expect(future.result?.reason).toBe('some_future_reason');
  });

  it('carries the receiver id through from view.you', () => {
    const view = parseGameView('{"phase":"upkeep","you":"p1"}');
    expect(view.you).toBe('p1');
  });

  it('normalizes a permanent’s combat state, defaulting omitted fields (issue #332)', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'combat_damage',
        battlefield: [
          {
            id: 'atk',
            controller: 'p2',
            owner: 'p2',
            attacking: true,
            damage: 2,
            card: { id: 'atk', name: 'Hill Giant', type_line: 'Creature — Giant' },
          },
          {
            id: 'blk',
            controller: 'p1',
            owner: 'p1',
            blocking: 'atk',
            card: { id: 'blk', name: 'Grizzly Bears', type_line: 'Creature — Bear' },
          },
        ],
      }),
    );
    const [atk, blk] = view.battlefield;
    // Present combat state round-trips; the attacker is not blocking, the blocker
    // names its attacker, and marked damage carries through as a number.
    expect(atk!.attacking).toBe(true);
    expect(atk!.blocking).toBeUndefined();
    expect(atk!.damage).toBe(2);
    expect(blk!.attacking).toBeUndefined();
    expect(blk!.blocking).toBe('atk');
    // An omitted/zero `damage` on the blocker defaults to undamaged (elided from wire).
    expect(blk!.damage).toBeUndefined();
  });

  it('normalizes a permanent’s attachment, defaulting an omitted host to unattached (issue #333)', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'declare_blockers',
        battlefield: [
          {
            id: 'bear',
            controller: 'p1',
            owner: 'p1',
            card: { id: 'bear', name: 'Grizzly Bears', type_line: 'Creature — Bear' },
          },
          {
            id: 'aura',
            controller: 'p1',
            owner: 'p1',
            attached_to: 'bear',
            card: { id: 'aura', name: 'Pacifism', type_line: 'Enchantment — Aura' },
          },
        ],
      }),
    );
    // The unattached permanent has no host; the aura names its host verbatim.
    expect(view.battlefield[0]!.attached_to).toBeUndefined();
    expect(view.battlefield[1]!.attached_to).toBe('bear');
  });

  it('normalizes multiplayer combat + seat order, defaulting omitted fields (issue #345)', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'declare_blockers',
        active_player: 'p0',
        seat_order: ['p0', 'p1', 'p2'],
        opponents: [
          { player_id: 'p1', hand_size: 3, life: 20, library_size: 40, eliminated: true },
          { player_id: 'p2', hand_size: 5, life: 12, library_size: 38 },
        ],
        battlefield: [
          {
            id: 'atk',
            controller: 'p0',
            owner: 'p0',
            attacking: true,
            attacking_player: 'p2',
            card: { id: 'atk', name: 'Raider', type_line: 'Creature — Orc' },
          },
        ],
      }),
    );
    // Seat order rides through verbatim; the attacker names whom it attacks.
    expect(view.seat_order).toEqual(['p0', 'p1', 'p2']);
    expect(view.battlefield[0]!.attacking_player).toBe('p2');
    // Eliminated flag carried on the opponent it applies to; absent ⇒ still in.
    expect(view.opponents[0]!.eliminated).toBe(true);
    expect(view.opponents[1]!.eliminated).toBeUndefined();
  });

  it('defaults omitted multiplayer fields for a two-player / older view (issue #345)', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'declare_blockers',
        battlefield: [
          {
            id: 'atk',
            controller: 'p1',
            owner: 'p1',
            attacking: true,
            card: { id: 'atk', name: 'Bear', type_line: 'Creature — Bear' },
          },
        ],
      }),
    );
    expect(view.seat_order).toEqual([]);
    expect(view.battlefield[0]!.attacking).toBe(true);
    expect(view.battlefield[0]!.attacking_player).toBeUndefined();
  });

  it('ignores unknown fields for forward compatibility', () => {
    const view = parseGameView('{"phase":"draw","some_future_field":42}');
    expect(view.phase).toBe('draw');
    expect('some_future_field' in view).toBe(false);
  });

  it('rejects a missing or invalid phase', () => {
    expect(() => parseGameView('{}')).toThrow(ProtocolError);
    expect(() => parseGameView('{"phase":"not_a_phase"}')).toThrow(ProtocolError);
  });

  it('rejects malformed JSON and non-object payloads', () => {
    expect(() => parseGameView('not json')).toThrow(ProtocolError);
    expect(() => parseGameView('[]')).toThrow(ProtocolError);
    expect(() => normalizeGameView(42)).toThrow(ProtocolError);
  });

  it('rejects a present-but-wrong-typed collection', () => {
    expect(() => parseGameView('{"phase":"draw","valid_actions":{}}')).toThrow(ProtocolError);
  });

  it('carries a targeted action’s requirements and content-binding token through intact', () => {
    // #74/ADR 0009: a multi-step action rides on valid_actions with its slots and
    // token. Normalization must not drop them (the client renders/echoes them).
    const view = parseGameView(
      JSON.stringify({
        phase: 'precombat_main',
        valid_actions: [
          {
            id: 'a3',
            type: 'cast_spell',
            label: 'Cast Lightning Bolt',
            subject: ['c3'],
            token: 'h:9f2c',
            requirements: [
              { slot: 't0', prompt: 'target creature or player', candidates: ['perm_a', 'p2'] },
            ],
          },
        ],
      }),
    );
    const action = view.valid_actions[0];
    expect(action.token).toBe('h:9f2c');
    expect(action.requirements).toEqual([
      { slot: 't0', prompt: 'target creature or player', candidates: ['perm_a', 'p2'] },
    ]);
  });
});

describe('cross-language contract fixture (issue #56)', () => {
  // The same JSON the Rust `rune-protocol` round-trip test consumes. If a field is
  // renamed/retyped/removed in the Rust crate and this fixture is updated to match,
  // these typed assertions break here (and vice versa) — drift fails a test instead
  // of relying on same-PR discipline.
  const wireJson = JSON.stringify(CONTRACT_FIXTURE);

  it('parses through parseGameView into the fully-typed GameView', () => {
    const view = parseGameView(wireJson);

    expect(view.you).toBe('p1');
    expect(view.phase).toBe('precombat_main');
    expect(view.turn).toBe(3);
    expect(view.active_player).toBe('p1');
    expect(view.priority_player).toBe('p1');
    expect(view.action_deadline).toBe(12.5);
    expect(view.mana_pool).toEqual(['{G}', '{G}']);

    // Populated hand: the creature carries P/T, the land omits them.
    expect(view.my_hand.map((c) => c.id)).toEqual(['c1', 'c2', 'c3']);
    expect(view.my_hand[0].power).toBe('1');
    expect(view.my_hand[1].power).toBeUndefined();

    // Opponent view: hidden zones reduced to counts, statuses carried through.
    expect(view.opponents[0].hand_size).toBe(7);
    expect(view.opponents[0].statuses).toEqual(['monarch']);

    // Battlefield exercises the `Counter { kind, count }` shape and `tapped`.
    expect(view.battlefield[0].tapped).toBe(true);
    expect(view.battlefield[0].counters).toEqual([{ kind: '+1/+1', count: 2 }]);
    expect(view.battlefield[1].counters).toEqual([{ kind: 'loyalty', count: 5 }]);
    expect(view.battlefield[1].tapped).toBeUndefined();

    // Stack: an ability carries its `source`; a spell does not.
    expect(view.stack[0].source).toBeUndefined();
    expect(view.stack[1].source).toBe('perm_bear');

    // Public piles round-trip populated.
    expect(view.graveyards[0].cards[0].id).toBe('g1');
    expect(view.exile[0].cards[0].id).toBe('x1');

    // Every valid-action kind emitted today is present, in order; pass is global.
    expect(view.valid_actions.map((a) => a.type)).toEqual([
      'pass_priority',
      'play_land',
      'cast_spell',
      'activate_ability',
    ]);
    expect(view.valid_actions[0].subject).toBeUndefined();
    expect(view.valid_actions[3].subject).toEqual(['perm_bear']);
  });

  it('normalizes the parsed object identically to the raw wire text', () => {
    // normalizeGameView (object) and parseGameView (text) are the same pipeline.
    expect(normalizeGameView(CONTRACT_FIXTURE)).toEqual(parseGameView(wireJson));
  });

  it('carries the prompt-shapes fixture (option + select_from_zone) through intact', () => {
    // The same JSON the Rust crate round-trips (issue #156). The prompt slots must
    // survive normalization verbatim so the client can render/echo them; a field
    // renamed/retyped in the Rust `Prompt` and updated here breaks this assertion.
    const view = parseGameView(JSON.stringify(CONTRACT_FIXTURE_PROMPTS));
    expect(view.you).toBe('p0');
    const decision = view.valid_actions[0];
    expect(decision.type).toBe('mulligan_decision');
    expect(decision.token).toBe('t00000000deadbeef');
    expect(decision.prompts).toEqual([
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
        owner: 'p0',
        count: 1,
        candidates: ['card_10', 'card_11'],
      },
    ]);
  });

  it('carries an order prompt through normalization intact', () => {
    // `order` has no server projection yet (awaits trigger ordering, #151), but its
    // wire shape must round-trip so the client can render it when it lands.
    const view = parseGameView(
      JSON.stringify({
        phase: 'upkeep',
        valid_actions: [
          {
            id: 'a0',
            type: 'order_triggers',
            label: 'Order triggers',
            token: 't0',
            prompts: [
              {
                kind: 'order',
                slot: 'triggers',
                prompt: 'Order these triggered abilities',
                items: ['stack_1', 'stack_2'],
              },
            ],
          },
        ],
      }),
    );
    expect(view.valid_actions[0].prompts).toEqual([
      {
        kind: 'order',
        slot: 'triggers',
        prompt: 'Order these triggered abilities',
        items: ['stack_1', 'stack_2'],
      },
    ]);
  });

  it('parses the terminal counterpart fixture into a game-over GameView', () => {
    // The finished-game fixture (issue #141): `result` present, `valid_actions`
    // empty. A field renamed/retyped in the Rust `GameResult`/`GameOverReason` and
    // updated here breaks this assertion, catching cross-language drift.
    const view = parseGameView(JSON.stringify(CONTRACT_FIXTURE_OVER));
    expect(view.you).toBe('p0');
    expect(view.valid_actions).toEqual([]);
    expect(view.result).toEqual({ winner: 'p0', losers: ['p1'], reason: 'decked' });
  });
});

describe('lobby wire (issue #114)', () => {
  it('normalizes a room-less LobbyView, defaulting elided fields', () => {
    const view = normalizeLobbyView(JSON.parse(LOBBY_ROOMLESS_JSON));
    expect(view).toEqual({
      session: 's:ab12',
      you: 'p1',
      directory: [],
      valid_commands: ['create_room', 'join_room'],
    });
    expect(view.room).toBeUndefined();
  });

  it('normalizes a room with a seat roster, defaulting decked/ready to false', () => {
    const view = normalizeLobbyView(JSON.parse(LOBBY_ROOM_UNDECKED_JSON));
    expect(view.room?.room_id).toBe('r:7f3');
    expect(view.room?.config).toEqual({ seats: 2, game_setup: '1v1' });
    // Seat 0 is occupied but not decked/ready; seat 1 is empty.
    expect(view.room?.seats[0]).toEqual({
      seat: 0,
      occupied_by: 'p1',
      decked: false,
      ready: false,
    });
    expect(view.room?.seats[1].occupied_by).toBeUndefined();
  });

  it('tolerates a sparse/empty LobbyView and unknown fields (forward compat)', () => {
    const view = normalizeLobbyView({ some_future_field: true });
    expect(view).toEqual({ session: '', you: '', directory: [], valid_commands: [] });
  });

  it('normalizes the room directory, defaulting an unknown state to gathering (issue #280)', () => {
    const view = normalizeLobbyView(JSON.parse(LOBBY_DIRECTORY_JSON));
    expect(view.room).toBeUndefined();
    expect(view.directory).toEqual([
      {
        room_id: 'r0',
        config: { seats: 2, game_setup: '1v1' },
        filled: 1,
        spectators: 0,
        state: 'gathering',
      },
      {
        room_id: 'r1',
        config: { seats: 4, game_setup: 'ffa-4' },
        filled: 4,
        spectators: 0,
        state: 'in_progress',
      },
    ]);

    // A sparse entry with an unknown state falls back to gathering, and missing
    // numeric/id fields default rather than throwing.
    const sparse = normalizeLobbyView({ directory: [{ state: 'exploding' }] });
    expect(sparse.directory).toEqual([
      {
        room_id: '',
        config: { seats: 0, game_setup: '' },
        filled: 0,
        spectators: 0,
        state: 'gathering',
      },
    ]);
  });

  it('rejects a non-object LobbyView payload', () => {
    expect(() => normalizeLobbyView(42)).toThrow(ProtocolError);
  });

  it('routes a frame with a valid phase to a GameView', () => {
    const frame = parseServerFrame(SAMPLE_GAME_VIEW_JSON);
    expect(frame.kind).toBe('game');
    if (frame.kind === 'game') expect(frame.view.phase).toBe('precombat_main');
  });

  it('routes a phase-less frame to a LobbyView', () => {
    const frame = parseServerFrame(LOBBY_ROOMLESS_JSON);
    expect(frame.kind).toBe('lobby');
    if (frame.kind === 'lobby') expect(frame.lobby.session).toBe('s:ab12');
  });

  it('rejects malformed JSON when routing a frame', () => {
    expect(() => parseServerFrame('not json')).toThrow(ProtocolError);
  });
});

describe('spectator wire (issue #351)', () => {
  /** A live spectator frame over a 3-seat game with one eliminated seat. It carries a
   * `phase` (like a game view) but NO `you` (unlike one) — the structural discriminator. */
  const SPECTATOR_JSON = JSON.stringify({
    players: [
      { player_id: 'p0', hand_size: 4, life: 18, library_size: 33, graveyard_size: 2 },
      {
        player_id: 'p1',
        hand_size: 0,
        life: 0,
        library_size: 0,
        graveyard_size: 7,
        eliminated: true,
      },
      { player_id: 'p2', hand_size: 6, life: 20, library_size: 34, graveyard_size: 1 },
    ],
    battlefield: [
      {
        id: 'perm_1',
        controller: 'p0',
        owner: 'p0',
        card: {
          id: 'perm_1',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
      },
    ],
    phase: 'precombat_main',
    turn: 9,
    active_player: 'p0',
    seat_order: ['p0', 'p1', 'p2'],
    priority_player: 'p0',
  });

  it('normalizes a spectator view with every seat as public state', () => {
    const view = normalizeSpectatorView(JSON.parse(SPECTATOR_JSON));
    expect(view.players).toHaveLength(3);
    expect(view.players[1]?.eliminated).toBe(true);
    expect(view.battlefield[0]?.id).toBe('perm_1');
    expect(view.phase).toBe('precombat_main');
    expect(view.seat_order).toEqual(['p0', 'p1', 'p2']);
    // Structurally there is no receiver/decision state to read.
    expect('you' in view).toBe(false);
    expect('my_hand' in view).toBe(false);
    expect('valid_actions' in view).toBe(false);
  });

  it('routes a phase-bearing frame with no `you` to a spectator view', () => {
    const frame = parseServerFrame(SPECTATOR_JSON);
    expect(frame.kind).toBe('spectator');
    if (frame.kind === 'spectator') expect(frame.view.players).toHaveLength(3);
  });

  it('still routes a phase-bearing frame WITH `you` to a seated game view', () => {
    // The discriminator must not misroute a seated view (which always carries `you`).
    const frame = parseServerFrame(SAMPLE_GAME_VIEW_JSON);
    expect(frame.kind).toBe('game');
  });

  it('throws on a spectator payload missing its phase', () => {
    expect(() => normalizeSpectatorView({ players: [] })).toThrow(ProtocolError);
  });

  it('defaults a missing spectator count on a room summary to 0', () => {
    const view = normalizeLobbyView({
      session: 's:1',
      you: 'p0',
      directory: [
        {
          room_id: 'r1',
          config: { seats: 4, game_setup: 'standard_ffa' },
          filled: 4,
          state: 'in_progress',
        },
      ],
      valid_commands: [],
    });
    expect(view.directory[0]?.spectators).toBe(0);
  });

  it('carries a spectator count when the server advertises one', () => {
    const view = normalizeLobbyView({
      session: 's:1',
      you: 'p0',
      directory: [
        {
          room_id: 'r1',
          config: { seats: 4, game_setup: 'standard_ffa' },
          filled: 4,
          spectators: 3,
          state: 'in_progress',
        },
      ],
      valid_commands: [],
    });
    expect(view.directory[0]?.spectators).toBe(3);
  });
});

describe('priority stops and auto-pass (issue #264)', () => {
  it('normalizes GameView.stops, keeping only known phases', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'upkeep',
        you: 'p0',
        stops: ['upkeep', 'end', 'not_a_phase'],
      }),
    );
    // Known phases survive; an unrecognized future value is dropped, never thrown.
    expect(view.stops).toEqual(['upkeep', 'end']);
  });

  it('defaults stops to an empty list and auto_passed to false when omitted', () => {
    const view = parseGameView('{"phase":"upkeep","you":"p0"}');
    expect(view.stops).toEqual([]);
    expect(view.auto_passed).toBe(false);
  });

  it('carries the auto_passed indicator through when set', () => {
    const view = parseGameView('{"phase":"upkeep","you":"p0","auto_passed":true}');
    expect(view.auto_passed).toBe(true);
  });

  it('treats a malformed stops value as empty rather than throwing', () => {
    const view = parseGameView('{"phase":"upkeep","you":"p0","stops":"upkeep"}');
    expect(view.stops).toEqual([]);
  });
});

describe('display names (issue #294)', () => {
  it('normalizes GameView.player_names, dropping non-string entries', () => {
    const view = parseGameView(
      JSON.stringify({
        phase: 'upkeep',
        you: 'p0',
        player_names: { p0: 'Alice', p1: 'Bob', p2: 42 },
      }),
    );
    // Well-formed string entries survive; a non-string value is dropped.
    expect(view.player_names).toEqual({ p0: 'Alice', p1: 'Bob' });
  });

  it('defaults player_names to an empty map when the server omits it', () => {
    const view = parseGameView('{"phase":"upkeep","you":"p0"}');
    expect(view.player_names).toEqual({});
  });

  it('carries a seat display name in the roster, absent when unset', () => {
    const view = normalizeLobbyView(
      JSON.parse(
        JSON.stringify({
          session: 's:1',
          you: 'p1',
          room: {
            room_id: 'r0',
            config: { seats: 2, game_setup: '1v1' },
            seats: [
              { seat: 0, occupied_by: 'p1', name: 'Alice' },
              { seat: 1, occupied_by: 'p2' },
            ],
          },
          valid_commands: ['set_name'],
        }),
      ),
    );
    expect(view.room?.seats[0].name).toBe('Alice');
    expect(view.room?.seats[1].name).toBeUndefined();
  });

  it('carries the connection’s own display name on the LobbyView, absent when unset', () => {
    const named = normalizeLobbyView({ session: 's:1', you: 'p1', name: 'Alice' });
    expect(named.name).toBe('Alice');
    const unnamed = normalizeLobbyView({ session: 's:1', you: 'p1' });
    expect(unnamed.name).toBeUndefined();
  });
});

describe('catalog wire (issue #367)', () => {
  // A terse wire catalog: one full creature card and one basic land that elides its
  // absent mana cost / P/T / keywords, plus a permissive format that omits its `None`
  // upper bounds and a strict one that carries them.
  const CATALOG_JSON = JSON.stringify({
    catalog_version: 1,
    cards: [
      {
        functional_id: 'serra_angel',
        name: 'Serra Angel',
        type_line: 'Creature — Angel',
        mana_cost: '{3}{W}{W}',
        rules_text: 'Flying, vigilance',
        power: '4',
        toughness: '4',
        keywords: ['flying', 'vigilance'],
      },
      {
        functional_id: 'forest',
        name: 'Forest',
        type_line: 'Basic Land — Forest',
        rules_text: '{T}: Add {G}.',
      },
    ],
    formats: [
      {
        game_setup: 'standard_2p',
        min_deck_size: 0,
        basic_land_exempt: true,
        min_seats: 2,
        max_seats: 8,
      },
      {
        game_setup: 'starter-1v1',
        min_deck_size: 40,
        max_copies: 4,
        basic_land_exempt: true,
        min_seats: 2,
        max_seats: 2,
      },
    ],
  });

  it('routes a catalog_version frame (no phase) to a CatalogView', () => {
    const frame = parseServerFrame(CATALOG_JSON);
    expect(frame.kind).toBe('catalog');
    if (frame.kind === 'catalog') {
      expect(frame.catalog.catalog_version).toBe(1);
      expect(frame.catalog.cards).toHaveLength(2);
      expect(frame.catalog.formats).toHaveLength(2);
    }
  });

  it('does not route a phase-less, version-less lobby frame to a CatalogView', () => {
    const frame = parseServerFrame(LOBBY_ROOMLESS_JSON);
    expect(frame.kind).toBe('lobby');
  });

  it('normalizes elided card optionals to their defaults', () => {
    const view = normalizeCatalogView(JSON.parse(CATALOG_JSON));
    const angel = view.cards[0];
    expect(angel.mana_cost).toBe('{3}{W}{W}');
    expect(angel.power).toBe('4');
    expect(angel.keywords).toEqual(['flying', 'vigilance']);
    const forest = view.cards[1];
    expect(forest.mana_cost).toBeUndefined();
    expect(forest.power).toBeUndefined();
    expect(forest.toughness).toBeUndefined();
    expect(forest.keywords).toBeUndefined();
    expect(forest.rules_text).toBe('{T}: Add {G}.');
  });

  it('keeps a permissive format’s absent upper bounds absent (honest permissiveness)', () => {
    const view = normalizeCatalogView(JSON.parse(CATALOG_JSON));
    const open = view.formats[0];
    expect(open.min_deck_size).toBe(0);
    expect(open.max_deck_size).toBeUndefined();
    expect(open.max_copies).toBeUndefined();
    const strict = view.formats[1];
    expect(strict.min_deck_size).toBe(40);
    expect(strict.max_copies).toBe(4);
  });

  it('defaults a missing catalog_version and collections', () => {
    const view = normalizeCatalogView({});
    expect(view.catalog_version).toBe(0);
    expect(view.cards).toEqual([]);
    expect(view.formats).toEqual([]);
  });

  it('rejects a non-object CatalogView payload', () => {
    expect(() => normalizeCatalogView(42)).toThrow(ProtocolError);
  });
});
