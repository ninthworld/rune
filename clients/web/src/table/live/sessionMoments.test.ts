/**
 * The §8 "Session moments" vocabulary (issue #509): budgets, skippability,
 * reduced-motion collapse, the verdict classification, and which transition is
 * which moment. These are the numbers `docs/design/visual-system.md` §8 states,
 * pinned here so a drift fails CI rather than a review eye.
 */
import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameResult, GameView } from '../../protocol';
import { SCENE_HUES, SCENE_SESSION, SCENE_SKIP_THRESHOLD_MS } from '../../sceneTokens';
import { lastMatchOf } from '../../store';
import type { PresentationMode } from './presentationMode';
import {
  demandsSkip,
  entryMoment,
  isSkippable,
  momentAccent,
  momentBudgetMs,
  momentCapMs,
  momentDurationMs,
  verdictMoment,
  type SessionMoment,
} from './sessionMoments';

const ALL: SessionMoment[] = [
  'game-start',
  'mulligan',
  'hand-kept',
  'reconnect',
  'victory',
  'defeat',
  'draw',
  'return-to-lobby',
];

describe('session moments — §8 budgets', () => {
  it('states the documented windows for every row', () => {
    // visual-system §8 "Session moments", read straight off the table.
    expect(momentBudgetMs('game-start')).toBe(800);
    expect(momentBudgetMs('reconnect')).toBe(300);
    expect(momentBudgetMs('defeat')).toBe(600);
    expect(momentBudgetMs('victory')).toBe(800);
    expect(momentBudgetMs('return-to-lobby')).toBe(400);
    // The mulligan pair rides the zone-travel budget ("within travel budgets").
    expect(momentBudgetMs('mulligan')).toBeLessThanOrEqual(400);
    expect(momentBudgetMs('hand-kept')).toBeLessThanOrEqual(400);
  });

  it('keeps every moment inside its cap', () => {
    for (const moment of ALL) {
      expect(momentBudgetMs(moment)).toBeLessThanOrEqual(momentCapMs(moment));
    }
    for (const spec of Object.values(SCENE_SESSION)) {
      expect(spec.ms).toBeLessThanOrEqual(spec.cap);
    }
  });

  it('marks every moment that may compose past the skip threshold as skippable', () => {
    // §8: "No unmarked row may compose past 600 ms." A row may still be marked
    // below the threshold (the mulligan's composed sweep+deal is), but none
    // above it may be left unskippable.
    for (const moment of ALL) {
      if (demandsSkip(momentCapMs(moment))) expect(isSkippable(moment)).toBe(true);
    }
    expect(demandsSkip(SCENE_SKIP_THRESHOLD_MS)).toBe(false);
    expect(demandsSkip(SCENE_SKIP_THRESHOLD_MS + 1)).toBe(true);
    // The rows §8 marks explicitly.
    expect(isSkippable('game-start')).toBe(true);
    expect(isSkippable('victory')).toBe(true);
    expect(isSkippable('mulligan')).toBe(true);
    // …and the ones it does not: they are shorter than a deliberate skip.
    expect(isSkippable('defeat')).toBe(false);
    expect(isSkippable('reconnect')).toBe(false);
    expect(isSkippable('return-to-lobby')).toBe(false);
  });

  it('collapses every moment to zero under reduced motion', () => {
    for (const moment of ALL) {
      expect(momentDurationMs(moment, true)).toBe(0);
      expect(momentDurationMs(moment, false)).toBe(momentBudgetMs(moment));
    }
  });

  it('keeps the verdict accents inside the §2 semantic families (no confetti)', () => {
    // Restraint is a requirement: the loss moment wears the loss family, the
    // victory bloom the disciplined gold, and a draw stays neutral.
    expect(momentAccent('defeat')).toBe(SCENE_HUES.red.value);
    expect(momentAccent('victory')).toBe(SCENE_HUES.gold.value);
    expect(momentAccent('draw')).not.toBe(SCENE_HUES.red.value);
    expect(momentAccent('draw')).not.toBe(SCENE_HUES.gold.value);
  });
});

describe('session moments — verdict classification', () => {
  const win: GameResult = { winner: 'p1', losers: ['p2'], reason: 'life_zero' };
  const concede: GameResult = { winner: 'p2', losers: ['p1'], reason: 'concede' };
  const drawn: GameResult = { losers: ['p1', 'p2'], reason: 'life_zero' };

  it('stages a victory only for the seat that actually won', () => {
    expect(verdictMoment(win, 'p1')).toBe('victory');
    expect(verdictMoment(win, 'p2')).toBe('defeat');
  });

  it('treats a concede as an ordinary terminal result (no special case)', () => {
    // Concede is a normal `valid_actions` entry; the client only reads the
    // result the server decided, exactly as it does for any other reason.
    expect(verdictMoment(concede, 'p1')).toBe('defeat');
    expect(verdictMoment(concede, 'p2')).toBe('victory');
  });

  it('stages a draw neutrally (CR 104.4a — no winner)', () => {
    expect(verdictMoment(drawn, 'p1')).toBe('draw');
  });

  it('never stages someone else’s verdict as a spectator’s own', () => {
    // A receiver-less view (`you: ''`) is still told who won by the panel; it
    // just does not wear a personal victory or defeat.
    expect(verdictMoment(win, '')).toBe('draw');
    expect(verdictMoment(drawn, '')).toBe('draw');
  });
});

describe('session moments — agreement with the last-match ribbon (issue #506)', () => {
  // `verdictMoment` (the staged verdict) and `lastMatchOf` (the lobby ribbon)
  // both classify victory/defeat/draw from the same `result` + `you`. They are
  // shown one after the other across the same exit, so a disagreement would be
  // a visible contradiction: "Victory" on the panel, "Defeat" on the ribbon.
  const seated = (you: string, result: GameResult): GameView => {
    const view = structuredClone(SAMPLE_GAME_VIEW);
    return { ...view, you, seat_order: ['p1', 'p2'], result };
  };

  const results: GameResult[] = [
    { winner: 'p1', losers: ['p2'], reason: 'life_zero' },
    { winner: 'p2', losers: ['p1'], reason: 'concede' },
    { winner: 'p1', losers: ['p2'], reason: 'decked' },
    { losers: ['p1', 'p2'], reason: 'life_zero' },
  ];

  it('classifies every seated outcome identically to lastMatchOf', () => {
    for (const result of results) {
      for (const you of ['p1', 'p2']) {
        const summary = lastMatchOf(seated(you, result), null);
        expect(summary).not.toBeNull();
        expect(summary!.outcome).toBe(verdictMoment(result, you));
      }
    }
  });

  it('leaves the receiver-less case outside the ribbon’s domain entirely', () => {
    // The only input on which the two classifiers *could* differ is `you: ''`,
    // where the verdict stays neutral rather than staging someone else's loss.
    // A spectator holds no `view`, so `lastMatchOf` reports nothing at all —
    // the divergence is unreachable rather than merely unlikely.
    expect(verdictMoment(results[0]!, '')).toBe('draw');
    expect(lastMatchOf(null, null)).toBeNull();
  });
});

describe('session moments — entry classification', () => {
  it('maps the presentation modes to their entry moments', () => {
    expect(entryMoment('initial')).toBe('game-start');
    expect(entryMoment('rebuild')).toBe('reconnect');
  });

  it('stages nothing for ordinary play', () => {
    const quiet: PresentationMode[] = ['reconcile', 'fast-forward'];
    for (const mode of quiet) expect(entryMoment(mode)).toBeNull();
  });
});
