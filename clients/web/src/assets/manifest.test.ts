import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import starterDecks from '../starter-decks.json';

const publicDir = resolve(dirname(fileURLToPath(import.meta.url)), '../../public');
const manifest = JSON.parse(readFileSync(resolve(publicDir, 'assets/manifest.json'), 'utf8'));
const cardArtManifest = JSON.parse(
  readFileSync(resolve(publicDir, 'card-art/manifest.json'), 'utf8'),
);

function assetSources(value: unknown): string[] {
  if (Array.isArray(value)) return value.flatMap(assetSources);
  if (value === null || typeof value !== 'object') return [];
  return Object.entries(value).flatMap(([key, child]) =>
    key === 'src' && typeof child === 'string' ? [child] : assetSources(child),
  );
}

describe('production asset manifests (#548)', () => {
  it('names the approved environment family and the complete Runic Vale layer contract', () => {
    expect(manifest.version).toBe(1);
    expect(manifest.environments.runicVale).toMatchObject({
      label: 'Runic Vale',
      production: true,
      authoringAspect: '21:9',
      focalSafe: { x: 0.1, y: 0, width: 0.8, height: 1 },
    });
    expect(Object.keys(manifest.environments.runicVale.layers).sort()).toEqual([
      'l0',
      'l1',
      'l1Half',
      'l2',
      'l3',
    ]);
    expect(
      Object.values(manifest.environmentStudies as Record<string, { label: string }>).map(
        (study) => study.label,
      ),
    ).toEqual(['Verdant Canals', 'Sunlit Observatory', 'Moonlit Ruins']);
  });

  it('ships one local and eight opponent portraits without the procedural fallback', () => {
    expect(manifest.portraits.fallback).toBeNull();
    expect(manifest.portraits.local.key).toBe('localHood');
    expect(manifest.portraits.opponents).toHaveLength(8);
    expect(
      new Set(
        (manifest.portraits.opponents as Array<{ key: string }>).map((portrait) => portrait.key),
      ).size,
    ).toBe(8);
  });

  it('ships two hidden-information-safe card-back manifest entries', () => {
    expect(manifest.cardBacks.default).toBe('runeSpiral');
    expect(Object.keys(manifest.cardBacks.skins).sort()).toEqual(['runeSpiral', 'verdantKnot']);
  });

  it('references only files that exist in the public shipping tree', () => {
    for (const src of assetSources(manifest)) {
      expect(src.startsWith('/')).toBe(true);
      expect(existsSync(resolve(publicDir, src.slice(1))), src).toBe(true);
    }
  });

  it('maps exactly the starter-deck functional ids to hashed WebP files', () => {
    expect(Object.keys(cardArtManifest.cards).sort()).toEqual([
      'air_elemental',
      'cancel',
      'colossal_dreadmaw',
      'divination',
      'druid_of_the_cowl',
      'electrify',
      'fire_elemental',
      'forest',
      'giant_spider',
      'gigantosaurus',
      'island',
      'jedit_ojanen',
      'lightning_strike',
      'llanowar_elves',
      'mountain',
      'onakke_ogre',
      'plains',
      'revitalize',
      'rustwing_falcon',
      'serra_angel',
      'shock',
      'skyscanner',
      'snapping_drake',
      'titanic_growth',
      'tolarian_scholar',
      'tranquil_expanse',
      'trusty_packbeast',
      'viashino_pyromancer',
      'volcanic_dragon',
    ]);
    for (const filename of Object.values(cardArtManifest.cards)) {
      expect(filename).toMatch(/^[a-z0-9_]+\.[a-f0-9]{8}\.webp$/);
      expect(existsSync(resolve(publicDir, 'card-art', String(filename)))).toBe(true);
    }
  });

  // The bundled source is only whole if it covers the decks a new player is
  // actually handed; a gap here silently drops one card back to procedural.
  it('covers every card in the starter-deck pool (#556)', () => {
    const pool = new Set(
      starterDecks.decks.flatMap((deck) => deck.entries.map((entry) => entry.identity)),
    );
    const bundled = new Set(Object.keys(cardArtManifest.cards));
    expect([...pool].filter((identity) => !bundled.has(identity))).toEqual([]);
  });
});
