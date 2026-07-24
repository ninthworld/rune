import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import {
  determinePresentationMode,
  isFullRebuild,
  orientationCue,
  rebuildBudgetMs,
  REBUILD_BUDGET_MS,
  SCENE_DOM_CEILING,
  ORIENTATION_PULSE_MS,
} from './presentationMode';

describe('determinePresentationMode', () => {
  it('is initial before any view has been presented', () => {
    expect(
      determinePresentationMode({
        hasPreviousView: false,
        discontinuity: true,
        presentationBusy: true,
      }),
    ).toBe('initial');
  });

  it('is a reconcile for an ordinary in-session update with the composition idle', () => {
    expect(
      determinePresentationMode({
        hasPreviousView: true,
        discontinuity: false,
        presentationBusy: false,
      }),
    ).toBe('reconcile');
  });

  it('is a rebuild on a reconnect/resync discontinuity', () => {
    expect(
      determinePresentationMode({
        hasPreviousView: true,
        discontinuity: true,
        presentationBusy: false,
      }),
    ).toBe('rebuild');
  });

  it('rebuilds a discontinuity even while a prior transition is still in flight', () => {
    // A reconnect frame must never be reconciled onto pre-disconnect motion.
    expect(
      determinePresentationMode({
        hasPreviousView: true,
        discontinuity: true,
        presentationBusy: true,
      }),
    ).toBe('rebuild');
  });

  it('fast-forwards when a newer view outruns the prior transition', () => {
    expect(
      determinePresentationMode({
        hasPreviousView: true,
        discontinuity: false,
        presentationBusy: true,
      }),
    ).toBe('fast-forward');
  });
});

describe('isFullRebuild', () => {
  it('is true for the modes that mount the scene from the latest view alone', () => {
    expect(isFullRebuild('initial')).toBe(true);
    expect(isFullRebuild('rebuild')).toBe(true);
    expect(isFullRebuild('reconcile')).toBe(false);
    expect(isFullRebuild('fast-forward')).toBe(false);
  });
});

describe('rebuild budgets', () => {
  it('exposes the binding presentation-budget numbers', () => {
    expect(REBUILD_BUDGET_MS).toEqual({ desktop: 50, compact: 100 });
    expect(SCENE_DOM_CEILING).toBe(15_000);
    expect(ORIENTATION_PULSE_MS).toBeLessThanOrEqual(300);
  });

  it('picks the looser ceiling for the compact composition', () => {
    expect(rebuildBudgetMs(false)).toBe(50);
    expect(rebuildBudgetMs(true)).toBe(100);
  });
});

describe('orientationCue', () => {
  it('lands one flow pulse on the active crest after an animated rebuild', () => {
    const cue = orientationCue(SAMPLE_GAME_VIEW, false);
    expect(cue).toEqual([
      {
        category: 'flow',
        target: { ref: `seat:${SAMPLE_GAME_VIEW.active_player}` },
        accent: '#F2C94C',
      },
    ]);
  });

  it('adds no pulse under reduced motion — the complete final state only', () => {
    expect(orientationCue(SAMPLE_GAME_VIEW, true)).toEqual([]);
  });
});
