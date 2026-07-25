import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameResult, GameView, Permanent } from '../../protocol';
import {
  SCENE_BATCH,
  SCENE_HUES,
  SCENE_MOTION,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
} from '../../sceneTokens';
import { TRANSIENT_CAP } from '../effects';
import {
  deriveGameViewPresentation,
  freezeDepartedEffectAnchors,
  freezeDepartedRelationshipAnchors,
  type GameViewPresentation,
} from './gameViewPresentation';
import { momentBudgetMs, momentCapMs } from './sessionMoments';

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

/** A member of a swarm: same identity and controller as its siblings, so the
 * adapter reads a simultaneous multiple as one batch (never a rules claim). */
function token(id: string, controller = 'p1'): Permanent {
  const base = permanent(id, controller);
  return {
    ...base,
    card: { ...base.card, name: 'Spirit', functional_id: 'spirit_token', type_line: 'Creature' },
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
    // The wire has no token/destruction-reason bit: a LONE appearance and pile
    // travel stay generic rather than guessing gameplay semantics (only
    // simultaneous identical multiples read as a swarm — see the batch tests).
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

  it('stages simultaneous identical arrivals as one token batch', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield = [];
    const current = view();
    current.log = [];
    current.battlefield = [
      ...Array.from({ length: 12 }, (_, index) => token(`spirit-${index}`)),
      // A lone appearance is not a swarm; it keeps the generic entry cue.
      permanent('reanimated'),
    ];

    const result = deriveGameViewPresentation(previous, current);
    const batch = result.motions.filter((motion) => motion.category === 'token-batch');

    expect(batch).toHaveLength(12);
    expect(result.motions).toContainEqual(
      expect.objectContaining({ category: 'battlefield-entry', entityId: 'reanimated' }),
    );
    // ≤ 80 ms stagger per item, ≤ 800 ms total window (presentation-budgets).
    const delays = [...new Set(batch.map((motion) => motion.delayMs))].sort((a, b) => a - b);
    for (const [index, delay] of delays.entries()) {
      if (index === 0) continue;
      expect(delay - delays[index - 1]!).toBeLessThanOrEqual(SCENE_BATCH.staggerCap);
    }
    expect(delays[0]).toBe(0);
    expect(
      Math.max(...batch.map((motion) => motion.delayMs + motion.durationMs)),
    ).toBeLessThanOrEqual(SCENE_BATCH.windowMs);
  });

  it('never reads a swarm across controllers, identities, or a known origin', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield = [];
    previous.graveyards = [
      { player_id: 'p1', cards: [{ id: 'returning', name: 'Spirit', type_line: 'Creature' }] },
    ];
    const current = view();
    current.log = [];
    current.graveyards = [];
    current.battlefield = [token('mine', 'p1'), token('theirs', 'p2'), token('returning')];

    const result = deriveGameViewPresentation(previous, current);

    // Different controllers are different swarms; a card that travelled from a
    // visible zone is zone travel, not an appearance at all.
    expect(result.motions.some((motion) => motion.category === 'token-batch')).toBe(false);
    expect(result.motions).toContainEqual(
      expect.objectContaining({ category: 'zone-travel', entityId: 'returning' }),
    );
  });

  it('snaps a token batch under Lite and reduced motion', () => {
    const previous = view();
    previous.log = [];
    previous.battlefield = [];
    const current = view();
    current.log = [];
    current.battlefield = Array.from({ length: 30 }, (_, index) => token(`spirit-${index}`));

    for (const staging of [{ quality: 'lite' as const }, { reducedMotion: true }]) {
      const result = deriveGameViewPresentation(previous, current, staging);
      const batch = result.motions.filter((motion) => motion.category === 'token-batch');
      expect(batch).toHaveLength(30);
      expect(batch.every((motion) => motion.delayMs === 0)).toBe(true);
    }
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

  // ── Session moments (visual-system §8, issue #509) ────────────────────────
  //
  // `game_over`, `mulligan`, and `hand_kept` were deliberately state-first
  // stubs: the adapter recognized them and emitted nothing. They now carry real
  // intents, inside the §8 windows, still derived only from what the server
  // already recorded in the log.

  it('sweeps the hand back to the library on a mulligan', () => {
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      { sequence: 37, event: { type: 'mulligan', player: 'p1' } },
    ];

    const result = deriveGameViewPresentation(previous, current);

    expect(result.motions).toContainEqual(
      expect.objectContaining({
        category: 'mulligan',
        from: 'hand:p1',
        to: 'pile:p1',
        durationMs: momentBudgetMs('mulligan'),
      }),
    );
    expect(result.transients).toContainEqual(
      expect.objectContaining({ category: 'flow', target: { ref: 'seat:p1' } }),
    );
  });

  it('travels the bottomed cards to the library when a hand is kept', () => {
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      { sequence: 37, event: { type: 'hand_kept', player: 'p1' } },
    ];

    const result = deriveGameViewPresentation(previous, current);

    expect(result.motions).toContainEqual(
      expect.objectContaining({
        category: 'hand-kept',
        from: 'hand:p1',
        to: 'pile:p1',
        durationMs: momentBudgetMs('hand-kept'),
      }),
    );
    expect(result.transients).toContainEqual(
      expect.objectContaining({ category: 'draw', target: { ref: 'seat:p1' } }),
    );
  });

  it('stages a multiplayer pre-game as one batch inside the 80/800 ms caps', () => {
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      ...['p1', 'p2', 'p3', 'p4'].map((player, index) => ({
        sequence: 40 + index,
        event: { type: 'hand_kept' as const, player },
      })),
    ];

    const result = deriveGameViewPresentation(previous, current);
    const batch = result.motions.filter((motion) => motion.category === 'hand-kept');

    expect(batch).toHaveLength(4);
    const delays = batch.map((motion) => motion.delayMs).sort((a, b) => a - b);
    expect(delays[0]).toBe(0);
    for (const [index, delay] of delays.entries()) {
      if (index === 0) continue;
      expect(delay - delays[index - 1]!).toBeLessThanOrEqual(SCENE_BATCH.staggerCap);
    }
    expect(
      Math.max(...batch.map((motion) => motion.delayMs + motion.durationMs)),
    ).toBeLessThanOrEqual(SCENE_BATCH.windowMs);
  });

  it('snaps the pre-game batch under Lite and reduced motion', () => {
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      { sequence: 40, event: { type: 'mulligan', player: 'p1' } },
      { sequence: 41, event: { type: 'hand_kept', player: 'p2' } },
    ];

    for (const staging of [{ quality: 'lite' as const }, { reducedMotion: true }]) {
      const result = deriveGameViewPresentation(previous, current, staging);
      const pregame = result.motions.filter(
        (motion) => motion.category === 'mulligan' || motion.category === 'hand-kept',
      );
      expect(pregame).toHaveLength(2);
      expect(pregame.every((motion) => motion.delayMs === 0)).toBe(true);
    }
  });

  it('stages a victory as the gold bloom inside the ≤ 800 ms window', () => {
    const previous = view();
    const current = view();
    const result: GameResult = { winner: 'p1', losers: ['p2'], reason: 'life_zero' };
    current.result = result;
    current.log = [...(previous.log ?? []), { sequence: 37, event: { type: 'game_over', result } }];

    const presentation = deriveGameViewPresentation(previous, current);
    const verdict = presentation.motions.find((motion) => motion.category === 'verdict');

    expect(verdict).toMatchObject({ to: 'seat:p1', durationMs: momentBudgetMs('victory') });
    expect(verdict!.durationMs).toBeLessThanOrEqual(momentCapMs('victory'));
    expect(presentation.transients).toContainEqual(
      expect.objectContaining({
        category: 'resolution',
        target: { ref: 'seat:p1' },
        accent: SCENE_HUES.gold.value,
      }),
    );
  });

  it('stages a defeat in the loss-moment family inside the ≤ 600 ms window', () => {
    const previous = view();
    const current = view();
    // A conceded game is an ordinary terminal result — no client special case.
    const result: GameResult = { winner: 'p2', losers: ['p1'], reason: 'concede' };
    current.result = result;
    current.log = [...(previous.log ?? []), { sequence: 37, event: { type: 'game_over', result } }];

    const presentation = deriveGameViewPresentation(previous, current);
    const verdict = presentation.motions.find((motion) => motion.category === 'verdict');

    expect(verdict).toMatchObject({ to: 'seat:p1', durationMs: momentBudgetMs('defeat') });
    expect(verdict!.durationMs).toBeLessThanOrEqual(momentCapMs('defeat'));
    expect(presentation.transients).toContainEqual(
      expect.objectContaining({
        category: 'death',
        target: { ref: 'seat:p1' },
        accent: SCENE_HUES.red.value,
      }),
    );
  });

  it('keeps a draw neutral and never invents a winner', () => {
    const previous = view();
    const current = view();
    const result: GameResult = { losers: ['p1', 'p2'], reason: 'life_zero' };
    current.result = result;
    current.log = [...(previous.log ?? []), { sequence: 37, event: { type: 'game_over', result } }];

    const presentation = deriveGameViewPresentation(previous, current);

    expect(presentation.motions).toContainEqual(
      expect.objectContaining({ category: 'verdict', to: 'seat:p1' }),
    );
    expect(
      presentation.transients.some(
        (effect) =>
          effect.accent === SCENE_HUES.red.value &&
          effect.target &&
          'ref' in effect.target &&
          effect.target.ref === 'seat:p1',
      ),
    ).toBe(false);
  });

  it('keeps an elimination distinct from the end of the game', () => {
    // A player leaving a multiplayer game is not a session moment: it keeps the
    // existing `player_eliminated` treatment and emits no verdict.
    const previous = view();
    const current = view();
    current.log = [
      ...(previous.log ?? []),
      { sequence: 37, event: { type: 'player_eliminated', player: 'p2', reason: 'life_zero' } },
    ];

    const presentation = deriveGameViewPresentation(previous, current);

    expect(presentation.motions.some((motion) => motion.category === 'verdict')).toBe(false);
    expect(presentation.transients).toContainEqual(
      expect.objectContaining({ category: 'death', target: { ref: 'seat:p2' } }),
    );
  });

  it('stages a receiver-less (spectator) verdict on the winner, never a defeat', () => {
    const previous = view();
    previous.you = '';
    const current = view();
    current.you = '';
    const result: GameResult = { winner: 'p2', losers: ['p1'], reason: 'decked' };
    current.result = result;
    current.log = [...(previous.log ?? []), { sequence: 37, event: { type: 'game_over', result } }];

    const presentation = deriveGameViewPresentation(previous, current);

    expect(presentation.motions).toContainEqual(
      expect.objectContaining({ category: 'verdict', to: 'seat:p2' }),
    );
    expect(presentation.transients.some((effect) => effect.category === 'death')).toBe(false);
  });

  it('keeps every session-moment intent inside the ≤ 800 ms window', () => {
    const previous = view();
    const current = view();
    const result: GameResult = { winner: 'p1', losers: ['p2'], reason: 'life_zero' };
    current.result = result;
    current.log = [
      ...(previous.log ?? []),
      { sequence: 37, event: { type: 'mulligan', player: 'p1' } },
      { sequence: 38, event: { type: 'hand_kept', player: 'p1' } },
      { sequence: 39, event: { type: 'game_over', result } },
    ];

    const presentation = deriveGameViewPresentation(previous, current);
    const moments = presentation.motions.filter((motion) =>
      ['mulligan', 'hand-kept', 'verdict'].includes(motion.category),
    );

    expect(moments).toHaveLength(3);
    for (const motion of moments) {
      expect(motion.delayMs + motion.durationMs).toBeLessThanOrEqual(SCENE_BATCH.windowMs);
    }
  });

  it('emits a session moment and an off-focus ping from the same transition', () => {
    // The two channels are independent (issues #501 and #509 landed together):
    // a wing seat acting still earns its quiet crest ping in the very transition
    // that carries a session moment, and the moment still carries its own cue.
    const previous = table();
    const current = table();
    const result: GameResult = { winner: 'p1', losers: ['p2', 'p3', 'p4'], reason: 'life_zero' };
    current.result = result;
    current.log = [
      { sequence: 40, event: { type: 'hand_kept', player: 'p1' } },
      { sequence: 41, event: { type: 'spell_cast', player: 'p3', card: { id: 'x', name: 'X' } } },
      { sequence: 42, event: { type: 'game_over', result } },
    ];

    const presentation = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    // Both session-moment intents…
    expect(presentation.motions).toContainEqual(
      expect.objectContaining({ category: 'hand-kept', from: 'hand:p1', to: 'pile:p1' }),
    );
    expect(presentation.motions).toContainEqual(
      expect.objectContaining({ category: 'verdict', to: 'seat:p1' }),
    );
    // …and the off-focus wing's ping, unshifted ahead of the batch as #501
    // requires so a cap can never drop it.
    expect(pingedSeats(presentation)).toEqual(['seat:p3']);
    expect(presentation.transients[0]?.category).toBe('off-focus-ping');
  });

  it('never credits a session moment itself as off-focus seat activity', () => {
    // A session moment already lands its own cue on the acting seat's crest;
    // crediting the channel too would double-pulse one crest in one transition.
    const previous = table();
    const current = table();
    const result: GameResult = { winner: 'p3', losers: ['p1', 'p2', 'p4'], reason: 'decked' };
    current.result = result;
    current.log = [
      { sequence: 40, event: { type: 'mulligan', player: 'p3' } },
      { sequence: 41, event: { type: 'hand_kept', player: 'p4' } },
      { sequence: 42, event: { type: 'game_over', result } },
    ];

    const presentation = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    expect(pingedSeats(presentation)).toEqual([]);
    expect(presentation.motions.map((motion) => motion.category)).toEqual(
      expect.arrayContaining(['mulligan', 'hand-kept', 'verdict']),
    );
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
    // A path with no `pending` flag is a slot the player already answered:
    // NOTE the state/endpoint fields below are the §4.4/§5 grammar (issue #535).
    // PROVISIONAL (dashed, crawl stopped), aimed at a PLAYER endpoint — the
    // classification is membership in `seat_order`, never a text reading.
    expect(result.persistent).toContainEqual({
      id: 'target:a1:target-0',
      category: 'targeting-path',
      from: { ref: 'perm_xyz' },
      to: { ref: 'seat:p2' },
      accent: '#E0784A',
      state: 'provisional',
      endpoint: 'player',
    });
  });
});

/**
 * The relationship grammar the adapter declares (issue #535, against
 * `stack-and-relationships.md` §4.3 and §5). Everything here is a **server-stated
 * pair** turned into a declared shape — no target is computed, no kind is
 * inferred from text, and the destination's kind is a membership test over the
 * view's own lists (gap G6) rather than a reading of anything.
 */
describe('deriveGameViewPresentation relationship grammar', () => {
  it('declares combat as CONFIRMED — server-stated facts are solid and static', () => {
    const current = view();
    current.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      { ...permanent('blocker', 'p2'), blocking: 'attacker' },
    ];

    const { persistent } = deriveGameViewPresentation(undefined, current);
    const attack = persistent.find((effect) => effect.id === 'attack:attacker')!;
    expect(attack.state).toBe('confirmed');
    // A defending player wears the §5.3 crest arc, not a reticle.
    expect(attack.endpoint).toBe('player');
    expect(persistent.find((effect) => effect.id === 'block:blocker')!.state).toBe('confirmed');
  });

  it('draws R9 attachment from `attached_to` in NEUTRAL line work, never a hue', () => {
    const current = view();
    current.battlefield = [
      permanent('host', 'p1'),
      { ...permanent('aura', 'p1'), attached_to: 'host' },
    ];

    const { persistent } = deriveGameViewPresentation(undefined, current);
    const bracket = persistent.find((effect) => effect.id === 'attach:aura')!;
    expect(bracket.category).toBe('attachment-bracket');
    expect(bracket.from).toEqual({ ref: 'aura' });
    expect(bracket.to).toEqual({ ref: 'host' });
    // "Belongs to" must never be confusable with "acts on", so it is not orange.
    expect(bracket.accent).not.toBe(SCENE_HUES.orange.value);
    expect(bracket.accent).toBe(SCENE_NEUTRALS.text);
  });

  it('tethers an ability on the stack back to its source permanent (R9)', () => {
    const current = view();
    current.battlefield = [permanent('engine', 'p1')];
    current.stack = [
      { id: 'ability1', controller: 'p1', description: 'Draw a card.', source: 'engine' },
      { id: 'spell1', controller: 'p1', description: 'A spell.' },
    ];

    const { persistent } = deriveGameViewPresentation(undefined, current);
    const tether = persistent.find((effect) => effect.id === 'tether:ability1')!;
    expect(tether.category).toBe('source-tether');
    expect(tether.from).toEqual({ ref: 'stack:ability1' });
    expect(tether.to).toEqual({ ref: 'engine' });
    // A spell has no source, so it gets no tether — the presence of the tether
    // IS the spell/ability discriminator the wire carries today.
    expect(persistent.some((effect) => effect.id === 'tether:spell1')).toBe(false);
  });

  it('classifies a target destination by MEMBERSHIP in the view, never by name', () => {
    const current = view();
    current.battlefield = [permanent('perm_xyz', 'p1'), permanent('victim', 'p2')];
    current.stack = [{ id: 'countered', controller: 'p2', description: 'A spell.' }];

    const { persistent } = deriveGameViewPresentation(undefined, current, {
      targetingPaths: [
        { id: 'a', from: 'perm_xyz', to: 'victim', pending: true, numeral: 1 },
        { id: 'b', from: 'perm_xyz', to: 'countered' },
        { id: 'c', from: 'perm_xyz', to: 'p2' },
      ],
    });
    const kind = (id: string): unknown => persistent.find((e) => e.id === `target:${id}`)?.endpoint;
    expect(kind('a')).toBe('card');
    expect(kind('b')).toBe('stack');
    expect(kind('c')).toBe('player');
    // The live slot crawls; the answered ones hold still. Both stay dashed.
    expect(persistent.find((e) => e.id === 'target:a')!.state).toBe('pending');
    expect(persistent.find((e) => e.id === 'target:a')!.numeral).toBe(1);
    expect(persistent.find((e) => e.id === 'target:b')!.state).toBe('provisional');
  });

  it('calms every other relationship while one object is isolated (§4.4/§9.3)', () => {
    const current = view();
    current.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      { ...permanent('other', 'p1'), attacking: true, attacking_player: 'p2' },
    ];

    const { persistent } = deriveGameViewPresentation(undefined, current, {
      isolatedId: 'attacker',
    });
    expect(persistent.find((e) => e.id === 'attack:attacker')!.state).toBe('confirmed');
    // Reduced, not removed: the relationship is never silently lost.
    expect(persistent.find((e) => e.id === 'attack:other')!.state).toBe('calmed');
    expect(persistent).toHaveLength(2);
  });

  it('lands a fizzle terminal on the STACK OBJECT, never on its target (§6.3)', () => {
    const previous = view();
    const current = view();
    current.log = [
      {
        sequence: 90,
        event: { type: 'spell_countered', player: 'p1', card: { id: 'bolt', name: 'Bolt' } },
      },
    ];

    const { transients } = deriveGameViewPresentation(previous, current);
    const terminal = transients.find((invocation) => invocation.category === 'counter')!;
    expect(terminal.target).toEqual({ ref: 'stack:bolt' });
    expect(terminal.accent).toBe(SCENE_HUES.red.value);
    // The object has left the stack in this very view, so the anchor resolves
    // through the departed-anchor freeze onto the rect it occupied.
    const frozen = freezeDepartedEffectAnchors(
      [terminal],
      new Map([['stack:bolt', { x: 10, y: 20, w: 48, h: 68 }]]),
      new Map(),
    );
    expect(frozen[0]!.target).toEqual({ rect: { x: 10, y: 20, w: 48, h: 68 } });
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

  it('credits a token swarm on a wing once, not once per token (#502)', () => {
    const previous = table();
    const current = table();
    current.battlefield = [
      ...previous.battlefield,
      ...Array.from({ length: 30 }, (_, index) => token(`spirit-${index}`, 'p3')),
    ];

    const result = deriveGameViewPresentation(previous, current, { focusSeat: 'p2' });

    // The swarm stages as a token batch, and the wing that grew it is credited
    // exactly once: the ping is per seat, never per arrival.
    expect(result.motions.filter((motion) => motion.category === 'token-batch')).toHaveLength(30);
    expect(pingedSeats(result)).toEqual(['seat:p3']);
    // Leading the batch is what keeps the guarantee: the swarm's own entry
    // transients outnumber the Lite transient cap, so a ping appended after
    // them would be dropped before it ever drew.
    expect(result.transients[0]?.category).toBe('off-focus-ping');
    expect(result.transients.length).toBeGreaterThan(TRANSIENT_CAP.lite);
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

/**
 * §6.2 / storyboard F6 — the resolution sequence, reached the way a real match
 * reaches it. Production relationship construction emits only `pending`,
 * `provisional`, and `confirmed`; when a stack entry, an attacker, a blocker, or
 * an aura leaves the next `GameView`, its relationship is simply *absent*. These
 * are the gates that the departure is staged rather than dropped, and that the
 * staging is ephemeral, interruptible presentation and nothing the view depends
 * on.
 */
describe('deriveGameViewPresentation — §6.2 departing relationships resolve', () => {
  /** A one-ability stack whose tether the next view will drop. */
  function withAbility(): { previous: GameView; current: GameView } {
    const previous = view();
    previous.battlefield = [permanent('source', 'p1')];
    previous.stack = [
      { id: 'ability', controller: 'p1', description: 'An ability.', source: 'source' },
    ];
    const current = structuredClone(previous);
    current.stack = [];
    return { previous, current };
  }

  const resolving = (result: GameViewPresentation): GameViewPresentation['persistent'] =>
    result.persistent.filter((effect) => effect.state === 'resolving');

  it('re-declares a resolved stack entry’s tether as a resolving path', () => {
    const { previous, current } = withAbility();
    const departed = resolving(deriveGameViewPresentation(previous, current));
    expect(departed).toHaveLength(1);
    expect(departed[0]).toMatchObject({
      id: 'tether:ability',
      category: 'source-tether',
      from: { ref: 'stack:ability' },
      to: { ref: 'source' },
      state: 'resolving',
    });
  });

  it('retracts a departed attack path and blocker link too', () => {
    const previous = view();
    previous.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      { ...permanent('blocker', 'p2'), blocking: 'attacker' },
    ];
    const current = view();
    current.battlefield = [permanent('attacker', 'p1'), permanent('blocker', 'p2')];
    const ids = resolving(deriveGameViewPresentation(previous, current)).map((e) => e.id);
    expect(ids.sort()).toEqual(['attack:attacker', 'block:blocker']);
  });

  it('declares nothing for a relationship the current view still states', () => {
    const previous = view();
    previous.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
    ];
    const current = structuredClone(previous);
    const result = deriveGameViewPresentation(previous, current);
    expect(resolving(result)).toHaveLength(0);
    expect(result.persistent.find((e) => e.id === 'attack:attacker')!.state).toBe('confirmed');
  });

  it('is not load-bearing: a rebuild from one view alone declares no retraction', () => {
    // §6.4 — `GameView` carries no "currently resolving" flag and must not need
    // one. Reconnect, first mount, rebuild, and fast-forward all pass
    // `previous === undefined`, and the settled stage is the whole truth.
    const { current } = withAbility();
    expect(resolving(deriveGameViewPresentation(undefined, current))).toHaveLength(0);
  });

  it('does not queue: a departure is only ever the LAST view’s departure', () => {
    // A fast sequence of views must not stack retractions. Because departures
    // are diffed against the immediately preceding view, an intent from N→N+1
    // cannot reappear in N+1→N+2 — the newest view always wins outright.
    const { previous, current } = withAbility();
    const later = structuredClone(current);
    later.turn = current.turn + 1;
    expect(resolving(deriveGameViewPresentation(previous, current))).toHaveLength(1);
    expect(resolving(deriveGameViewPresentation(current, later))).toHaveLength(0);
  });

  it('takes the reduced-motion equivalent instead of a retraction (§7.2)', () => {
    const { previous, current } = withAbility();
    const result = deriveGameViewPresentation(previous, current, { reducedMotion: true });
    expect(resolving(result)).toHaveLength(0);
  });

  it('never lets a retraction change how the standing board reads', () => {
    // The departure rides after the emphasis pass: it is not calmed by an
    // isolation and it does not count toward the crowded-board threshold.
    const previous = view();
    previous.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      { ...permanent('other', 'p1'), attacking: true, attacking_player: 'p2' },
    ];
    const current = view();
    current.battlefield = [
      { ...permanent('attacker', 'p1'), attacking: true, attacking_player: 'p2' },
      permanent('other', 'p1'),
    ];
    const result = deriveGameViewPresentation(previous, current, { isolatedId: 'attacker' });
    expect(result.persistent.find((e) => e.id === 'attack:attacker')!.state).toBe('confirmed');
    expect(result.persistent.find((e) => e.id === 'attack:other')!.state).toBe('resolving');
  });
});

describe('freezeDepartedRelationshipAnchors — a retraction needs somewhere to retract from', () => {
  const slot = { x: 10, y: 20, w: 48, h: 68 };
  const tether = {
    id: 'tether:ability',
    category: 'source-tether' as const,
    from: { ref: 'stack:ability' },
    to: { ref: 'source' },
    accent: SCENE_NEUTRALS.text,
    state: 'resolving' as const,
  };

  it('freezes a departed endpoint onto the rect it last occupied', () => {
    const [frozen] = freezeDepartedRelationshipAnchors(
      [tether],
      new Map([['stack:ability', slot]]),
      new Map([['source', { x: 200, y: 300, w: 66, h: 92 }]]),
    );
    expect(frozen!.from).toEqual({ rect: slot });
    // The endpoint that is still on the board stays LIVE, so the retraction
    // tracks it while the reconciler is mid-motion.
    expect(frozen!.to).toEqual({ ref: 'source' });
  });

  it('leaves a live relationship alone — a stale rect there would be a lie', () => {
    const confirmed = { ...tether, state: 'confirmed' as const };
    const [same] = freezeDepartedRelationshipAnchors(
      [confirmed],
      new Map([['stack:ability', slot]]),
      new Map(),
    );
    expect(same).toEqual(confirmed);
  });
});
