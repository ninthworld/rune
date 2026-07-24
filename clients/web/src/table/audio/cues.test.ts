/**
 * The intent → sound/haptic cue mapping (issue #507), exercised against real
 * presentation fixtures: every case below builds a `GameViewPresentation` with
 * the production adapter and asserts on what the hook layer derives from it, so
 * the two channels can never drift apart silently.
 */
import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameLogEntry, GameView, Permanent } from '../../protocol';
import { SCENE_BATCH } from '../../sceneTokens';
import { deriveGameViewPresentation } from '../live/gameViewPresentation';
import { deriveAudioCues } from './cues';
import { AUDIO_CUE_CATEGORIES, type AudioCue, type AudioCueCategory } from './types';

function view(): GameView {
  return structuredClone(SAMPLE_GAME_VIEW);
}

function permanent(id: string, controller = 'p1'): Permanent {
  return {
    id,
    controller,
    owner: controller,
    card: { id, name: `Card ${id}`, type_line: 'Creature', power: '1', toughness: '1' },
  };
}

/** A swarm member: identical identity and controller, so the adapter batches it. */
function token(id: string, controller = 'p1'): Permanent {
  const base = permanent(id, controller);
  return {
    ...base,
    card: { ...base.card, name: 'Spirit', functional_id: 'spirit_token', type_line: 'Creature' },
  };
}

/** Derive the cues for one transition, at the given staging. */
function cuesFor(
  previous: GameView,
  current: GameView,
  staging: Parameters<typeof deriveGameViewPresentation>[2] = {},
): AudioCue[] {
  return deriveAudioCues(deriveGameViewPresentation(previous, current, staging));
}

/** Append log entries to a copy of the base frame. */
function withLog(base: GameView, ...events: GameLogEntry['event'][]): GameView {
  const next = structuredClone(base);
  const start = Math.max(0, ...(base.log ?? []).map((entry) => entry.sequence));
  next.log = [
    ...(base.log ?? []),
    ...events.map((event, index) => ({ sequence: start + index + 1, event })),
  ];
  return next;
}

/** The one cue of a category, asserted to exist exactly once. */
function only(cues: readonly AudioCue[], category: AudioCueCategory): AudioCue {
  const matches = cues.filter((cue) => cue.category === category);
  expect(matches).toHaveLength(1);
  return matches[0]!;
}

describe('deriveAudioCues — the visual-system §9 taxonomy', () => {
  it('invokes cast, resolve, impact, destroy, draw, and phase from log events', () => {
    const previous = view();
    const current = withLog(
      previous,
      { type: 'cards_drawn', player: 'p1', count: 2 },
      { type: 'spell_cast', player: 'p1', card: { id: 'spell', name: 'Spell' } },
      { type: 'spell_resolved', player: 'p1', card: { id: 'spell', name: 'Spell' } },
      { type: 'damage_dealt', target: { kind: 'player', player: 'p2' }, amount: 5 },
      { type: 'permanent_died', permanent: { id: 'perm_xyz', name: 'Grizzly Bears' } },
      { type: 'step_changed', turn: 1, active_player: 'p1', phase: 'combat_damage' },
    );

    const cues = cuesFor(previous, current);
    const byCategory = new Map(cues.map((cue) => [cue.category, cue]));

    expect([...byCategory.keys()].sort()).toEqual(
      ['cast', 'destroy', 'draw', 'impact', 'phase', 'resolve'].sort(),
    );
    // Params ride along: the drawing seat and the count, the damaged seat and
    // the amount — all values the client already had, no rules claim added.
    expect(byCategory.get('draw')).toMatchObject({ seat: 'p1', magnitude: 2, count: 1 });
    expect(byCategory.get('impact')).toMatchObject({ seat: 'p2', magnitude: 5 });
    expect(byCategory.get('destroy')).toMatchObject({ seat: 'p1' });
  });

  it('invokes tap from the view diff', () => {
    const previous = view();
    previous.battlefield = [permanent('perm_xyz')];
    const current = structuredClone(previous);
    current.battlefield = [{ ...permanent('perm_xyz'), tapped: true }];

    expect(only(cuesFor(previous, current), 'tap')).toMatchObject({ count: 1 });
  });

  it('collapses tap and untap onto the single tap category', () => {
    const previous = view();
    previous.battlefield = [permanent('a'), { ...permanent('b'), tapped: true }];
    const current = structuredClone(previous);
    current.battlefield = [{ ...permanent('a'), tapped: true }, permanent('b')];

    expect(only(cuesFor(previous, current), 'tap').count).toBe(2);
  });

  it('invokes priority when priority moves', () => {
    const previous = view();
    previous.priority_player = 'p1';
    const current = structuredClone(previous);
    current.priority_player = 'p2';

    expect(only(cuesFor(previous, current), 'priority')).toMatchObject({ seat: 'p2' });
  });

  it('invokes phase for both a step change and a turn change', () => {
    const previous = view();
    previous.phase = 'precombat_main';
    previous.turn = 3;
    const current = structuredClone(previous);
    current.phase = 'combat_damage';
    current.turn = 4;

    // Two motion classes (phase, turn), one taxonomy category, one sound.
    expect(only(cuesFor(previous, current), 'phase').count).toBe(2);
  });

  it('invokes play for a card entering the battlefield from hand', () => {
    const previous = view();
    const current = structuredClone(previous);
    current.my_hand = [];
    current.battlefield = [...previous.battlefield, permanent('c1')];

    expect(only(cuesFor(previous, current), 'play')).toMatchObject({ count: 1 });
  });

  it('invokes victory from game_over, crediting the winner', () => {
    const previous = view();
    const current = withLog(previous, {
      type: 'game_over',
      result: { winner: 'p2', losers: ['p1'], reason: 'life_zero' },
    });

    expect(only(cuesFor(previous, current), 'victory')).toMatchObject({ seat: 'p2', count: 1 });
  });

  it('invokes victory with no seat for a draw', () => {
    const previous = view();
    const current = withLog(previous, {
      type: 'game_over',
      result: { losers: ['p1', 'p2'], reason: 'life_zero' },
    });

    expect(only(cuesFor(previous, current), 'victory').seat).toBeUndefined();
  });

  it('gives an elimination the destruction cue', () => {
    const previous = view();
    const current = withLog(previous, {
      type: 'player_eliminated',
      player: 'p2',
      reason: 'concede',
    });

    expect(only(cuesFor(previous, current), 'destroy')).toMatchObject({ seat: 'p2' });
  });

  it('never sounds the client’s own camera move', () => {
    const previous = view();
    const current = structuredClone(previous);

    // A focus change is a staging cue, not a game event: the plane moved, the
    // server said nothing. It must never be narrated.
    expect(cuesFor(previous, current, { previousFocusSeat: 'p2', focusSeat: 'p3' })).toEqual([]);
  });

  it('derives nothing at all from a first mount', () => {
    expect(deriveAudioCues(deriveGameViewPresentation(undefined, view()))).toEqual([]);
  });

  it('maps every taxonomy category from some presentation input', () => {
    // A guard against a category being declared and then never reachable.
    const reachable = new Set<AudioCueCategory>();
    const previous = view();
    previous.battlefield = [permanent('perm_xyz')];
    previous.priority_player = 'p1';
    const current = withLog(
      { ...structuredClone(previous), priority_player: 'p2', phase: 'combat_damage' },
      { type: 'cards_drawn', player: 'p1', count: 1 },
      { type: 'spell_cast', player: 'p1', card: { id: 's', name: 'S' } },
      { type: 'spell_resolved', player: 'p1', card: { id: 's', name: 'S' } },
      { type: 'damage_dealt', target: { kind: 'player', player: 'p2' }, amount: 3 },
      { type: 'permanent_died', permanent: { id: 'perm_xyz', name: 'X' } },
      { type: 'step_changed', turn: 2, active_player: 'p1', phase: 'combat_damage' },
      { type: 'game_over', result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' } },
    );
    current.my_hand = [];
    current.battlefield = [{ ...permanent('perm_xyz'), tapped: true }, permanent('c1')];

    for (const cue of cuesFor(previous, current)) reachable.add(cue.category);
    expect([...reachable].sort()).toEqual([...AUDIO_CUE_CATEGORIES].sort());
  });
});

describe('deriveAudioCues — batch collapse mirrors the visual stagger budget', () => {
  it('makes a thirty-token swarm exactly one sound', () => {
    const previous = view();
    previous.battlefield = [];
    const current = structuredClone(previous);
    current.battlefield = Array.from({ length: 30 }, (_, index) => token(`t${index}`));

    const presentation = deriveGameViewPresentation(previous, current, { quality: 'high' });
    // The visual channel really does stage thirty separate arrivals…
    expect(presentation.motions.filter((m) => m.category === 'token-batch')).toHaveLength(30);

    // …and the audio channel makes exactly one sound for the batch window.
    const cues = deriveAudioCues(presentation);
    const play = only(cues, 'play');
    expect(play.count).toBe(30);
    // It lands on the leading edge of the batch, not trailing the last token.
    expect(play.delayMs).toBe(0);
  });

  it('never emits more than one cue per category per window', () => {
    const previous = view();
    previous.battlefield = [];
    const current = structuredClone(previous);
    current.battlefield = Array.from({ length: 12 }, (_, index) => permanent(`d${index}`));

    const cues = deriveAudioCues(
      deriveGameViewPresentation(previous, current, { quality: 'high' }),
    );
    const perCategory = new Map<AudioCueCategory, number>();
    for (const cue of cues) perCategory.set(cue.category, (perCategory.get(cue.category) ?? 0) + 1);
    for (const count of perCategory.values()) expect(count).toBe(1);
  });

  it('keeps every stagger inside the batch window it collapses on', () => {
    const previous = view();
    previous.battlefield = [];
    const current = structuredClone(previous);
    current.battlefield = Array.from({ length: 30 }, (_, index) => token(`t${index}`));

    const presentation = deriveGameViewPresentation(previous, current, { quality: 'high' });
    for (const motion of presentation.motions) {
      expect(motion.delayMs).toBeLessThanOrEqual(SCENE_BATCH.windowMs);
    }
  });
});

describe('deriveAudioCues — reduced motion and audio are independent channels', () => {
  it('derives the same cue categories with reduced motion on', () => {
    const previous = view();
    previous.battlefield = [];
    const base = structuredClone(previous);
    base.battlefield = Array.from({ length: 6 }, (_, index) => token(`t${index}`));
    const current = withLog(
      base,
      { type: 'cards_drawn', player: 'p1', count: 1 },
      { type: 'permanent_died', permanent: { id: 'gone', name: 'Gone' } },
    );

    const full = cuesFor(previous, current, { quality: 'high', reducedMotion: false });
    const reduced = cuesFor(previous, current, { quality: 'high', reducedMotion: true });

    expect(reduced.map((cue) => cue.category)).toEqual(full.map((cue) => cue.category));
    expect(reduced.map((cue) => cue.count)).toEqual(full.map((cue) => cue.count));
    // Reduced motion only zeroes the visual stagger; it never removes a sound.
    expect(reduced.every((cue) => cue.delayMs === 0)).toBe(true);
  });

  it('derives the same cue categories at Lite quality', () => {
    const previous = view();
    previous.battlefield = [];
    const current = structuredClone(previous);
    current.battlefield = Array.from({ length: 4 }, (_, index) => token(`t${index}`));

    expect(cuesFor(previous, current, { quality: 'lite' }).map((cue) => cue.category)).toEqual(
      cuesFor(previous, current, { quality: 'high' }).map((cue) => cue.category),
    );
  });
});
