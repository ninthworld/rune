import { describe, expect, it } from 'vitest';
import { normalizeGameView, parseGameView, ProtocolError } from './wire';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';

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
});
