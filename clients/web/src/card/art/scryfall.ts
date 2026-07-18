/**
 * Scryfall resolution helpers (ADR 0024): pure functions that turn a real card
 * name into an illustration (`art_crop`) URL and fetch the image bytes. The
 * network function is always injected (`FetchLike`), so every test runs against
 * a stub and nothing here ever touches the real API in CI.
 *
 * Design constraints (ADR 0024):
 * - Requests are made by the PLAYER's browser after an explicit opt-in; the
 *   project never proxies, stores, or redistributes the images.
 * - Only `art_crop` (the bare illustration) is ever requested — the official
 *   card frame stays excluded; the illustration renders inside RUNE's own
 *   procedural frame.
 * - Callers must space requests by {@link SCRYFALL_REQUEST_SPACING_MS} per
 *   Scryfall's API guidelines (≤10 requests/second).
 */

/** The subset of `fetch` the resolvers use; injected so tests never hit the network. */
export type FetchLike = (
  url: string,
  init?: { headers?: Record<string, string> },
) => Promise<{
  ok: boolean;
  status: number;
  json(): Promise<unknown>;
  blob(): Promise<Blob>;
}>;

/** Scryfall API origin. */
export const SCRYFALL_API = 'https://api.scryfall.com';

/** Minimum delay between consecutive Scryfall requests (their guidelines ask 50–100ms). */
export const SCRYFALL_REQUEST_SPACING_MS = 120;

/** The exact-name lookup URL for a card (GET /cards/named?exact=…). */
export function namedCardUrl(name: string): string {
  return `${SCRYFALL_API}/cards/named?exact=${encodeURIComponent(name)}`;
}

/** Narrowing helper: an object with (optionally) an `art_crop` image URI. */
interface ImageUris {
  art_crop?: string;
}

/** The slice of a Scryfall card object the resolver reads. */
interface ScryfallCard {
  image_uris?: ImageUris;
  card_faces?: { image_uris?: ImageUris }[];
}

/**
 * Extract the illustration (`art_crop`) URL from a Scryfall card object,
 * falling back to the first face of a multi-faced card. `null` when the
 * response carries no usable illustration.
 */
export function artCropFromCard(card: unknown): string | null {
  if (typeof card !== 'object' || card === null) return null;
  const c = card as ScryfallCard;
  if (c.image_uris?.art_crop) return c.image_uris.art_crop;
  const face = c.card_faces?.find((f) => f.image_uris?.art_crop);
  return face?.image_uris?.art_crop ?? null;
}

/**
 * Resolve a real card name to its `art_crop` URL via the exact-name endpoint.
 * `null` on a miss (unknown name, network refusal, malformed body) — the caller
 * records the card as unavailable rather than retrying in a loop.
 */
export async function resolveArtCrop(fetchLike: FetchLike, name: string): Promise<string | null> {
  try {
    const response = await fetchLike(namedCardUrl(name), {
      headers: { Accept: 'application/json' },
    });
    if (!response.ok) return null;
    return artCropFromCard(await response.json());
  } catch {
    return null;
  }
}

/** Fetch an image URL to a Blob; `null` on any failure. */
export async function fetchImageBlob(fetchLike: FetchLike, url: string): Promise<Blob | null> {
  try {
    const response = await fetchLike(url);
    if (!response.ok) return null;
    return await response.blob();
  } catch {
    return null;
  }
}
