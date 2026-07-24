/*
 * Real-hardware presentation-budget probe (issue #510).
 *
 * Paste this whole file into a console attached to the device under test —
 * Android Chrome over `chrome://inspect`, iOS Safari over the macOS Web
 * Inspector, or the laptop's own devtools — with `/fixtures/2.5d` open. It
 * drives `window.__RUNE_2_5D_FIXTURE__` through the stress scenarios, reads the
 * live `fixture/metrics.ts` report for each, and prints a Markdown table ready
 * to paste into `docs/design/presentation-budgets.md`.
 *
 *   await runeDeviceBudgetProbe({ label: 'Pixel 3a — Android 12, Chrome 126' })
 *
 * The quality level comes from the page (`?quality=lite|standard|high`), not
 * from here: reload the route to change it, so every run records the level it
 * was actually measured at. The full procedure is written out in the budgets
 * document under §Real-hardware validation.
 *
 * Deliberately paste-safe: no imports, no exports, no top-level await, and
 * everything scoped inside one IIFE so re-pasting it never collides. Exercised
 * by `deviceBudgetProbe.test.js` against a stub harness.
 */
(function installRuneDeviceBudgetProbe() {
  /** The stress scenarios the budgets document requires evidence for. */
  const DEFAULT_SCENARIOS = [
    'commander4',
    'six',
    'tokens',
    'big-hand',
    'combat-web',
    'deep-stack',
    'phone',
  ];

  /** Evidence-table columns, in the order the budgets document lists them. */
  const COLUMNS = [
    'Scenario',
    'Quality',
    'Idle fps',
    'Idle p95 ms',
    'Tween fps',
    'Tween p95 ms',
    'Rebuild ms',
    'DOM nodes',
    'Heap MB',
    'Verdict',
  ];

  /** The harness republishes its hook as the report changes; never cache it. */
  function harness() {
    const hook = window.__RUNE_2_5D_FIXTURE__;
    if (!hook) {
      throw new Error(
        'window.__RUNE_2_5D_FIXTURE__ is absent. Open /fixtures/2.5d (a production' +
          ' preview needs VITE_RUNE_FIXTURE_HARNESS=true) and rerun.',
      );
    }
    return hook;
  }

  function wait(ms) {
    return new Promise(function resolveAfter(resolve) {
      setTimeout(resolve, ms);
    });
  }

  /** Frame columns stay blank until the sampler has enough real frames. */
  function fps(summary) {
    return summary.samples < 2 ? '—' : summary.fps.toFixed(1);
  }

  function p95(summary) {
    return summary.samples < 2 ? '—' : summary.p95Ms.toFixed(1);
  }

  /** Chrome-only; Safari does not expose it and the cell stays blank there. */
  function heapMb() {
    const memory = window.performance && window.performance.memory;
    if (!memory || typeof memory.usedJSHeapSize !== 'number') return '—';
    return (memory.usedJSHeapSize / 1048576).toFixed(1);
  }

  function row(report) {
    return [
      report.scenario,
      report.quality,
      fps(report.idle),
      p95(report.idle),
      fps(report.tween),
      p95(report.tween),
      report.rebuildMs.toFixed(1),
      String(report.domNodes),
      heapMb(),
      report.passes ? 'within budget' : 'over budget',
    ];
  }

  function markdownTable(rows) {
    const divider = COLUMNS.map(function dash() {
      return '---';
    });
    const lines = [COLUMNS, divider].concat(rows).map(function line(cells) {
      return '| ' + cells.join(' | ') + ' |';
    });
    return lines.join('\n');
  }

  function environment(label) {
    return [
      '**Device:** ' + (label || '(fill in: device, OS, browser version)'),
      '**User agent:** `' + window.navigator.userAgent + '`',
      '**Viewport:** ' +
        window.innerWidth +
        '×' +
        window.innerHeight +
        ' @ DPR ' +
        (window.devicePixelRatio || 1),
      '**Run:** ' + new Date().toISOString(),
    ].join('  \n');
  }

  /**
   * Measure one scenario: select it (which resets the frame sampler), let the
   * scene settle, time a reconnect-style rebuild, then sample while the
   * sequence plays. Single-frame scenarios have nothing to play, so their tween
   * columns come back blank — that is the expected shape, not a failed run.
   */
  async function measureScenario(id, settings) {
    harness().selectScenario(id);
    await wait(settings.settleMs);
    const rebuildMs = harness().rebuild();
    harness().play();
    await wait(settings.sampleMs);
    const report = harness().report;
    harness().pause();
    return Object.assign({}, report, { rebuildMs: rebuildMs });
  }

  /**
   * Run the probe. Resolves to `{ rows, reports, markdown }`; the Markdown is
   * also logged so it can be copied straight out of the console.
   */
  window.runeDeviceBudgetProbe = async function runeDeviceBudgetProbe(options) {
    const settings = Object.assign(
      { label: '', scenarios: DEFAULT_SCENARIOS, sampleMs: 6000, settleMs: 500 },
      options || {},
    );
    const reports = [];
    const rows = [];
    for (const id of settings.scenarios) {
      const report = await measureScenario(id, settings);
      reports.push(report);
      rows.push(row(report));
    }
    const markdown = environment(settings.label) + '\n\n' + markdownTable(rows);
    console.log(markdown);
    return { rows: rows, reports: reports, markdown: markdown };
  };
})();
