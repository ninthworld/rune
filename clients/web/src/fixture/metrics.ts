/**
 * CI-runnable presentation-budget measurements for the fixture battlefield.
 *
 * The browser harness feeds real `requestAnimationFrame` deltas into
 * {@link FrameBudgetSampler}; unit tests feed controlled timestamps through the
 * same code. Rebuild time and DOM counts are sampled directly by the harness.
 */
import type { EffectQuality } from '../table/effects';

/** One measured animation mode. */
export type FrameMode = 'idle' | 'tween';

/** The budget values that apply to one fixture run. */
export interface FixtureBudget {
  /** Minimum sustained frames per second. */
  minFps: number;
  /** Maximum p95 frame time in milliseconds. */
  maxP95Ms: number;
  /** Maximum reconnect/full-rebuild time in milliseconds. */
  maxRebuildMs: number;
  /** Maximum scene DOM nodes. */
  maxDomNodes: number;
}

/** One summarized frame sample set. */
export interface FrameSummary {
  /** Frames represented by the sample. */
  samples: number;
  /** Mean frames per second. */
  fps: number;
  /** 95th-percentile frame time in milliseconds. */
  p95Ms: number;
}

/** The complete live report published by the fixture route. */
export interface FixtureBudgetReport {
  /** Scenario id under measurement. */
  scenario: string;
  /** Current quality selection. */
  quality: EffectQuality;
  /** Whether phone/tablet rebuild limits apply. */
  compact: boolean;
  /** Frame measurements while no transition was live. */
  idle: FrameSummary;
  /** Frame measurements while a reconciler transition was live. */
  tween: FrameSummary;
  /** Most recent full-scene rebuild duration. */
  rebuildMs: number;
  /** Current document DOM node count. */
  domNodes: number;
  /** Resolved limits for this run. */
  budget: FixtureBudget;
  /** Whether every measurement with enough samples is within budget. */
  passes: boolean;
}

const MAX_SAMPLES = 240;

/** Resolve presentation-budgets.md limits for a quality/device class. */
export function fixtureBudget(quality: EffectQuality, compact: boolean): FixtureBudget {
  const lite = quality === 'lite';
  return {
    minFps: lite ? 30 : 60,
    maxP95Ms: lite ? 33.3 : 16.7,
    maxRebuildMs: compact ? 100 : 50,
    maxDomNodes: 15_000,
  };
}

/** Summarize frame-to-frame deltas. Empty and single-frame sets stay neutral. */
export function summarizeFrameDeltas(deltas: readonly number[]): FrameSummary {
  if (deltas.length === 0) return { samples: 0, fps: 0, p95Ms: 0 };
  const sorted = [...deltas].sort((a, b) => a - b);
  const total = deltas.reduce((sum, value) => sum + value, 0);
  const mean = total / deltas.length;
  const p95At = Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1);
  return {
    samples: deltas.length,
    fps: mean <= 0 ? 0 : 1000 / mean,
    p95Ms: sorted[p95At] ?? 0,
  };
}

/** Whether a frame summary has enough evidence and satisfies its limits. */
export function frameSummaryPasses(summary: FrameSummary, budget: FixtureBudget): boolean {
  if (summary.samples < 2) return true;
  return summary.fps >= budget.minFps - 0.5 && summary.p95Ms <= budget.maxP95Ms + 0.2;
}

/** Assemble the public report and its aggregate pass/fail result. */
export function fixtureBudgetReport(input: {
  scenario: string;
  quality: EffectQuality;
  compact: boolean;
  idle: FrameSummary;
  tween: FrameSummary;
  rebuildMs: number;
  domNodes: number;
}): FixtureBudgetReport {
  const budget = fixtureBudget(input.quality, input.compact);
  return {
    ...input,
    budget,
    passes:
      frameSummaryPasses(input.idle, budget) &&
      frameSummaryPasses(input.tween, budget) &&
      input.rebuildMs <= budget.maxRebuildMs &&
      input.domNodes <= budget.maxDomNodes,
  };
}

/** Collect bounded idle/tween RAF deltas and produce stable summaries. */
export class FrameBudgetSampler {
  private last?: number;
  private readonly idle: number[] = [];
  private readonly tween: number[] = [];

  /** Add one RAF timestamp, classified by whether scene motion is live. */
  sample(now: number, mode: FrameMode): void {
    if (this.last !== undefined) {
      const delta = now - this.last;
      if (delta > 0 && delta < 1000) {
        const target = mode === 'tween' ? this.tween : this.idle;
        target.push(delta);
        if (target.length > MAX_SAMPLES) target.shift();
      }
    }
    this.last = now;
  }

  /** Current idle summary. */
  idleSummary(): FrameSummary {
    return summarizeFrameDeltas(this.idle);
  }

  /** Current tween summary. */
  tweenSummary(): FrameSummary {
    return summarizeFrameDeltas(this.tween);
  }

  /** Drop all samples and restart the clock. */
  reset(): void {
    this.last = undefined;
    this.idle.length = 0;
    this.tween.length = 0;
  }
}
