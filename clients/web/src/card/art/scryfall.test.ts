import { describe, expect, it } from 'vitest';
import {
  artCropFromCard,
  fetchImageBlob,
  namedCardUrl,
  resolveArtCrop,
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

  it('extracts art_crop from a single-faced card', () => {
    expect(artCropFromCard({ image_uris: { art_crop: 'https://img/x.jpg' } })).toBe(
      'https://img/x.jpg',
    );
  });

  it('falls back to the first face carrying art on a multi-faced card', () => {
    const card = {
      card_faces: [{ image_uris: {} }, { image_uris: { art_crop: 'https://img/face.jpg' } }],
    };
    expect(artCropFromCard(card)).toBe('https://img/face.jpg');
  });

  it('returns null for a card without a usable illustration', () => {
    expect(artCropFromCard({})).toBeNull();
    expect(artCropFromCard(null)).toBeNull();
    expect(artCropFromCard('nonsense')).toBeNull();
  });

  it('resolves a name to its art_crop URL via the injected fetch', async () => {
    const seen: string[] = [];
    const fetchLike: FetchLike = (url) => {
      seen.push(url);
      return Promise.resolve(response({ image_uris: { art_crop: 'https://img/a.jpg' } }));
    };
    expect(await resolveArtCrop(fetchLike, 'Shock')).toBe('https://img/a.jpg');
    expect(seen[0]).toBe(namedCardUrl('Shock'));
  });

  it('returns null on a miss or a network refusal instead of throwing', async () => {
    const missing: FetchLike = () => Promise.resolve(response({}, false));
    expect(await resolveArtCrop(missing, 'Not A Card')).toBeNull();
    const refused: FetchLike = () => Promise.reject(new Error('offline'));
    expect(await resolveArtCrop(refused, 'Shock')).toBeNull();
    expect(await fetchImageBlob(refused, 'https://img/a.jpg')).toBeNull();
  });

  it('fetches image bytes as a blob', async () => {
    const fetchLike: FetchLike = () => Promise.resolve(response({}));
    const blob = await fetchImageBlob(fetchLike, 'https://img/a.jpg');
    expect(blob).toBeInstanceOf(Blob);
  });
});
