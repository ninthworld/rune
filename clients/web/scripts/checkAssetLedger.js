#!/usr/bin/env node

/**
 * ADR 0031 provenance and repository-weight gate.
 *
 * Every shipping raster in the presentation trees must have one ledger entry,
 * use a content-hashed filename, and actually match that hash. The same pass
 * enforces the 1.5 MB environment-theme and 12 MB total ceilings.
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '../../..');
const ledgerPath = resolve(repoRoot, 'clients/web/src/assets/ledger.json');
const assetRoots = [
  resolve(repoRoot, 'clients/web/public/assets'),
  resolve(repoRoot, 'clients/web/public/lazy'),
  resolve(repoRoot, 'clients/web/public/card-art'),
];
const rasterExtensions = /\.(?:avif|jpe?g|png|webp)$/i;
const hashedFilename = /\.([a-f0-9]{8})\.(?:avif|jpe?g|png|webp)$/i;
const forbiddenPromptTargets =
  /\b(?:magic:\s*the gathering|magic the gathering|wizards of the coast|mtg arena)\b/i;
const themeLimit = 1_500_000;
const totalLimit = 12_000_000;

function posix(path) {
  return path.split(sep).join('/');
}

function walk(root) {
  if (!existsSync(root)) return [];
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = resolve(root, entry.name);
    if (entry.isDirectory()) files.push(...walk(path));
    else if (entry.isFile() && rasterExtensions.test(entry.name)) files.push(path);
  }
  return files;
}

function digest(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex').slice(0, 8);
}

export function evaluateAssetLedger() {
  const ledger = JSON.parse(readFileSync(ledgerPath, 'utf8'));
  const entries = Array.isArray(ledger.assets) ? ledger.assets : [];
  const files = assetRoots.flatMap(walk);
  const filePaths = new Set(files.map((path) => posix(relative(repoRoot, path))));
  const entryPaths = new Set();
  const errors = [];

  for (const entry of entries) {
    if (!entry || typeof entry.path !== 'string') {
      errors.push('Ledger entry is missing a string path.');
      continue;
    }
    if (entryPaths.has(entry.path)) errors.push(`Duplicate ledger path: ${entry.path}`);
    entryPaths.add(entry.path);
    const absolute = resolve(repoRoot, entry.path);
    if (!existsSync(absolute)) {
      errors.push(`Ledger path does not exist: ${entry.path}`);
      continue;
    }
    if (![1, 2, 3].includes(entry.provenanceClass)) {
      errors.push(`Invalid provenance class for ${entry.path}`);
    }
    if (typeof entry.license !== 'string' || entry.license.length === 0) {
      errors.push(`Missing license for ${entry.path}`);
    }
    if (entry.provenanceClass === 2) {
      if (typeof entry.authorTool !== 'string' || entry.authorTool.length === 0) {
        errors.push(`Missing generation tool for ${entry.path}`);
      }
      if (typeof entry.promptEssence !== 'string' || entry.promptEssence.length === 0) {
        errors.push(`Missing prompt essence for ${entry.path}`);
      } else if (forbiddenPromptTargets.test(entry.promptEssence)) {
        errors.push(`Forbidden style target in prompt essence for ${entry.path}`);
      }
    }
    const match = entry.path.match(hashedFilename);
    if (!match) {
      errors.push(`Raster filename is not content-hashed: ${entry.path}`);
    } else if (digest(absolute) !== match[1]) {
      errors.push(`Content hash mismatch: ${entry.path}`);
    }
  }

  for (const path of filePaths) {
    if (!entryPaths.has(path)) errors.push(`Unledgered presentation asset: ${path}`);
  }
  for (const path of entryPaths) {
    if (!filePaths.has(path)) errors.push(`Ledger path is outside the shipping trees: ${path}`);
  }

  const totalBytes = files.reduce((sum, path) => sum + statSync(path).size, 0);
  if (totalBytes >= totalLimit) {
    errors.push(`Presentation assets total ${totalBytes} bytes; limit is < ${totalLimit}.`);
  }

  const themeBytes = new Map();
  for (const path of files) {
    const relativePath = posix(relative(repoRoot, path));
    const match = relativePath.match(/\/environments\/([^/]+)\//);
    if (!match) continue;
    themeBytes.set(match[1], (themeBytes.get(match[1]) ?? 0) + statSync(path).size);
  }
  for (const [theme, bytes] of themeBytes) {
    if (bytes > themeLimit) {
      errors.push(`Environment theme ${theme} is ${bytes} bytes; limit is ${themeLimit}.`);
    }
  }

  return { entries: entries.length, files: files.length, totalBytes, themeBytes, errors };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = evaluateAssetLedger();
  for (const error of result.errors) console.error(`FAIL  ${error}`);
  if (result.errors.length > 0) process.exit(1);
  console.log(
    `PASS  ${result.files} presentation assets, ${result.totalBytes} bytes total; ` +
      [...result.themeBytes].map(([theme, bytes]) => `${theme} ${bytes} bytes`).join(', '),
  );
}
