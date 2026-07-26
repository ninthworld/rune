#!/usr/bin/env node

/**
 * Generate the shipping card-frame plates (issue #570).
 *
 *     node scripts/generateFramePlates.js
 *
 * The synthesis lives in `framePlates.js`; this is the I/O half. It renders
 * every plate, encodes it, content-hashes the shipping bytes, writes them under
 * `public/assets/frames/`, and rewrites the two records that make an asset
 * legitimate in this repository:
 *
 * - the `cardFrames` section of `public/assets/manifest.json` — the single
 *   source of truth for the hashed paths the client fetches, exactly as the
 *   environment and card-back sections are (`src/assets/productionManifest.ts`
 *   explains why the client never transcribes a hash); and
 * - the `card-frame` entries of `src/assets/ledger.json` — the ADR 0031
 *   provenance record `scripts/checkAssetLedger.js` enforces.
 *
 * Both rewrites are idempotent and scoped: nothing outside the frame plates is
 * touched, and stale hashed files from a previous run are deleted, so a
 * re-run after a token change leaves the tree coherent rather than accreting.
 *
 * Encoding: the plates ship as PNG from the in-process encoder in
 * `framePlates.js` — no external tool and no probing, because the committed
 * bytes have to be reproducible and checkable. `framePlates.test.js` re-encodes
 * every plate and compares it byte-for-byte with the file in the tree, so a
 * re-run of this command is a verified no-op rather than a promise. That module
 * documents the trade in full.
 */
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';
import prettier from 'prettier';
import { encodePng, PLATE_SPECS, plateFilename, renderPlate } from './framePlates.js';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const clientRoot = resolve(scriptDir, '..');
const repoRoot = resolve(clientRoot, '../..');
const outputDir = resolve(clientRoot, 'public/assets/frames');
const manifestPath = resolve(clientRoot, 'public/assets/manifest.json');
const ledgerPath = resolve(clientRoot, 'src/assets/ledger.json');

/** The ledger category these assets file under. */
const CATEGORY = 'card-frame';

/* ── Records ──────────────────────────────────────────────────────────────── */

/**
 * Write JSON the way the repository formats it. Both records are committed
 * files that `npm run lint` checks with Prettier, so the generator formats
 * through Prettier itself rather than leaving a `lint:fix` step to remember.
 */
async function writeJson(path, source) {
  const config = (await prettier.resolveConfig(path)) ?? {};
  writeFileSync(path, await prettier.format(source, { ...config, filepath: path }));
}

/**
 * Replace (or append) one top-level section of a JSON document **textually**,
 * leaving every other byte of the file alone.
 *
 * Re-serialising the whole manifest would work and would be shorter, but
 * Prettier's JSON printer preserves the author's own expansion choices, so a
 * `JSON.stringify` round trip re-expands every hand-compacted line in the file
 * and buries seven new plates in a two-hundred-line reformat. The generator
 * owns its own section and nothing else; the diff should say so.
 *
 * The section is always written last, which is what makes the scan trivial:
 * everything from the key to the end of the document is ours to replace.
 */
function upsertLastSection(text, key, value) {
  const marker = `\n  "${key}": `;
  const at = text.indexOf(marker);
  const head = (at >= 0 ? text.slice(0, at) : text.replace(/\s*\}\s*$/, '')).replace(/,\s*$/, '');
  const body = JSON.stringify(value, null, 2).split('\n').join('\n  ');
  return `${head},\n  "${key}": ${body}\n}\n`;
}

/** Replace the `card-frame` block of the ledger, leaving every other entry. */
async function rewriteLedger(entries) {
  const ledger = JSON.parse(readFileSync(ledgerPath, 'utf8'));
  const kept = ledger.assets.filter((entry) => entry.category !== CATEGORY);
  ledger.assets = [...kept, ...entries];
  // The ledger is a flat list this generator appends to, and it is already
  // fully expanded, so a whole-file round trip costs no spurious diff.
  await writeJson(ledgerPath, `${JSON.stringify(ledger, null, 2)}\n`);
}

/* ── Main ─────────────────────────────────────────────────────────────────── */

async function main() {
  mkdirSync(outputDir, { recursive: true });
  for (const name of existsSync(outputDir) ? readdirSync(outputDir) : []) {
    rmSync(resolve(outputDir, name), { force: true });
  }

  const plates = {};
  const ledgerEntries = [];
  let total = 0;

  for (const spec of PLATE_SPECS) {
    const { width, height, rgba } = renderPlate(spec);
    const bytes = encodePng(width, height, rgba, deflateSync);
    const hash = createHash('sha256').update(bytes).digest('hex').slice(0, 8);
    const file = plateFilename(spec, hash);
    writeFileSync(resolve(outputDir, file), bytes);
    total += bytes.length;

    plates[spec.key] = {
      src: `/assets/frames/${file}`,
      width,
      height,
      // The nine-slice inset, in authored px. `0` marks a plate that tiles
      // instead of slicing (the identity material).
      slice: spec.slice,
      // The fraction of the card width `W` the drawn band occupies — what
      // `theme.ts` resolves into `border-image-width` per tier.
      band: spec.band,
      // Whether the middle patch is painted (a printed surface) or dropped
      // (a ring, so the card body shows through).
      fill: spec.fill,
      load: 'first-match',
      ...(spec.tile === undefined ? {} : { tile: spec.tile }),
    };

    ledgerEntries.push({
      path: `clients/web/public/assets/frames/${file}`,
      title: spec.title,
      category: CATEGORY,
      provenanceClass: 1,
      authorTool: 'clients/web/scripts/generateFramePlates.js (deterministic synthesis)',
      license: 'MIT',
      sourceUrl: null,
      promptEssence: spec.essence,
    });

    console.log(`  ${file.padEnd(34)} ${String(bytes.length).padStart(6)} B`);
  }

  const manifest = readFileSync(manifestPath, 'utf8');
  await writeJson(manifestPath, upsertLastSection(manifest, 'cardFrames', { plates }));
  await rewriteLedger(ledgerEntries);

  console.log(
    `\nWrote ${PLATE_SPECS.length} frame plates (${total} B) to ${resolve(outputDir).replace(`${repoRoot}/`, '')}`,
  );
}

await main();
