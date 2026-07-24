import { describe, expect, it } from 'vitest';
import {
  FrameBudgetSampler,
  fixtureBudget,
  fixtureBudgetReport,
  maxCardFaceNodes,
  summarizeFrameDeltas,
} from './metrics';

describe('fixture presentation budgets', () => {
  it('applies the documented desktop, phone, and Lite limits', () => {
    expect(fixtureBudget('standard', false)).toEqual({
      minFps: 60,
      maxP95Ms: 16.7,
      maxRebuildMs: 50,
      maxDomNodes: 15_000,
      maxFaceNodes: 12,
    });
    expect(fixtureBudget('lite', true)).toEqual({
      minFps: 30,
      maxP95Ms: 33.3,
      maxRebuildMs: 100,
      maxDomNodes: 15_000,
      maxFaceNodes: 12,
    });
  });

  it('reports the largest staged card face, ignoring empty wrappers', () => {
    const root = document.createElement('div');
    root.innerHTML =
      '<div data-entity-id="a"><div><span></span></div></div>' +
      '<div data-entity-id="b"><div><span><i></i></span><span></span></div></div>' +
      '<div data-entity-id="c"></div>';
    // The deepest face is root + 2 spans + 1 nested <i> = 4 elements.
    expect(maxCardFaceNodes(root)).toBe(4);
    expect(maxCardFaceNodes(document.createElement('div'))).toBe(0);
  });

  it('summarizes controlled RAF deltas without a wall clock', () => {
    const summary = summarizeFrameDeltas([16, 17, 16, 18, 16]);
    expect(summary.samples).toBe(5);
    expect(summary.fps).toBeCloseTo(60.24, 1);
    expect(summary.p95Ms).toBe(18);
  });

  it('keeps independent bounded idle and tween samples', () => {
    const sampler = new FrameBudgetSampler();
    sampler.sample(0, 'idle');
    sampler.sample(16.6, 'idle');
    sampler.sample(33.2, 'tween');
    sampler.sample(49.8, 'tween');
    expect(sampler.idleSummary().samples).toBe(1);
    expect(sampler.tweenSummary().samples).toBe(2);
    sampler.reset();
    expect(sampler.idleSummary().samples).toBe(0);
    expect(sampler.tweenSummary().samples).toBe(0);
  });

  it('fails the aggregate report when any binding budget is exceeded', () => {
    const passing = fixtureBudgetReport({
      scenario: 'commander4',
      quality: 'standard',
      compact: false,
      idle: { samples: 60, fps: 60.1, p95Ms: 16.7 },
      tween: { samples: 60, fps: 60, p95Ms: 16.6 },
      rebuildMs: 12,
      domNodes: 1200,
      faceNodes: 11,
    });
    expect(passing.passes).toBe(true);

    expect(
      fixtureBudgetReport({
        ...passing,
        domNodes: 15_001,
      }).passes,
    ).toBe(false);
    expect(
      fixtureBudgetReport({
        ...passing,
        faceNodes: 13,
      }).passes,
    ).toBe(false);
  });
});
