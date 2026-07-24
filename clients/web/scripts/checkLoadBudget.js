#!/usr/bin/env node
/**
 * CI load-budget gate (issue #510).
 *
 * Measures the **built production bundle** in `dist/` — never dev-server output
 * — against `docs/design/presentation-budgets.md` §Load and asset budgets, and
 * exits non-zero on any violation so a regression fails the client gate.
 *
 * Usage: `npm run budget` (after `npm run build`), or
 * `node scripts/checkLoadBudget.js [distDir] [--json]`.
 */
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { evaluateLoadBudget, formatLoadBudgetReport } from './loadBudget.js';

/**
 * Resolve the build output directory to measure. An explicit argument is
 * resolved against the working directory — `resolve` returns an absolute
 * argument unchanged, where `join` would have concatenated it onto the cwd —
 * and with no argument the gate measures this package's own `dist/`, which is
 * the path CI takes.
 *
 * @param {string | undefined} arg
 * @param {string} cwd
 * @param {string} packageRoot
 * @returns {string}
 */
export function resolveDistDir(arg, cwd, packageRoot) {
  return arg ? resolve(cwd, arg) : join(packageRoot, 'dist');
}

/**
 * Collect every built file as a POSIX-style path relative to the output root.
 *
 * @param {string} root
 * @param {string} [prefix]
 * @returns {string[]}
 */
function collectFiles(root, prefix = '') {
  const found = [];
  for (const entry of readdirSync(join(root, prefix), { withFileTypes: true })) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) found.push(...collectFiles(root, path));
    else if (entry.isFile()) found.push(path);
  }
  return found;
}

/**
 * Measure raw and gzipped bytes for one built file. Gzip runs at the zlib
 * default level, which is what a static host or CDN compresses with on the fly
 * — the conservative reading, since a precompressed deploy at level 9 is
 * slightly smaller still.
 *
 * @param {string} root
 * @param {string} path
 * @returns {{ path: string, bytes: number, gzipBytes: number }}
 */
function measure(root, path) {
  const bytes = readFileSync(join(root, path));
  return { path, bytes: bytes.length, gzipBytes: gzipSync(bytes).length };
}

function main() {
  const args = process.argv.slice(2);
  const json = args.includes('--json');
  const distArg = args.find((arg) => !arg.startsWith('--'));
  const packageRoot = fileURLToPath(new URL('..', import.meta.url));
  const distDir = resolveDistDir(distArg, process.cwd(), packageRoot);

  // The fixture harness (`/fixtures/2.5d`) is tree-shaken out of a normal
  // production build and pulls its scenario data in when it is not. Measuring
  // that build would report a bundle nobody ships.
  if (process.env.VITE_RUNE_FIXTURE_HARNESS === 'true') {
    console.error(
      'VITE_RUNE_FIXTURE_HARNESS=true: this build carries the fixture harness and is not' +
        ' the shipped bundle. Rebuild without it before measuring load budgets.',
    );
    process.exitCode = 1;
    return;
  }

  let files;
  try {
    files = collectFiles(distDir);
  } catch {
    console.error(`No build output at ${distDir}. Run \`npm run build\` first.`);
    process.exitCode = 1;
    return;
  }
  if (files.length === 0) {
    console.error(`Build output at ${distDir} is empty. Run \`npm run build\` first.`);
    process.exitCode = 1;
    return;
  }

  const report = evaluateLoadBudget(files.map((path) => measure(distDir, path)));

  if (json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(formatLoadBudgetReport(report));
  }

  if (!report.ok) {
    console.error('\nLoad budget exceeded:');
    for (const violation of report.violations) console.error(`  ${violation}`);
    console.error(
      '\nTrim the payload or amend docs/design/presentation-budgets.md with a measured' +
        ' rationale — do not raise the ceiling to make the gate pass.',
    );
    process.exitCode = 1;
  }
}

// Run only when invoked as the command, so the unit test can import
// `resolveDistDir` without the gate measuring anything.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
