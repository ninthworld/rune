import { afterEach, beforeAll, describe, expect, it } from 'vitest';
// The probe is pasted into a device console, so it must stay a plain script: no
// imports, no exports, no top-level await. Reading the raw source and evaluating
// it is how a console loads it, and the only way to test it.
import source from './deviceBudgetProbe.js?raw';

/** A harness stub that answers like the mounted `/fixtures/2.5d` route. */
function stubHarness(overrides = {}) {
  const calls = [];
  const harness = {
    calls,
    ready: true,
    report: report('commander4'),
    selectScenario(id) {
      calls.push(`select:${id}`);
      harness.report = report(id);
    },
    selectFrame() {},
    play() {
      calls.push('play');
    },
    pause() {
      calls.push('pause');
    },
    rebuild() {
      calls.push('rebuild');
      return 12.5;
    },
    ...overrides,
  };
  return harness;
}

function report(scenario) {
  return {
    scenario,
    quality: 'lite',
    compact: false,
    idle: { samples: 240, fps: 57.3, p95Ms: 16.8 },
    tween: { samples: 240, fps: 50.3, p95Ms: 33.4 },
    rebuildMs: 9,
    domNodes: 4210,
    budget: { minFps: 30, maxP95Ms: 33.3, maxRebuildMs: 50, maxDomNodes: 15_000 },
    passes: true,
  };
}

const options = { sampleMs: 0, settleMs: 0, scenarios: ['commander4', 'tokens'] };

beforeAll(() => {
  new Function(source)();
});

afterEach(() => {
  delete window.__RUNE_2_5D_FIXTURE__;
});

describe('runeDeviceBudgetProbe', () => {
  it('installs itself without module syntax', () => {
    expect(source).not.toMatch(/^\s*(import|export)\s/m);
    expect(typeof window.runeDeviceBudgetProbe).toBe('function');
  });

  it('explains what to open when the harness is absent', async () => {
    await expect(window.runeDeviceBudgetProbe(options)).rejects.toThrow('__RUNE_2_5D_FIXTURE__');
  });

  it('selects, rebuilds, plays, and pauses each scenario in order', async () => {
    const harness = stubHarness();
    window.__RUNE_2_5D_FIXTURE__ = harness;
    await window.runeDeviceBudgetProbe(options);
    expect(harness.calls).toEqual([
      'select:commander4',
      'rebuild',
      'play',
      'pause',
      'select:tokens',
      'rebuild',
      'play',
      'pause',
    ]);
  });

  it('records the measured rebuild rather than the last published one', async () => {
    window.__RUNE_2_5D_FIXTURE__ = stubHarness();
    const { reports } = await window.runeDeviceBudgetProbe(options);
    expect(reports.map((entry) => entry.rebuildMs)).toEqual([12.5, 12.5]);
  });

  it('emits one Markdown row per scenario with the measured figures', async () => {
    window.__RUNE_2_5D_FIXTURE__ = stubHarness();
    const { rows, markdown } = await window.runeDeviceBudgetProbe({
      ...options,
      label: 'Pixel 3a — Android 12, Chrome 126',
    });
    expect(rows).toHaveLength(2);
    expect(rows[0].slice(0, 8)).toEqual([
      'commander4',
      'lite',
      '57.3',
      '16.8',
      '50.3',
      '33.4',
      '12.5',
      '4210',
    ]);
    expect(markdown).toContain('Pixel 3a — Android 12, Chrome 126');
    expect(markdown).toContain('| Scenario | Quality |');
    expect(markdown).toContain('| commander4 | lite |');
  });

  it('blanks frame columns that never gathered enough samples', async () => {
    const empty = report('phone');
    empty.tween = { samples: 0, fps: 0, p95Ms: 0 };
    window.__RUNE_2_5D_FIXTURE__ = stubHarness({ report: empty, selectScenario() {} });
    const { rows } = await window.runeDeviceBudgetProbe({ ...options, scenarios: ['phone'] });
    expect(rows[0][4]).toBe('—');
    expect(rows[0][5]).toBe('—');
  });

  it('marks a run that missed its budget', async () => {
    const missed = report('tokens');
    missed.passes = false;
    window.__RUNE_2_5D_FIXTURE__ = stubHarness({ report: missed, selectScenario() {} });
    const { rows } = await window.runeDeviceBudgetProbe({ ...options, scenarios: ['tokens'] });
    expect(rows[0][9]).toBe('over budget');
  });

  it('leaves the heap column blank where the browser does not expose it', async () => {
    window.__RUNE_2_5D_FIXTURE__ = stubHarness();
    const { rows } = await window.runeDeviceBudgetProbe({ ...options, scenarios: ['six'] });
    expect(rows[0][8]).toBe('—');
  });
});
