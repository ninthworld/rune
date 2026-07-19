import { afterEach, describe, expect, it, vi } from 'vitest';
import { Texture } from 'pixi.js';
import {
  artKeyFor,
  artStatus,
  artUrlFor,
  clearDownloadedArt,
  collectArtCards,
  configureArtStore,
  getArtSource,
  getArtStyle,
  getArtVersion,
  noteCards,
  resetArtStore,
  setArtSource,
  setArtStyle,
  subscribeArt,
  textureForArtKey,
  type ArtStoreDeps,
} from './artStore';
import { MemoryArtCache } from './artCache';
import { namedCardUrl, type FetchLike } from './scryfall';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';

afterEach(() => {
  resetArtStore();
  localStorage.clear();
});

/** Let the store's chained promises settle (all injected effects are immediate). */
async function settle(): Promise<void> {
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** A stub Scryfall + asset network: every name resolves, every image downloads. */
function stubFetch(seen: string[]): FetchLike {
  return (url) => {
    seen.push(url);
    return Promise.resolve({
      ok: true,
      status: 200,
      json: () =>
        Promise.resolve(
          url.endsWith('manifest.json')
            ? []
            : {
                image_uris: {
                  art_crop: `https://img/crop/${url}`,
                  normal: `https://img/full/${url}`,
                },
              },
        ),
      blob: () => Promise.resolve(new Blob(['img'])),
    });
  };
}

/** Store deps that never touch the network, DOM decode paths, or wall clock. */
function testDeps(seen: string[] = []): Partial<ArtStoreDeps> {
  return {
    fetchLike: stubFetch(seen),
    cache: new MemoryArtCache(),
    loadArt: () => Promise.resolve({ texture: Texture.WHITE, url: 'blob:stub' }),
    delay: () => Promise.resolve(),
    now: () => 1234,
  };
}

describe('artStore (ADR 0024)', () => {
  it('defaults to the procedural source and publishes no art keys', async () => {
    configureArtStore(testDeps());
    expect(getArtSource()).toBe('procedural');
    noteCards([{ functionalId: 'onakke_ogre', name: 'Onakke Ogre' }]);
    await settle();
    expect(artKeyFor('onakke_ogre')).toBeUndefined();
    expect(artUrlFor('onakke_ogre')).toBeUndefined();
  });

  it('resolves a real card by its own name and publishes its texture', async () => {
    const seen: string[] = [];
    configureArtStore(testDeps(seen));
    setArtSource('scryfall');
    noteCards([{ functionalId: 'onakke_ogre', name: 'Onakke Ogre' }]);
    await settle();
    // The catalog ships real cards, so a card resolves by its own name (ADR 0026).
    expect(seen[0]).toBe(namedCardUrl('Onakke Ogre'));
    const key = artKeyFor('onakke_ogre');
    expect(key).toBeDefined();
    expect(textureForArtKey(key)?.texture).toBe(Texture.WHITE);
    expect(artUrlFor('onakke_ogre')).toBe('blob:stub');
  });

  it('falls back to the card name for a card outside the map', async () => {
    const seen: string[] = [];
    configureArtStore(testDeps(seen));
    setArtSource('scryfall');
    noteCards([{ functionalId: 'llanowar_elves', name: 'Llanowar Elves' }]);
    await settle();
    expect(seen[0]).toBe(namedCardUrl('Llanowar Elves'));
    expect(artKeyFor('llanowar_elves')).toBeDefined();
  });

  it('serves repeat sessions from the device cache without refetching', async () => {
    const cache = new MemoryArtCache();
    await cache.put('onakke_ogre#crop', {
      blob: new Blob(['cached']),
      source: 'scryfall',
      sourceName: 'Onakke Ogre',
      fetchedAt: 1,
    });
    const seen: string[] = [];
    configureArtStore({ ...testDeps(seen), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'onakke_ogre', name: 'Onakke Ogre' }]);
    await settle();
    expect(seen).toHaveLength(0);
    expect(artKeyFor('onakke_ogre')).toBeDefined();
  });

  it('persists a fresh download into the device cache', async () => {
    const cache = new MemoryArtCache();
    configureArtStore({ ...testDeps(), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    expect(await cache.keys()).toEqual(['shock#crop']);
    expect((await cache.get('shock#crop'))?.sourceName).toBe('Shock');
  });

  it('marks an unresolvable card failed and keeps the face procedural', async () => {
    configureArtStore({
      ...testDeps(),
      fetchLike: () =>
        Promise.resolve({
          ok: false,
          status: 404,
          json: () => Promise.resolve({}),
          blob: () => Promise.resolve(new Blob()),
        }),
    });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'mystery_card', name: 'Mystery Card' }]);
    await settle();
    expect(artKeyFor('mystery_card')).toBeUndefined();
    expect(artStatus()).toEqual({ total: 1, loaded: 0, failed: 1, pending: 0 });
  });

  it('notifies subscribers when art arrives and when the source changes', async () => {
    configureArtStore(testDeps());
    const listener = vi.fn();
    subscribeArt(listener);
    const before = getArtVersion();
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    expect(listener).toHaveBeenCalled();
    expect(getArtVersion()).toBeGreaterThan(before);
  });

  it('hides published art the moment the player returns to procedural', async () => {
    configureArtStore(testDeps());
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    expect(artKeyFor('shock')).toBeDefined();
    setArtSource('procedural');
    expect(artKeyFor('shock')).toBeUndefined();
    expect(artUrlFor('shock')).toBeUndefined();
  });

  it('spaces Scryfall requests per the API guidelines', async () => {
    const delays: number[] = [];
    configureArtStore({
      ...testDeps(),
      delay: (ms) => {
        delays.push(ms);
        return Promise.resolve();
      },
    });
    setArtSource('scryfall');
    noteCards([
      { functionalId: 'shock', name: 'Shock' },
      { functionalId: 'revitalize', name: 'Revitalize' },
    ]);
    await settle();
    // Each card waits between its lookup and image download, and the queue
    // spaces consecutive cards — at least three waits for two cards.
    expect(delays.length).toBeGreaterThanOrEqual(3);
    expect(delays.every((ms) => ms >= 100)).toBe(true);
  });

  it('clears downloaded art: cache emptied, textures dropped, faces procedural', async () => {
    const cache = new MemoryArtCache();
    configureArtStore({ ...testDeps(), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    const key = artKeyFor('shock');
    expect(key).toBeDefined();
    await clearDownloadedArt();
    expect(await cache.keys()).toEqual([]);
    expect(artKeyFor('shock')).toBeUndefined();
    expect(textureForArtKey(key)).toBeUndefined();
  });

  it('loads bundled art only for manifest-listed cards', async () => {
    const urls: string[] = [];
    configureArtStore({
      ...testDeps(),
      fetchLike: (url) => {
        urls.push(url);
        return Promise.resolve({
          ok: true,
          status: 200,
          json: () => Promise.resolve(['onakke_ogre']),
          blob: () => Promise.resolve(new Blob(['img'])),
        });
      },
    });
    setArtSource('bundled');
    noteCards([
      { functionalId: 'onakke_ogre', name: 'Onakke Ogre' },
      { functionalId: 'shock', name: 'Shock' },
    ]);
    await settle();
    expect(artKeyFor('onakke_ogre')).toBeDefined();
    expect(artKeyFor('shock')).toBeUndefined();
    expect(urls).toContain('/card-art/manifest.json');
    expect(urls).toContain('/card-art/onakke_ogre.jpg');
    expect(urls).not.toContain('/card-art/shock.jpg');
  });

  it('downloads the entire card image under full-card mode (ADR 0024)', async () => {
    const seen: string[] = [];
    const cache = new MemoryArtCache();
    configureArtStore({ ...testDeps(seen), cache });
    setArtSource('scryfall');
    setArtStyle('full');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    // The image URL fetched is the `normal` (whole card) one, cached under the
    // full-mode key, and the published record is flagged as a full-card face.
    expect(seen.some((url) => url.startsWith('https://img/full/'))).toBe(true);
    expect(await cache.keys()).toEqual(['shock#full']);
    const key = artKeyFor('shock');
    expect(key).toContain('scryfall:full');
    expect(textureForArtKey(key)?.full).toBe(true);
  });

  it('keeps the two presentation styles independently cached and keyed', async () => {
    const cache = new MemoryArtCache();
    configureArtStore({ ...testDeps(), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'shock', name: 'Shock' }]);
    await settle();
    const windowKey = artKeyFor('shock');
    expect(textureForArtKey(windowKey)?.full).toBe(false);
    setArtStyle('full');
    await settle();
    const fullKey = artKeyFor('shock');
    expect(fullKey).not.toBe(windowKey);
    expect(textureForArtKey(fullKey)?.full).toBe(true);
    expect((await cache.keys()).sort()).toEqual(['shock#crop', 'shock#full']);
    // Switching back is instant: the window texture is still published.
    setArtStyle('window');
    expect(artKeyFor('shock')).toBe(windowKey);
    expect(getArtStyle()).toBe('window');
  });

  it('collects every face-up card a view shows', () => {
    const cards = collectArtCards(SAMPLE_GAME_VIEW);
    const names = cards.map((card) => card.name);
    // Hand, battlefield, and graveyard faces are all wanted; ids ride along.
    expect(names).toContain('Llanowar Elves');
    expect(cards.some((card) => card.functionalId === 'llanowar_elves')).toBe(true);
    expect(cards.length).toBeGreaterThanOrEqual(SAMPLE_GAME_VIEW.my_hand.length);
  });
});
