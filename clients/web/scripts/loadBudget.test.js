import { describe, expect, it } from 'vitest';
import {
  LOAD_BUDGETS,
  assetExtension,
  classifyAsset,
  evaluateLoadBudget,
  formatBytes,
  formatLoadBudgetReport,
  transferBytes,
} from './loadBudget.js';

/** A file entry sized so gzip is the smaller of the two measurements. */
function entry(path, bytes, gzipBytes = Math.round(bytes / 3)) {
  return { path, bytes, gzipBytes };
}

/** The shape of a passing build: code, one font, one deferred art file. */
const BUILT = [
  entry('assets/index-B7vrc7AG.js', 913_125, 282_267),
  entry('assets/index-RIA-8Pbt.css', 96_864, 16_220),
  entry('assets/rune-display-BRKeoiZa.woff2', 14_516, 14_539),
  entry('index.html', 389, 263),
  entry('card-art/manifest.json', 3, 23),
];

describe('assetExtension', () => {
  it('lowercases the final extension', () => {
    expect(assetExtension('assets/index-ABC.JS')).toBe('.js');
  });

  it('has no extension for a dotfile or a bare name', () => {
    expect(assetExtension('assets/.gitkeep')).toBe('');
    expect(assetExtension('LICENSE')).toBe('');
  });
});

describe('classifyAsset', () => {
  it('counts scripts, styles, and documents as the code bundle', () => {
    expect(classifyAsset('assets/index.js')).toBe('code');
    expect(classifyAsset('assets/index.css')).toBe('code');
    expect(classifyAsset('index.html')).toBe('code');
  });

  it('counts web fonts against the font budget', () => {
    expect(classifyAsset('assets/rune-display.woff2')).toBe('font');
    expect(classifyAsset('assets/rune-display.ttf')).toBe('font');
  });

  it('defers ADR 0024 card art and the lazy-load convention', () => {
    expect(classifyAsset('card-art/manifest.json')).toBe('deferred');
    expect(classifyAsset('lazy/theme-dusk.webp')).toBe('deferred');
  });

  it('counts anything else as a first-match asset rather than dropping it', () => {
    expect(classifyAsset('assets/crest.svg')).toBe('asset');
    expect(classifyAsset('assets/board.png')).toBe('asset');
    expect(classifyAsset('unknown')).toBe('asset');
  });
});

describe('transferBytes', () => {
  it('uses gzipped bytes for text', () => {
    expect(transferBytes(entry('assets/index.js', 900, 300))).toBe(300);
  });

  it('uses raw bytes for already-compressed formats', () => {
    expect(transferBytes(entry('assets/font.woff2', 14_516, 14_539))).toBe(14_516);
  });

  it('never reports more than the raw bytes when gzip inflates', () => {
    expect(transferBytes(entry('card-art/manifest.json', 3, 23))).toBe(3);
  });
});

describe('evaluateLoadBudget', () => {
  it('passes the current build and excludes deferred files from every group', () => {
    const report = evaluateLoadBudget(BUILT);
    expect(report.ok).toBe(true);
    expect(report.violations).toEqual([]);
    const [interactive, fonts, firstMatch] = report.groups;
    expect(interactive.bytes).toBe(282_267 + 16_220 + 263);
    expect(fonts.bytes).toBe(14_516);
    expect(firstMatch.bytes).toBe(interactive.bytes + fonts.bytes);
  });

  it('sorts files by transfer size so the largest payload reads first', () => {
    expect(evaluateLoadBudget(BUILT).files[0].path).toBe('assets/index-B7vrc7AG.js');
  });

  it('fails when the gzipped code bundle exceeds 1.0 MB', () => {
    const report = evaluateLoadBudget([entry('assets/index.js', 6_000_000, 1_000_001)]);
    expect(report.ok).toBe(false);
    expect(report.violations).toHaveLength(1);
    expect(report.violations[0]).toContain('Interactive code bundle');
    expect(report.violations[0]).toContain('exceeds the 1.00 MB budget');
  });

  it('fails when the font set exceeds 60 KB', () => {
    const report = evaluateLoadBudget([
      entry('assets/a.woff2', 40_000, 40_000),
      entry('assets/b.woff2', 20_001, 20_001),
    ]);
    expect(report.ok).toBe(false);
    expect(report.groups.find((group) => group.id === 'fonts').ok).toBe(false);
    expect(report.groups.find((group) => group.id === 'interactive').ok).toBe(true);
  });

  it('fails when the first-match set exceeds 4 MB even with code inside budget', () => {
    const report = evaluateLoadBudget([
      entry('assets/index.js', 900_000, 300_000),
      entry('assets/environment.png', 3_800_000, 3_800_000),
    ]);
    expect(report.ok).toBe(false);
    expect(report.violations).toHaveLength(1);
    expect(report.violations[0]).toContain('First-match download');
  });

  it('keeps a payload out of the budgets only when it is deliberately deferred', () => {
    const heavy = entry('theme-dusk.webp', 3_900_000, 3_900_000);
    expect(evaluateLoadBudget([...BUILT, heavy]).ok).toBe(false);
    expect(evaluateLoadBudget([...BUILT, entry('lazy/theme-dusk.webp', 3_900_000)]).ok).toBe(true);
  });

  it('honours an overridden ceiling so the gate can be exercised', () => {
    const report = evaluateLoadBudget(BUILT, { ...LOAD_BUDGETS, interactive: 100_000 });
    expect(report.ok).toBe(false);
    expect(report.violations[0]).toContain('exceeds the 100.00 kB budget');
  });
});

describe('formatBytes', () => {
  it('states kB below a megabyte and MB above it', () => {
    expect(formatBytes(298_750)).toBe('298.75 kB');
    expect(formatBytes(1_000_000)).toBe('1.00 MB');
  });
});

describe('formatLoadBudgetReport', () => {
  it('reports every file with its class and every group with its verdict', () => {
    const text = formatLoadBudgetReport(evaluateLoadBudget(BUILT));
    expect(text).toContain('assets/index-B7vrc7AG.js');
    expect(text).toContain('deferred');
    expect(text.match(/PASS/g)).toHaveLength(3);
  });

  it('marks a violated group FAIL', () => {
    const text = formatLoadBudgetReport(
      evaluateLoadBudget(BUILT, { ...LOAD_BUDGETS, fonts: 1000 }),
    );
    expect(text).toContain('FAIL  Bundled fonts');
  });
});
