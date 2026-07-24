import { describe, expect, it } from 'vitest';
import { cardFaceRenderer } from '../table/planeFaceRenderer';
import { PlaneReconciler, planeRenders } from '../table/planeReconciler';
import { stagePlane } from '../table/plane';
import { toDisplayData } from '../table/scene/card-helpers';
import { fixtureBudget } from './metrics';
import { FIXTURE_SCENARIOS, normalizeFixture } from './scenarios';

describe('2.5D fixture scenarios', () => {
  it('covers the complete layout-model scenario set', () => {
    expect(FIXTURE_SCENARIOS.map((scenario) => scenario.id)).toEqual([
      'commander4',
      'duel',
      'three',
      'five',
      'six',
      'ultrawide',
      'tablet',
      'tokens',
      'big-hand',
      'combat-web',
      'deep-stack',
      'phone',
    ]);
    expect(FIXTURE_SCENARIOS[0]?.frames.map((frame) => frame.label)).toEqual([
      'Opening composition',
      'Draw to hand',
      'Play to battlefield',
      'Tap and settle',
      'Focus Ember Reach',
      'Combat paths',
      'Resolution and impact',
    ]);
  });

  it('round-trips every frame as a complete normalized GameView', () => {
    for (const scenario of FIXTURE_SCENARIOS) {
      for (const frame of scenario.frames) {
        expect(normalizeFixture(frame.view)).toEqual(frame.view);
      }
    }
  });

  it('rebuilds every scenario through the real plane and CardFace stack inside budget', () => {
    for (const scenario of FIXTURE_SCENARIOS) {
      const frame = scenario.frames[0]!;
      const plane = stagePlane(frame.view, scenario.viewport, frame.staging);
      const root = document.createElement('div');
      const renderer = cardFaceRenderer((render) => {
        const permanent = frame.view.battlefield.find((entry) => entry.id === render.entityId);
        return permanent
          ? toDisplayData(permanent.card, {
              tapped: permanent.tapped,
              counters: permanent.counters,
              selected: false,
              actionable: false,
              attacking: permanent.attacking,
              attackingPlayer: permanent.attacking_player,
              blocking: permanent.blocking !== undefined,
              markedDamage: permanent.damage,
            })
          : {
              name: render.name,
              typeLine: 'Permanent',
              colorIdentity: 'C',
            };
      });
      const reconciler = new PlaneReconciler(root, { face: renderer });

      // Warm the renderer before measuring the reconnect/full-rebuild path.
      // This measures JS execution of the rebuild in jsdom (no real layout), so the
      // number tracks CI-runner CPU availability, not the presentation budget itself
      // (real rebuild timing is the browser harness's job, #499). Take the best of a
      // wide sample so a contended-runner slice can't flake the heaviest scenario at
      // the budget boundary; the ≤50 ms budget it checks is unchanged.
      reconciler.rebuild(plane);
      const rebuildMs = Math.min(
        ...Array.from({ length: 15 }, () => {
          const started = performance.now();
          reconciler.rebuild(plane);
          return performance.now() - started;
        }),
      );
      const compact = scenario.viewport.height > scenario.viewport.width;

      expect(rebuildMs, `${scenario.id} rebuild`).toBeLessThanOrEqual(
        fixtureBudget('standard', compact).maxRebuildMs,
      );
      expect(root.querySelectorAll('*').length, `${scenario.id} DOM`).toBeLessThanOrEqual(
        fixtureBudget('standard', compact).maxDomNodes,
      );
      expect(
        root.querySelectorAll('[data-entity-id]').length,
        `${scenario.id} staged entities`,
      ).toBe(planeRenders(plane).length);
    }
  });
});
