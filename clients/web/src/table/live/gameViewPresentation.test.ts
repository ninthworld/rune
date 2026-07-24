import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView, Permanent } from '../../protocol';
import { SCENE_BATCH, SCENE_MOTION, SCENE_SEAT_ACCENTS } from '../../sceneTokens';
import {
  deriveGameViewPresentation,
  freezeDepartedEffectAnchors,
  type GameViewPresentation,
} from './gameViewPresentation';

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

/**
 * A four-seat table on the same sample frame: `p1` receives, `p2` is focused,
 * `p3`/`p4` are off-focus wings (or summary tiles on compact geometry).
 */
function table(): GameView {
  const seated = view();
  seated.seat_order = ['p1', 'p2', 'p3', 'p4'];
  seated.opponents = ['p2', 'p3', 'p4'].map((player_id) => ({
    player_id,
    hand_size: 7,
    life: 40,
    library_size: 53,
    graveyard_size: 0,
  }));
  seated.log = [];
  return seated;
}

/** The anchors every off-focus ping in a presentation landed on. */
function pingedSeats(presentation: GameViewPresentation): string[] {
  return presentation.transients
    .filter((invocation) => invocation.category === 'off-focus-ping')
    .map((invocation) => ('ref' in invocation.target ? invocation.target.ref : ''));
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

/**
 * The off-focus activity channel (issue #501, layout-model §Focus model:
 * "off-focus activity is never silent"). The ping's anchor is the seat
 * reference, which the plane resolves to the wing's crest cluster or — on
 * compact geometry — the seat's summary tile; the derivation is the same.
 */
describe('deriveGameViewPresentation off-focus staging', () => {
  it('pings a wing seat that casts, without touching receiver or focus staging', () => {
    const previous = table();
    const current = table();
    current.log = [
      { sequence: 40, event: { type: 'spell_cast', player: 'p3', card: { id: 'x', name: 'X' } } },
      { sequence: 41, event: { type: 'cards_drawn', player: 'p1', count: 1 } },
      { sequence: 42, event: { type: 'spell_cast', player: 'p2', card: { id: 'y', name: 'Y' } } },
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    // Only the off-focus wing pings; the receiver and the focused seat keep
    // exactly the presentation they had before this channel existed.
    expect(pingedSeats(result)).toEqual(['seat:p3']);
    expect(result.transients).toContainEqual(
      expect.objectContaining({ category: 'cast', target: { ref: 'seat:p2' } }),
    );
    expect(result.transients).toContainEqual(
      expect.objectContaining({ category: 'draw', target: { ref: 'seat:p1' } }),
    );
  });

  it('pings a wing seat for a trigger-shaped resolution and for a zone change', () => {
    const previous = table();
    const current = table();
    // No `trigger` event exists in the log taxonomy: a triggered ability reaches
    // the client as a stack resolution plus the zone change it caused.
    current.log = [
      {
        sequence: 40,
        event: { type: 'spell_resolved', player: 'p4', card: { id: 'z', name: 'Z' } },
      },
    ];
    current.graveyards = [
      { player_id: 'p3', cards: [{ id: 'gone', name: 'Gone', type_line: 'Creature' }] },
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    expect(pingedSeats(result)).toEqual(['seat:p3', 'seat:p4']);
  });

  it('pings a wing seat whose board changes with no log entry at all', () => {
    const previous = table();
    const current = table();
    current.battlefield = [
      ...previous.battlefield,
      { ...permanent('wing-token', 'p4'), tapped: false },
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    expect(pingedSeats(result)).toEqual(['seat:p4']);
  });

  it('batches a busy wing into exactly one ping, ahead of the capped batch', () => {
    const previous = table();
    const current = table();
    current.log = [
      { sequence: 40, event: { type: 'cards_drawn', player: 'p3', count: 3 } },
      { sequence: 41, event: { type: 'spell_cast', player: 'p3', card: { id: 'x', name: 'X' } } },
      {
        sequence: 42,
        event: { type: 'spell_resolved', player: 'p3', card: { id: 'x', name: 'X' } },
      },
    ];
    current.battlefield = [
      ...previous.battlefield,
      permanent('a', 'p3'),
      permanent('b', 'p3'),
      permanent('c', 'p3'),
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    // One cue per seat, never a stack of pulses — and first in the batch, so a
    // dense moment can never push the "never silent" guarantee past the cap.
    expect(pingedSeats(result)).toEqual(['seat:p3']);
    expect(result.transients[0]?.category).toBe('off-focus-ping');
  });

  it('never pings the receiver, the focused seat, or a duel opponent', () => {
    const focused = table();
    focused.log = [
      { sequence: 40, event: { type: 'spell_cast', player: 'p2', card: { id: 'x', name: 'X' } } },
      { sequence: 41, event: { type: 'spell_cast', player: 'p1', card: { id: 'y', name: 'Y' } } },
    ];
    expect(pingedSeats(deriveGameViewPresentation(table(), focused, { focusSeat: 'p2' }))).toEqual(
      [],
    );

    // A duel stages both boards: there is no off-focus seat to be silent.
    const previous = view();
    const duel = view();
    duel.log = [
      ...(previous.log ?? []),
      { sequence: 40, event: { type: 'spell_cast', player: 'p2', card: { id: 'x', name: 'X' } } },
    ];
    expect(pingedSeats(deriveGameViewPresentation(previous, duel))).toEqual([]);
  });

  it("wears the acting seat's identity accent, in stable seat order", () => {
    const previous = table();
    const current = table();
    current.log = [
      { sequence: 40, event: { type: 'cards_drawn', player: 'p4', count: 1 } },
      { sequence: 41, event: { type: 'cards_drawn', player: 'p3', count: 1 } },
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    expect(result.transients.slice(0, 2)).toEqual([
      { category: 'off-focus-ping', target: { ref: 'seat:p3' }, accent: SCENE_SEAT_ACCENTS[2] },
      { category: 'off-focus-ping', target: { ref: 'seat:p4' }, accent: SCENE_SEAT_ACCENTS[3] },
    ]);
  });

  it('stages combat against a wing seat regardless of which board holds focus', () => {
    const current = table();
    current.battlefield = [
      { ...permanent('wing-attacker', 'p4'), attacking: true, attacking_player: 'p3' },
    ];

    const result = deriveGameViewPresentation(undefined, current, { focusSeat: 'p2' });

    // Neither end of the attack is the focused seat; the path is drawn anyway,
    // terminating at the defending wing's crest.
    expect(result.persistent).toContainEqual(
      expect.objectContaining({
        id: 'attack:wing-attacker',
        category: 'attack-path',
        from: { ref: 'wing-attacker' },
        to: { ref: 'seat:p3' },
      }),
    );
  });
});
