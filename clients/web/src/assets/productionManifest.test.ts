/**
 * The typed reader over the shipped production manifest.
 *
 * `productionManifest.ts` performs one unchecked step — a cast from the imported
 * JSON to its declared shape — and this file is what makes that step safe: every
 * accessor is compared against the file read from disk, so a manifest whose
 * shape drifts fails here rather than at a blank backdrop in a browser.
 *
 * The `portraits` section is deliberately untouched: seat identity (issue #532)
 * owns it, and `manifest.test.ts` already gates its shape.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  PRODUCTION_CARD_BACKS,
  PRODUCTION_CARD_BACK_DEFAULT,
  PRODUCTION_ENVIRONMENTS,
  PRODUCTION_ENVIRONMENT_STUDIES,
  PRODUCTION_MANIFEST_VERSION,
  productionCardBack,
  productionEnvironment,
  productionStudy,
} from './productionManifest';

const publicDir = resolve(dirname(fileURLToPath(import.meta.url)), '../../public');
const onDisk = JSON.parse(readFileSync(resolve(publicDir, 'assets/manifest.json'), 'utf8'));

describe('the production manifest reader', () => {
  it('reads the committed file rather than a transcription of it', () => {
    expect(PRODUCTION_MANIFEST_VERSION).toBe(onDisk.version);
    expect(PRODUCTION_ENVIRONMENTS).toEqual(onDisk.environments);
    expect(PRODUCTION_ENVIRONMENT_STUDIES).toEqual(onDisk.environmentStudies);
    expect(PRODUCTION_CARD_BACKS).toEqual(onDisk.cardBacks.skins);
    expect(PRODUCTION_CARD_BACK_DEFAULT).toBe(onDisk.cardBacks.default);
  });

  it('separates a production layer set from a study, and never confuses the two', () => {
    expect(productionEnvironment('runicVale')?.production).toBe(true);
    expect(productionStudy('runicVale')).toBeUndefined();
    for (const theme of ['verdantCanals', 'sunlitObservatory', 'moonlitRuins']) {
      expect(productionEnvironment(theme)).toBeUndefined();
      expect(productionStudy(theme)?.src).toMatch(/^\/lazy\//);
    }
  });

  it('answers `undefined` for a theme or skin that did not ship', () => {
    // The consumers must degrade to the procedural form rather than throw: an
    // asset set is optional by ADR 0031, and the client renders without it.
    expect(productionEnvironment('notATheme')).toBeUndefined();
    expect(productionStudy('notATheme')).toBeUndefined();
    expect(productionCardBack('notASkin')).toBeUndefined();
    expect(productionCardBack(PRODUCTION_CARD_BACK_DEFAULT)).toBeDefined();
  });

  it('names a default card back that actually shipped', () => {
    expect(Object.keys(PRODUCTION_CARD_BACKS)).toContain(PRODUCTION_CARD_BACK_DEFAULT);
  });
});
