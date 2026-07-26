import { describe, expect, it } from 'vitest';
import { normalizeGameView, normalizeSpectatorView, parseGameView } from './wire';
// The presentation-window fixture (issue #594): one causal batch of ordered moments on a
// mid-game frame whose board has already moved past every one of them. The same bytes the
// `rune-protocol` crate round-trips, resolved via the path alias (tsconfig.json +
// vitest.config.ts) so there is exactly one copy of the JSON in the repo.
import CONTRACT_FIXTURE_MOMENTS from '@protocol-fixtures/gameview-moments.json';

describe('presentation window (issue #594)', () => {
  const BOLT_FACE = {
    id: 'card_bolt',
    name: 'Quickfire Bolt',
    type_line: 'Instant',
    mana_cost: '{R}',
    rules_text: 'Quickfire Bolt deals 3 damage to any target.',
    functional_id: 'quickfire_bolt',
  };
  const BEAR_FACE = {
    id: 'perm_bear',
    name: 'Grizzly Bears',
    type_line: 'Creature — Bear',
    mana_cost: '{1}{G}',
    power: '2',
    toughness: '2',
    functional_id: 'grizzly_bears',
  };

  it('normalizes the contract fixture into the ordered presentation window', () => {
    // The same bytes the Rust round-trip test reads (issue #56). Every moment describes
    // a board the frame has already moved past — the bolt is gone from the stack and the
    // bear is already in the graveyard — which is exactly why the retained faces ride
    // along instead of being looked up in the view.
    const view = normalizeGameView(CONTRACT_FIXTURE_MOMENTS);
    expect(view.presentation).toEqual([
      {
        id: 412,
        batch: 57,
        turn: 8,
        phase: 'precombat_main',
        count: 1,
        kind: {
          kind: 'cast',
          player: 'p1',
          object: { id: 'card_bolt', name: 'Quickfire Bolt', card: BOLT_FACE },
        },
      },
      {
        id: 413,
        batch: 57,
        turn: 8,
        phase: 'precombat_main',
        cause: 412,
        count: 1,
        kind: {
          kind: 'resolved',
          player: 'p1',
          object: { id: 'card_bolt', name: 'Quickfire Bolt', card: BOLT_FACE },
        },
      },
      {
        id: 414,
        batch: 57,
        turn: 8,
        phase: 'precombat_main',
        cause: 413,
        count: 1,
        kind: {
          kind: 'died',
          object: { id: 'perm_bear', name: 'Grizzly Bears', card: BEAR_FACE },
        },
      },
      {
        id: 415,
        batch: 57,
        turn: 8,
        phase: 'precombat_main',
        cause: 414,
        count: 1,
        kind: {
          kind: 'zone_move',
          object: { id: 'perm_bear', name: 'Grizzly Bears', card: BEAR_FACE },
          from: 'battlefield',
          to: 'graveyard',
        },
      },
      {
        id: 416,
        batch: 57,
        turn: 8,
        phase: 'precombat_main',
        // Aggregated: two identical damage moments collapsed into one caption. `count`
        // is an occurrence tally, never an amount — the amount stays 1.
        count: 2,
        kind: { kind: 'damage', target: { kind: 'player', player: 'p1' }, amount: 1 },
      },
      {
        id: 417,
        batch: 57,
        turn: 8,
        // Where the game *was*, not where it is: the frame's own phase is
        // `declare_attackers` and the moment happened at `begin_combat`.
        phase: 'begin_combat',
        count: 1,
        kind: {
          kind: 'phases_skipped',
          steps: [
            { phase: 'upkeep', turn: 8 },
            { phase: 'draw', turn: 8 },
            { phase: 'begin_combat', turn: 8 },
          ],
          reason: 'no_response_available',
        },
      },
    ]);
    // The window is advisory: the authoritative position and log are unchanged by it.
    expect(view.phase).toBe('declare_attackers');
    expect(view.log).toHaveLength(3);
  });

  it('defaults the presentation window to empty when the wire omits it', () => {
    // A server that predates the pacing contract sends no key at all, and a malformed
    // value is treated the same way — never thrown, because a caption may not break a
    // board a client is otherwise able to render.
    expect(parseGameView('{"phase":"upkeep","you":"p0"}').presentation).toEqual([]);
    expect(
      parseGameView('{"phase":"upkeep","you":"p0","presentation":"cast"}').presentation,
    ).toEqual([]);
  });

  it('keeps the window in wire order, gaps and repeats included', () => {
    // Ids may start late and skip (the window is bounded, and another seat's
    // `phases_skipped` is filtered out of this stream), and a repeated kind is a real
    // repeat. Nothing here sorts, merges, or de-duplicates.
    const view = normalizeGameView({
      phase: 'end',
      you: 'p0',
      presentation: [
        {
          id: 90,
          batch: 4,
          turn: 3,
          phase: 'draw',
          kind: { kind: 'drew', player: 'p0', count: 1 },
        },
        {
          id: 97,
          batch: 4,
          turn: 3,
          phase: 'draw',
          kind: { kind: 'drew', player: 'p0', count: 1 },
        },
        { id: 91, batch: 4, turn: 3, phase: 'end', kind: { kind: 'phase_change', phase: 'end' } },
      ],
    });
    expect(view.presentation?.map((moment) => moment.id)).toEqual([90, 97, 91]);
  });

  it('drops a moment whose frame or kind payload it cannot read', () => {
    // An unreadable frame cannot be ordered or de-duplicated, and a defaulted id would
    // collide with a real one; a known kind with a missing half names a beat this client
    // cannot honestly caption, and inventing the half would assert game structure the
    // server never sent.
    const view = normalizeGameView({
      phase: 'upkeep',
      you: 'p0',
      presentation: [
        { batch: 1, turn: 2, phase: 'upkeep', kind: { kind: 'phase_change', phase: 'upkeep' } },
        {
          id: 'seven',
          batch: 1,
          turn: 2,
          phase: 'upkeep',
          kind: { kind: 'drew', player: 'p0', count: 1 },
        },
        {
          id: 2,
          batch: 1,
          turn: 2,
          phase: 'nonsense',
          kind: { kind: 'phase_change', phase: 'upkeep' },
        },
        { id: 3, batch: 1, turn: 2, phase: 'upkeep' },
        // Known kinds, unreadable payloads: no amount, no retained name, an endpoint
        // outside the zone vocabulary, and an unstated auto-pass reason.
        {
          id: 4,
          batch: 1,
          turn: 2,
          phase: 'upkeep',
          kind: { kind: 'damage', target: { kind: 'player', player: 'p1' } },
        },
        {
          id: 5,
          batch: 1,
          turn: 2,
          phase: 'upkeep',
          kind: { kind: 'died', object: { id: 'perm_a' } },
        },
        {
          id: 6,
          batch: 1,
          turn: 2,
          phase: 'upkeep',
          kind: {
            kind: 'zone_move',
            object: { id: 'c1', name: 'Bear' },
            from: 'battlefield',
            to: 'sideboard',
          },
        },
        { id: 7, batch: 1, turn: 2, phase: 'upkeep', kind: { kind: 'phases_skipped', steps: [] } },
        'not a moment',
        {
          id: 8,
          batch: 1,
          turn: 2,
          phase: 'upkeep',
          kind: { kind: 'drew', player: 'p0', count: 2 },
        },
      ],
    });
    expect(view.presentation).toEqual([
      {
        id: 8,
        batch: 1,
        turn: 2,
        phase: 'upkeep',
        count: 1,
        kind: { kind: 'drew', player: 'p0', count: 2 },
      },
    ]);
  });

  it('keeps a moment whose kind this build does not know as unclassified', () => {
    // The vocabulary widens additively (a counter change is the named next candidate).
    // An unknown tag is not coerced into a known arm and not dropped either: the moment
    // holds its place in the ordered stream, marked so nothing can misread it.
    const view = normalizeGameView({
      phase: 'upkeep',
      you: 'p0',
      presentation: [
        {
          id: 20,
          batch: 2,
          turn: 1,
          phase: 'upkeep',
          count: 3,
          kind: {
            kind: 'counter_changed',
            object: { id: 'perm_a', name: 'Bear' },
            counter: '+1/+1',
          },
        },
        {
          id: 21,
          batch: 2,
          turn: 1,
          phase: 'upkeep',
          kind: { kind: 'phase_change', phase: 'upkeep' },
        },
      ],
    });
    expect(view.presentation).toEqual([
      { id: 20, batch: 2, turn: 1, phase: 'upkeep', count: 3, kindUnknown: true },
      {
        id: 21,
        batch: 2,
        turn: 1,
        phase: 'upkeep',
        count: 1,
        kind: { kind: 'phase_change', phase: 'upkeep' },
      },
    ]);
    // Unclassified means unclassified: no `kind` is invented for it.
    expect(view.presentation?.[0].kind).toBeUndefined();
  });

  it('carries the same public window on a spectator view', () => {
    const view = normalizeSpectatorView({
      players: [],
      phase: 'combat_damage',
      presentation: [
        {
          id: 61,
          batch: 9,
          turn: 4,
          phase: 'combat_damage',
          kind: {
            kind: 'damage',
            target: { kind: 'permanent', permanent: { id: 'perm_a', name: 'Bear' } },
            amount: 3,
          },
        },
      ],
    });
    expect(view.presentation).toEqual([
      {
        id: 61,
        batch: 9,
        turn: 4,
        phase: 'combat_damage',
        count: 1,
        kind: {
          kind: 'damage',
          target: { kind: 'permanent', permanent: { id: 'perm_a', name: 'Bear' } },
          amount: 3,
        },
      },
    ]);
    // A spectator holds no seat, so an older frame with no window is still just empty.
    expect(normalizeSpectatorView({ players: [], phase: 'upkeep' }).presentation).toEqual([]);
  });
});
