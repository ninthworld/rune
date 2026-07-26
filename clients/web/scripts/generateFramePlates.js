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
 * Encoding: the plates are synthesised as PNG with the zero-dependency encoder
 * below (the client toolchain has no image library, and adding one for a script
 * that runs on demand is not worth the audit surface). If `cwebp` or ImageMagick
 * is on PATH the PNG is converted to lossless WebP — smaller for the same
 * pixels, and the format ADR 0031 asks for. Without either, the PNG ships; both
 * are ledger-legal rasters and the manifest records whichever landed.
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';
import prettier from 'prettier';
import { PLATE_SPECS, renderPlate } from './framePlates.js';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const clientRoot = resolve(scriptDir, '..');
const repoRoot = resolve(clientRoot, '../..');
const outputDir = resolve(clientRoot, 'public/assets/frames');
const manifestPath = resolve(clientRoot, 'public/assets/manifest.json');
const ledgerPath = resolve(clientRoot, 'src/assets/ledger.json');

/** The ledger category these assets file under. */
const CATEGORY = 'card-frame';

/* ── PNG ──────────────────────────────────────────────────────────────────── */

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, body) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(body.length, 0);
  head.write(type, 4, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), body])), 0);
  return Buffer.concat([head, body, crc]);
}

/** Encode 8-bit RGBA to PNG. Filter 0 on every scanline; deflate does the rest. */
function encodePng(width, height, rgba) {
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(rgba.buffer, rgba.byteOffset + y * stride, stride).copy(raw, y * (stride + 1) + 1);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // colour type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/* ── WebP ─────────────────────────────────────────────────────────────────── */

function tryConvert(command, args) {
  try {
    execFileSync(command, args, { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

/**
 * Convert PNG bytes to lossless WebP, or return `null` when no converter is
 * installed. Lossless because a light map's value steps are the whole asset:
 * lossy banding in a bevel is visible on every card at once.
 */
function toWebp(png, scratch) {
  writeFileSync(scratch, png);
  const target = `${scratch}.webp`;
  const converted =
    tryConvert('cwebp', ['-quiet', '-lossless', '-exact', '-z', '9', scratch, '-o', target]) ||
    tryConvert('magick', [scratch, '-define', 'webp:lossless=true', '-quality', '100', target]);
  if (!converted) {
    rmSync(scratch, { force: true });
    return null;
  }
  const bytes = readFileSync(target);
  rmSync(scratch, { force: true });
  rmSync(target, { force: true });
  return bytes;
}

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
    const png = encodePng(width, height, rgba);
    const webp = toWebp(png, resolve(outputDir, `.${spec.id}.tmp.png`));
    const bytes = webp ?? png;
    const extension = webp ? 'webp' : 'png';
    const hash = createHash('sha256').update(bytes).digest('hex').slice(0, 8);
    const file = `${spec.id}.${hash}.${extension}`;
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
