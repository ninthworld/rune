import { describe, expect, it } from 'vitest';
import {
  normalizeGameView,
  normalizeLobbyView,
  parseGameView,
  parseServerFrame,
  ProtocolError,
} from './wire';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';
import { LOBBY_ROOM_UNDECKED_JSON, LOBBY_ROOMLESS_JSON } from './lobby-view.fixture';
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
      mana_pool: [],
      priority_player: undefined,
      valid_actions: [],
      action_deadline: undefined,
      result: undefined,
    });
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
    expect(view).toEqual({ session: '', you: '', valid_commands: [] });
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
