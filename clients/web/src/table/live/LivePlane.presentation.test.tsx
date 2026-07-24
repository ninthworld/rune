/**
 * Presentation-mode behavior of the live plane (issue #493): reconnect/resync
 * rebuilds, rapid-update fast-forward, the post-rebuild "you are here" cue, and
 * the budget instrumentation. The effects layer is mocked to a call recorder so
 * transient/persistent traffic is assertable without a WebGL context.
 */
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView } from '../../protocol';
import type { RebuildSample } from './presentationMode';
import { LivePlane } from './LivePlane';

const effects = vi.hoisted(() => ({
  transients: [] as unknown[][],
  persistent: [] as unknown[][],
  rects: undefined as
    undefined | ((ref: string) => { x: number; y: number; w: number; h: number } | undefined),
}));

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    constructor(options: { rects: typeof effects.rects }) {
      effects.rects = options.rects;
    }
    setPersistent(next: unknown[]): void {
      effects.persistent.push(next);
    }
    replaceTransients(next: unknown[]): void {
      effects.transients.push(next);
    }
    trackMotion(): void {}
  },
}));

/** A fresh view object with the same content — a distinct reference to reconcile. */
const clone = (view: GameView): GameView => ({ ...view });

/** SAMPLE with one extra untapped permanent, which enters (spawns motion). */
const withExtraPermanent = (): GameView => {
  const base = SAMPLE_GAME_VIEW.battlefield[0]!;
  return {
    ...SAMPLE_GAME_VIEW,
    battlefield: [
      ...SAMPLE_GAME_VIEW.battlefield,
      { ...base, id: 'perm_new', card: { ...base.card, id: 'perm_new' }, tapped: false },
    ],
  };
};

describe('LivePlane presentation modes', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
    effects.transients = [];
    effects.persistent = [];
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    effects.rects = undefined;
  });

  it('rebuilds on a session-epoch discontinuity and lands one "you are here" pulse', () => {
    const onMode = vi.fn();
    const samples: RebuildSample[] = [];
    const { rerender } = render(
      <LivePlane
        view={SAMPLE_GAME_VIEW}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        sessionEpoch={1}
        onMode={onMode}
        onRebuild={(sample) => samples.push(sample)}
      />,
    );
    onMode.mockClear();
    effects.transients = [];

    // A reconnect: the same seat, a newer transport generation, a fresh view.
    rerender(
      <LivePlane
        view={clone(SAMPLE_GAME_VIEW)}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        sessionEpoch={2}
        onMode={onMode}
        onRebuild={(sample) => samples.push(sample)}
      />,
    );

    expect(onMode).toHaveBeenCalledWith('rebuild');
    // The complete board is rebuilt from the one latest view.
    expect(document.querySelectorAll('[data-slot="region"]')).toHaveLength(2);
    // The single non-blocking cue on the active crest.
    expect(effects.transients.at(-1)).toEqual([
      { category: 'flow', target: { ref: 'seat:p1' }, accent: '#F2C94C' },
    ]);
    const rebuild = samples.find((s) => s.mode === 'rebuild');
    expect(rebuild).toBeDefined();
    expect(rebuild!.domNodes).toBeGreaterThan(0);
    expect(rebuild!.withinDomCeiling).toBe(true);
    expect(rebuild!.withinBudget).toBe(true);
  });

  it('rebuilds under reduced motion with no pulse, from the latest view alone', () => {
    const onMode = vi.fn();
    const { rerender } = render(
      <LivePlane
        view={SAMPLE_GAME_VIEW}
        quality="standard"
        density="reduced"
        reducedMotion
        sessionEpoch={1}
        onMode={onMode}
      />,
    );
    onMode.mockClear();
    effects.transients = [];

    rerender(
      <LivePlane
        view={clone(SAMPLE_GAME_VIEW)}
        quality="standard"
        density="reduced"
        reducedMotion
        sessionEpoch={2}
        onMode={onMode}
      />,
    );

    expect(onMode).toHaveBeenCalledWith('rebuild');
    expect(document.querySelectorAll('[data-slot="region"]')).toHaveLength(2);
    // Reduced motion receives the complete final state with no orientation pulse.
    expect(effects.transients.at(-1)).toEqual([]);
  });

  it('collapses a burst to the newest view and discards obsolete transient work', () => {
    const onMode = vi.fn();
    const { rerender } = render(
      <LivePlane
        view={SAMPLE_GAME_VIEW}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        sessionEpoch={1}
        onMode={onMode}
      />,
    );

    // A view that adds an entering permanent leaves a transition in flight
    // (rAF is stubbed, so nothing advances it to completion).
    rerender(
      <LivePlane
        view={withExtraPermanent()}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        sessionEpoch={1}
        onMode={onMode}
      />,
    );
    expect(document.querySelector('[data-entity-id="perm_new"]')).not.toBeNull();
    onMode.mockClear();
    effects.transients = [];

    // A newer view arrives before that settles — collapse to it.
    rerender(
      <LivePlane
        view={clone(SAMPLE_GAME_VIEW)}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        sessionEpoch={1}
        onMode={onMode}
      />,
    );

    expect(onMode).toHaveBeenCalledWith('fast-forward');
    // Settled on the newest view: the entering permanent is gone, no ghost left.
    expect(document.querySelector('[data-entity-id="perm_new"]')).toBeNull();
    expect(document.querySelectorAll('[data-ghost]')).toHaveLength(0);
    // Obsolete transient work is discarded on the collapse.
    expect(effects.transients.at(-1)).toEqual([]);
  });

  it('reports no rebuild sample for an ordinary in-session reconcile', () => {
    const onMode = vi.fn();
    const onRebuild = vi.fn();
    const { rerender } = render(
      <LivePlane
        view={SAMPLE_GAME_VIEW}
        quality="standard"
        density="reduced"
        reducedMotion
        sessionEpoch={1}
        onMode={onMode}
        onRebuild={onRebuild}
      />,
    );
    onRebuild.mockClear();
    onMode.mockClear();

    rerender(
      <LivePlane
        view={clone(SAMPLE_GAME_VIEW)}
        quality="standard"
        density="reduced"
        reducedMotion
        sessionEpoch={1}
        onMode={onMode}
        onRebuild={onRebuild}
      />,
    );

    expect(onMode).toHaveBeenCalledWith('reconcile');
    expect(onRebuild).not.toHaveBeenCalled();
  });
});
