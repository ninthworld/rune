import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView, Permanent } from '../../protocol';
import { SCENE_BATCH, SCENE_MOTION } from '../../sceneTokens';
import { deriveGameViewPresentation, freezeDepartedEffectAnchors } from './gameViewPresentation';

function view(): GameView {
  return structuredClone(SAMPLE_GAME_VIEW);
}

function permanent(id: string, controller = 'p1'): Permanent {
  return {
    id,
    controller,
    owner: controller,
    card: {
      id,
      name: `Card ${id}`,
      type_line: 'Creature',
      power: '1',
      toughness: '1',
    },
  };
}

describe('deriveGameViewPresentation', () => {
  it('reconstructs persistent combat on first mount without replaying history', () => {
    const current = view();
    current.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      { ...permanent('blocker', 'p2'), blocking: 'attacker' },
    ];

    const result = deriveGameViewPresentation(undefined, current);

    expect(result.motions).toEqual([]);
    expect(result.transients).toEqual([]);
    expect(result.persistent.map(({ id, category }) => ({ id, category }))).toEqual([
      { id: 'attack:attacker', category: 'attack-path' },
      { id: 'block:blocker', category: 'blocker-link' },
    ]);
  });

  it('resolves the sole duel defender when attacking_player is omitted', () => {
    const current = view();
    current.seat_order = ['p1', 'p2'];
    current.battlefield = [{ ...permanent('attacker', 'p1'), attacking: true }];

    const result = deriveGameViewPresentation(undefined, current);

    expect(result.persistent).toContainEqual(
      expect.objectContaining({
        id: 'attack:attacker',
        category: 'attack-path',
        to: { ref: 'seat:p2' },
      }),
    );
  });

  it('maps new structured log entries to semantic cast, resolve, counter, draw, and flow cues', () => {
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      {
        sequence: 37,
        event: { type: 'cards_drawn', player: 'p1', count: 2 },
      },
      {
        sequence: 38,
        event: { type: 'spell_cast', player: 'p1', card: { id: 'spell', name: 'Spell' } },
      },
      {
        sequence: 39,
        event: { type: 'spell_resolved', player: 'p1', card: { id: 'spell', name: 'Spell' } },
      },
      {
        sequence: 40,
        event: {
          type: 'spell_countered',
          player: 'p2',
          card: { id: 'other', name: 'Other' },
        },
      },
      {
        sequence: 41,
        event: { type: 'step_changed', turn: 6, active_player: 'p2', phase: 'untap' },
      },
    ];
    current.turn = 6;
    current.phase = 'untap';
    current.active_player = 'p2';

    const result = deriveGameViewPresentation(previous, current);

    expect(result.motions.map((motion) => motion.category)).toEqual(
      expect.arrayContaining(['draw', 'cast', 'resolve', 'counter', 'turn']),
    );
    expect(result.transients.map((effect) => effect.category)).toEqual(
      expect.arrayContaining(['draw', 'cast', 'resolution', 'counter', 'flow']),
    );
    expect(result.motions.every((motion) => motion.durationMs <= 600)).toBe(true);
  });

  it('uses view diffs for tap, counters, P/T, appearances, and generic zone travel', () => {
    const previous = view();
    previous.log = [];
    previous.graveyards = [
      {
        player_id: 'p1',
        cards: [{ id: 'grave-card', name: 'Past', type_line: 'Creature' }],
      },
    ];
    const current = view();
    current.log = [];
    current.battlefield = [
      {
        ...previous.battlefield[0]!,
        tapped: false,
        counters: [{ kind: '+1/+1', count: 3 }],
        card: { ...previous.battlefield[0]!.card, power: '3', toughness: '3' },
      },
      permanent('appeared'),
    ];
    current.graveyards = [];
    current.exile = [
      {
        player_id: 'p1',
        cards: [{ id: 'grave-card', name: 'Past', type_line: 'Creature' }],
      },
    ];

    const result = deriveGameViewPresentation(previous, current);

    expect(result.motions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ category: 'untap', entityId: 'perm_xyz' }),
        expect.objectContaining({ category: 'counter-change', entityId: 'perm_xyz' }),
        expect.objectContaining({ category: 'battlefield-entry', entityId: 'appeared' }),
        expect.objectContaining({ category: 'zone-travel', entityId: 'grave-card' }),
      ]),
    );
    // The wire has no token/destruction-reason bit: appearance and pile travel
    // deliberately stay generic rather than guessing gameplay semantics.
    expect(result.transients).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ category: 'counter-change' }),
        expect.objectContaining({ category: 'battlefield-entry' }),
      ]),
    );
  });

  it('detects counter-kind changes even when the total count is unchanged', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield[0] = {
      ...previous.battlefield[0]!,
      counters: [{ kind: '+1/+1', count: 1 }],
    };
    const current = view();
    current.log = [];
    current.battlefield[0] = {
      ...current.battlefield[0]!,
      counters: [{ kind: 'shield', count: 1 }],
    };

    const result = deriveGameViewPresentation(previous, current);

    expect(result.motions).toContainEqual(
      expect.objectContaining({ category: 'counter-change', entityId: 'perm_xyz' }),
    );
    expect(result.transients).toContainEqual(
      expect.objectContaining({ category: 'counter-change', target: { ref: 'perm_xyz' } }),
    );
  });

  it('stages mass untap/appearance batches inside the 80/800 ms caps', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield = Array.from({ length: 20 }, (_, index) => ({
      ...permanent(`old-${index}`),
      tapped: true,
    }));
    const current = view();
    current.log = [];
    current.battlefield = [
      ...previous.battlefield.map((entry) => ({ ...entry, tapped: false })),
      ...Array.from({ length: 20 }, (_, index) => permanent(`new-${index}`)),
    ];

    const result = deriveGameViewPresentation(previous, current);
    const batches = result.motions.filter(
      (motion) => motion.category === 'untap' || motion.category === 'battlefield-entry',
    );

    expect(batches).toHaveLength(40);
    expect(
      Math.max(...batches.map((motion) => motion.delayMs + motion.durationMs)),
    ).toBeLessThanOrEqual(SCENE_BATCH.windowMs);
    expect(
      batches.every(
        (motion) =>
          motion.delayMs === 0 ||
          motion.delayMs % SCENE_BATCH.staggerMs === 0 ||
          motion.delayMs + motion.durationMs === SCENE_BATCH.windowMs,
      ),
    ).toBe(true);
  });

  it('maps damage/healing/death without guessing an ambiguous zone-change reason', () => {
    const previous = view();
    const current = view();
    current.battlefield = [];
    current.log = [
      ...(previous.log ?? []),
      { sequence: 37, event: { type: 'life_changed', player: 'p1', amount: 3 } },
      {
        sequence: 38,
        event: {
          type: 'damage_dealt',
          target: { kind: 'permanent', permanent: { id: 'perm_xyz', name: 'Bear' } },
          amount: 2,
        },
      },
      {
        sequence: 39,
        event: { type: 'permanent_died', permanent: { id: 'perm_xyz', name: 'Bear' } },
      },
    ];

    const result = deriveGameViewPresentation(previous, current);

    expect(result.transients).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ category: 'healing' }),
        expect.objectContaining({ category: 'damage' }),
        expect.objectContaining({ category: 'death', target: { ref: 'perm_xyz' } }),
      ]),
    );
    expect(result.motions.find((motion) => motion.category === 'death')).toMatchObject({
      from: 'perm_xyz',
      to: 'pile:p1',
      durationMs: SCENE_MOTION.zoneTravel.ms,
    });
  });

  it('collapses Lite and reduced-motion batches to a single unstaggered beat', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield = Array.from({ length: 12 }, (_, index) => ({
      ...permanent(`old-${index}`),
      tapped: true,
    }));
    const current = view();
    current.log = [];
    current.battlefield = previous.battlefield.map((entry) => ({ ...entry, tapped: false }));

    const lite = deriveGameViewPresentation(previous, current, { quality: 'lite' });
    const reduced = deriveGameViewPresentation(previous, current, { reducedMotion: true });

    expect(lite.motions.every((motion) => motion.delayMs === 0)).toBe(true);
    expect(reduced.motions.every((motion) => motion.delayMs === 0)).toBe(true);
  });

  it('freezes a departed effect at the prior visual rect', () => {
    const oldRect = { x: 10, y: 20, w: 44, h: 60 };
    const result = freezeDepartedEffectAnchors(
      [{ category: 'death', target: { ref: 'gone' }, accent: '#fff' }],
      new Map([['gone', oldRect]]),
      new Map(),
    );

    expect(result[0]!.target).toEqual({ rect: oldRect });
  });

  it('accepts server-driven targeting paths and emits a focus staging cue', () => {
    const previous = view();
    const current = view();
    const result = deriveGameViewPresentation(previous, current, {
      previousFocusSeat: 'p1',
      focusSeat: 'p2',
      targetingPaths: [{ id: 'a1:target-0', from: 'perm_xyz', to: 'p2' }],
    });

    expect(result.motions).toContainEqual(
      expect.objectContaining({ category: 'focus', to: 'seat:p2' }),
    );
    expect(result.persistent).toContainEqual({
      id: 'target:a1:target-0',
      category: 'targeting-path',
      from: { ref: 'perm_xyz' },
      to: { ref: 'seat:p2' },
      accent: '#E0784A',
    });
  });
});
