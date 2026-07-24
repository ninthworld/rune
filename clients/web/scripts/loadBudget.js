/**
 * Load-budget classification and evaluation for the built web client.
 *
 * The ceilings come from `docs/design/presentation-budgets.md` §Load and asset
 * budgets and this module is the only place that encodes them. It is pure:
 * `checkLoadBudget.js` walks `dist/` and measures gzip, the Vitest suite hands
 * in synthetic entries, and both get the same classification and the same
 * pass/fail decision.
 *
 * Plain ESM JavaScript rather than TypeScript because CI runs the gate with
 * bare `node` against the build output, after `tsc` has already run and with no
 * bundler in the loop.
 */

/**
 * Budget ceilings in bytes. kB/MB are decimal (1 kB = 1000 B), matching both
 * the budgets document and Vite's build report so the numbers can be compared
 * without conversion.
 */
export const LOAD_BUDGETS = Object.freeze({
  /** Interactive code bundle, gzipped, excluding art/audio: ≤ 1.0 MB. */
  interactive: 1_000_000,
  /** Bundled fonts, transfer size: ≤ 60 KB. */
  fonts: 60_000,
  /** Total first-match download at default quality: ≤ 4 MB. */
  firstMatch: 4_000_000,
});

/** Extensions that make up the interactive code bundle. */
const CODE_EXTENSIONS = new Set(['.css', '.html', '.js', '.mjs']);

/** Extensions counted against the font budget. */
const FONT_EXTENSIONS = new Set(['.otf', '.ttf', '.woff', '.woff2']);

/** Extensions whose bytes are already compressed, so gzip buys nothing. */
const PRECOMPRESSED_EXTENSIONS = new Set([
  ...FONT_EXTENSIONS,
  '.avif',
  '.br',
  '.gif',
  '.gz',
  '.jpeg',
  '.jpg',
  '.m4a',
  '.mp3',
  '.mp4',
  '.ogg',
  '.opus',
  '.png',
  '.wav',
  '.webm',
  '.webp',
  '.zip',
]);

/**
 * Path prefixes that ship in `dist/` but are outside the first-match download.
 *
 * - `card-art/` — ADR 0024 art is player-side, opt-in, and device-cached; it
 *   never blocks play.
 * - `lazy/` — the convention for anything that must load after the match is
 *   playable (alternate environment themes, audio).
 *
 * Everything else counts. An asset is in the first-match set until it is
 * deliberately moved out of it, so a new asset can never slip past the gate by
 * being unclassified.
 */
const DEFERRED_PREFIXES = ['card-art/', 'lazy/'];

/** The budget groups, in report order. */
const GROUPS = [
  {
    id: 'interactive',
    label: 'Interactive code bundle (gzipped, excl. art/audio)',
    classes: ['code'],
  },
  { id: 'fonts', label: 'Bundled fonts', classes: ['font'] },
  {
    id: 'firstMatch',
    label: 'First-match download (code + fonts + UI assets)',
    classes: ['code', 'font', 'asset'],
  },
];

/**
 * Lowercased final extension of a POSIX-style relative path, including the dot.
 * A dotfile with no other dot (`.gitkeep`) has no extension.
 *
 * @param {string} path
 * @returns {string}
 */
export function assetExtension(path) {
  const name = path.slice(path.lastIndexOf('/') + 1);
  const dot = name.lastIndexOf('.');
  return dot <= 0 ? '' : name.slice(dot).toLowerCase();
}

/**
 * Which budget class a built file belongs to: `code`, `font`, `asset` (any
 * other shipped file), or `deferred` (excluded from every budget).
 *
 * @param {string} path POSIX-style path relative to the build output root.
 * @returns {'code' | 'font' | 'asset' | 'deferred'}
 */
export function classifyAsset(path) {
  if (DEFERRED_PREFIXES.some((prefix) => path.startsWith(prefix))) return 'deferred';
  const extension = assetExtension(path);
  if (CODE_EXTENSIONS.has(extension)) return 'code';
  if (FONT_EXTENSIONS.has(extension)) return 'font';
  return 'asset';
}

/**
 * Bytes a browser downloads for one file. Text is served gzipped; already
 * compressed formats are served as-is, and gzip never gets to report *more*
 * than the raw bytes (it inflates very small files).
 *
 * @param {{ path: string, bytes: number, gzipBytes: number }} entry
 * @returns {number}
 */
export function transferBytes(entry) {
  if (PRECOMPRESSED_EXTENSIONS.has(assetExtension(entry.path))) return entry.bytes;
  return Math.min(entry.bytes, entry.gzipBytes);
}

/**
 * Format a byte count the way the budgets document and Vite state sizes.
 *
 * @param {number} bytes
 * @returns {string}
 */
export function formatBytes(bytes) {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(2)} MB`;
  return `${(bytes / 1000).toFixed(2)} kB`;
}

/**
 * Measure built files against the load budgets.
 *
 * @param {ReadonlyArray<{ path: string, bytes: number, gzipBytes: number }>} entries
 * @param {typeof LOAD_BUDGETS} [budgets]
 * @returns {{
 *   files: Array<{ path: string, bytes: number, gzipBytes: number, class: string, transfer: number }>,
 *   groups: Array<{ id: string, label: string, bytes: number, limit: number, ok: boolean }>,
 *   violations: string[],
 *   ok: boolean,
 * }}
 */
export function evaluateLoadBudget(entries, budgets = LOAD_BUDGETS) {
  const files = entries
    .map((entry) => ({
      ...entry,
      class: classifyAsset(entry.path),
      transfer: transferBytes(entry),
    }))
    .sort((a, b) => b.transfer - a.transfer || a.path.localeCompare(b.path));

  const groups = GROUPS.map((group) => {
    const bytes = files
      .filter((file) => group.classes.includes(file.class))
      .reduce((sum, file) => sum + file.transfer, 0);
    const limit = budgets[group.id];
    return { id: group.id, label: group.label, bytes, limit, ok: bytes <= limit };
  });

  const violations = groups
    .filter((group) => !group.ok)
    .map(
      (group) =>
        `${group.label}: ${formatBytes(group.bytes)} exceeds the ${formatBytes(group.limit)} budget` +
        ` by ${formatBytes(group.bytes - group.limit)}`,
    );

  return { files, groups, violations, ok: violations.length === 0 };
}

/**
 * Render a report as the fixed-width table the gate prints. Deferred files are
 * listed too, with their class, so an exclusion is visible rather than silent.
 *
 * @param {ReturnType<typeof evaluateLoadBudget>} report
 * @returns {string}
 */
export function formatLoadBudgetReport(report) {
  const lines = ['Load budgets (docs/design/presentation-budgets.md)', ''];
  for (const file of report.files) {
    lines.push(
      `  ${file.class.padEnd(8)} ${formatBytes(file.transfer).padStart(10)}  ${file.path}`,
    );
  }
  lines.push('');
  for (const group of report.groups) {
    const used = group.limit === 0 ? 0 : (group.bytes / group.limit) * 100;
    lines.push(
      `  ${group.ok ? 'PASS' : 'FAIL'}  ${group.label}` +
        `\n        ${formatBytes(group.bytes)} of ${formatBytes(group.limit)} (${used.toFixed(1)}%)`,
    );
  }
  return lines.join('\n');
}
