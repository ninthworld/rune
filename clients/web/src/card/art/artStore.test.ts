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
  getArtVersion,
  noteCards,
  resetArtStore,
  setArtSource,
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
          url.endsWith('manifest.json') ? [] : { image_uris: { art_crop: `https://img/${url}` } },
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
    noteCards([{ functionalId: 'emberfang_jackal', name: 'Emberfang Jackal' }]);
    await settle();
    expect(artKeyFor('emberfang_jackal')).toBeUndefined();
    expect(artUrlFor('emberfang_jackal')).toBeUndefined();
  });

  it('resolves a mapped stand-in card through its real-card counterpart', async () => {
    const seen: string[] = [];
    configureArtStore(testDeps(seen));
    setArtSource('scryfall');
    noteCards([{ functionalId: 'emberfang_jackal', name: 'Emberfang Jackal' }]);
    await settle();
    // The stand-in resolves via its artMap counterpart, not its invented name.
    expect(seen[0]).toBe(namedCardUrl('Jackal Pup'));
    const key = artKeyFor('emberfang_jackal');
    expect(key).toBeDefined();
    expect(textureForArtKey(key)?.texture).toBe(Texture.WHITE);
    expect(artUrlFor('emberfang_jackal')).toBe('blob:stub');
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
    await cache.put('emberfang_jackal', {
      blob: new Blob(['cached']),
      source: 'scryfall',
      sourceName: 'Jackal Pup',
      fetchedAt: 1,
    });
    const seen: string[] = [];
    configureArtStore({ ...testDeps(seen), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'emberfang_jackal', name: 'Emberfang Jackal' }]);
    await settle();
    expect(seen).toHaveLength(0);
    expect(artKeyFor('emberfang_jackal')).toBeDefined();
  });

  it('persists a fresh download into the device cache', async () => {
    const cache = new MemoryArtCache();
    configureArtStore({ ...testDeps(), cache });
    setArtSource('scryfall');
    noteCards([{ functionalId: 'cinder_shock', name: 'Cinder Shock' }]);
    await settle();
    expect(await cache.keys()).toEqual(['cinder_shock']);
    expect((await cache.get('cinder_shock'))?.sourceName).toBe('Shock');
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
    noteCards([{ functionalId: 'cinder_shock', name: 'Cinder Shock' }]);
    await settle();
    expect(listener).toHaveBeenCalled();
    expect(getArtVersion()).toBeGreaterThan(before);
  });

  it('hides published art the moment the player returns to procedural', async () => {
    configureArtStore(testDeps());
    setArtSource('scryfall');
    noteCards([{ functionalId: 'cinder_shock', name: 'Cinder Shock' }]);
    await settle();
    expect(artKeyFor('cinder_shock')).toBeDefined();
    setArtSource('procedural');
    expect(artKeyFor('cinder_shock')).toBeUndefined();
    expect(artUrlFor('cinder_shock')).toBeUndefined();
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
      { functionalId: 'cinder_shock', name: 'Cinder Shock' },
      { functionalId: 'soothing_balm', name: 'Soothing Balm' },
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
    noteCards([{ functionalId: 'cinder_shock', name: 'Cinder Shock' }]);
    await settle();
    const key = artKeyFor('cinder_shock');
    expect(key).toBeDefined();
    await clearDownloadedArt();
    expect(await cache.keys()).toEqual([]);
    expect(artKeyFor('cinder_shock')).toBeUndefined();
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
          json: () => Promise.resolve(['emberfang_jackal']),
          blob: () => Promise.resolve(new Blob(['img'])),
        });
      },
    });
    setArtSource('bundled');
    noteCards([
      { functionalId: 'emberfang_jackal', name: 'Emberfang Jackal' },
      { functionalId: 'cinder_shock', name: 'Cinder Shock' },
    ]);
    await settle();
    expect(artKeyFor('emberfang_jackal')).toBeDefined();
    expect(artKeyFor('cinder_shock')).toBeUndefined();
    expect(urls).toContain('/card-art/manifest.json');
    expect(urls).toContain('/card-art/emberfang_jackal.jpg');
    expect(urls).not.toContain('/card-art/cinder_shock.jpg');
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
