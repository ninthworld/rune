import { describe, expect, it } from 'vitest';
import {
  fetchImageBlob,
  imageFromCard,
  lookupUrl,
  namedCardUrl,
  printingUrl,
  resolveCardImage,
  type FetchLike,
} from './scryfall';

/** A minimal stub response for the injected fetch. */
function response(body: unknown, ok = true): Awaited<ReturnType<FetchLike>> {
  return {
    ok,
    status: ok ? 200 : 404,
    json: () => Promise.resolve(body),
    blob: () => Promise.resolve(new Blob(['img'])),
  };
}

describe('scryfall resolution (ADR 0024)', () => {
  it('builds the exact-name lookup URL with encoding', () => {
    expect(namedCardUrl('Jackal Pup')).toBe(
      'https://api.scryfall.com/cards/named?exact=Jackal%20Pup',
    );
    expect(namedCardUrl('Kitsa, Otterball Elite')).toContain('Kitsa%2C%20Otterball%20Elite');
  });

  it('builds the pinned-printing URL, and prefers the pin over the name', () => {
    expect(printingUrl('znr', '270')).toBe('https://api.scryfall.com/cards/znr/270');
    // A pinned reference selects a deliberate version (e.g. a full-art land).
    expect(lookupUrl({ name: 'Forest', set: 'znr', number: '270' })).toBe(
      'https://api.scryfall.com/cards/znr/270',
    );
    expect(lookupUrl({ name: 'Forest' })).toBe(namedCardUrl('Forest'));
  });

  it('extracts the requested image kind from a single-faced card', () => {
    const card = {
      image_uris: { art_crop: 'https://img/crop.jpg', normal: 'https://img/full.jpg' },
    };
    expect(imageFromCard(card, 'art_crop')).toBe('https://img/crop.jpg');
    expect(imageFromCard(card, 'normal')).toBe('https://img/full.jpg');
  });

  it('falls back to the first face carrying the image on a multi-faced card', () => {
    const card = {
      card_faces: [{ image_uris: {} }, { image_uris: { art_crop: 'https://img/face.jpg' } }],
    };
    expect(imageFromCard(card, 'art_crop')).toBe('https://img/face.jpg');
  });

  it('returns null for a card without a usable image', () => {
    expect(imageFromCard({}, 'art_crop')).toBeNull();
    expect(imageFromCard(null, 'normal')).toBeNull();
    expect(imageFromCard('nonsense', 'art_crop')).toBeNull();
  });

  it('resolves a reference to an image URL via the injected fetch', async () => {
    const seen: string[] = [];
    const fetchLike: FetchLike = (url) => {
      seen.push(url);
      return Promise.resolve(response({ image_uris: { art_crop: 'https://img/a.jpg' } }));
    };
    expect(await resolveCardImage(fetchLike, { name: 'Shock' }, 'art_crop')).toBe(
      'https://img/a.jpg',
    );
    expect(seen[0]).toBe(namedCardUrl('Shock'));
  });

  it('returns null on a miss or a network refusal instead of throwing', async () => {
    const missing: FetchLike = () => Promise.resolve(response({}, false));
    expect(await resolveCardImage(missing, { name: 'Not A Card' }, 'art_crop')).toBeNull();
    const refused: FetchLike = () => Promise.reject(new Error('offline'));
    expect(await resolveCardImage(refused, { name: 'Shock' }, 'normal')).toBeNull();
    expect(await fetchImageBlob(refused, 'https://img/a.jpg')).toBeNull();
  });

  it('fetches image bytes as a blob', async () => {
    const fetchLike: FetchLike = () => Promise.resolve(response({}));
    const blob = await fetchImageBlob(fetchLike, 'https://img/a.jpg');
    expect(blob).toBeInstanceOf(Blob);
  });
});
