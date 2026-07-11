import { describe, expect, it } from 'vitest';
import { normalizeGameView, parseGameView, ProtocolError } from './wire';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';
// The single canonical cross-language fixture, owned by the `rune-protocol` crate
// and round-tripped by its Rust test. Resolved via a path alias (tsconfig.json +
// vitest.config.ts) so there is exactly one copy of the JSON in the repo.
import CONTRACT_FIXTURE from '@protocol-fixtures/gameview.json';

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
    });
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
});
