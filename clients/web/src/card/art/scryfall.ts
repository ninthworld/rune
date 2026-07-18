/**
 * Scryfall resolution helpers (ADR 0024): pure functions that turn a real-card
 * reference into an image URL and fetch the image bytes. The network function
 * is always injected (`FetchLike`), so every test runs against a stub and
 * nothing here ever touches the real API in CI.
 *
 * Design constraints (ADR 0024):
 * - Requests are made by the PLAYER's browser after an explicit opt-in; the
 *   project never proxies, stores, or redistributes the images.
 * - Two image kinds, matching the two presentation styles: `art_crop` (the bare
 *   illustration, rendered inside RUNE's own frame) and `normal` (the entire
 *   official card image, used only when the player chose full-card mode).
 * - A reference may pin a specific printing (`set` + collector `number`) — how
 *   a particular version (e.g. a full-art land) is selected deliberately
 *   instead of taking Scryfall's default printing for the name.
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

/**
 * The image kinds the client ever requests: `art_crop` for the illustration
 * window, `normal` for the entire card face. Nothing else (no `png`, no
 * `border_crop`) so the download surface stays minimal and predictable.
 */
export type ScryfallImageKind = 'art_crop' | 'normal';

/**
 * A real-card reference to resolve: an exact name, optionally pinned to one
 * printing. Pinning selects a deliberate version — full-art basics, a specific
 * illustration — where the name alone would take Scryfall's default printing.
 */
export interface PrintingRef {
  /** Exact real card name (the `?exact=` lookup). */
  name: string;
  /** Set code of a pinned printing, e.g. `"znr"`. Requires `number`. */
  set?: string;
  /** Collector number of the pinned printing within `set`. */
  number?: string;
}

/** The exact-name lookup URL for a card (GET /cards/named?exact=…). */
export function namedCardUrl(name: string): string {
  return `${SCRYFALL_API}/cards/named?exact=${encodeURIComponent(name)}`;
}

/** The pinned-printing lookup URL (GET /cards/{set}/{number}). */
export function printingUrl(set: string, number: string): string {
  return `${SCRYFALL_API}/cards/${encodeURIComponent(set)}/${encodeURIComponent(number)}`;
}

/** The lookup URL for a reference: its pinned printing when set, else its name. */
export function lookupUrl(ref: PrintingRef): string {
  return ref.set && ref.number ? printingUrl(ref.set, ref.number) : namedCardUrl(ref.name);
}

/** Narrowing helper: an object with (optionally) the image URIs we read. */
type ImageUris = Partial<Record<ScryfallImageKind, string>>;

/** The slice of a Scryfall card object the resolver reads. */
interface ScryfallCard {
  image_uris?: ImageUris;
  card_faces?: { image_uris?: ImageUris }[];
}

/**
 * Extract an image URL of the requested kind from a Scryfall card object,
 * falling back to the first face of a multi-faced card. `null` when the
 * response carries no usable image.
 */
export function imageFromCard(card: unknown, kind: ScryfallImageKind): string | null {
  if (typeof card !== 'object' || card === null) return null;
  const c = card as ScryfallCard;
  if (c.image_uris?.[kind]) return c.image_uris[kind];
  const face = c.card_faces?.find((f) => f.image_uris?.[kind]);
  return face?.image_uris?.[kind] ?? null;
}

/**
 * Resolve a card reference to an image URL of the requested kind. `null` on a
 * miss (unknown name/printing, network refusal, malformed body) — the caller
 * records the card as unavailable rather than retrying in a loop.
 */
export async function resolveCardImage(
  fetchLike: FetchLike,
  ref: PrintingRef,
  kind: ScryfallImageKind,
): Promise<string | null> {
  try {
    const response = await fetchLike(lookupUrl(ref), {
      headers: { Accept: 'application/json' },
    });
    if (!response.ok) return null;
    return imageFromCard(await response.json(), kind);
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
