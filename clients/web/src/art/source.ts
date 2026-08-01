/**
 * Where an illustration is looked up, behind one interface.
 *
 * ADR 0012 names three sources and expects more: procedural (which resolves nothing and is not
 * one of these), art the project owns and may redistribute, and a third-party source the player
 * points their own browser at. Only the third exists today, and it is written against this
 * interface rather than reached for directly so the second can be added without the store, the
 * cache, or any component learning a second way to ask.
 *
 * What crosses this boundary is deliberately thin: a card's stable identity and its printed name
 * go out, image URLs come back. **No game data flows to a source beyond the name being
 * resolved** — not the board, not the opponent, not the format, not that a game is even in
 * progress. A source is handed one card at a time and cannot tell why.
 */

/** One card to resolve: the cache key, and the only thing a source is allowed to be told. */
export interface ArtRequest {
  /** `functional_id` — stable across games and builds, and what the cache is keyed by. */
  key: string
  /** The printed name, which is what a source looks up. */
  name: string
}

/** What a source found. Either may be absent; a card with neither is a miss. */
export interface ArtImages {
  /** The illustration alone, for SAGE's own frame. */
  window?: string
  /** The whole card image, frame and all. */
  full?: string
}

export interface ArtSource {
  id: 'scryfall'
  label: string
  /** Where a player can read the terms they are agreeing to. Shown, never fetched. */
  home: string
  /**
   * The shortest gap between two requests, per the source's own published guidelines. The store
   * honours this; a source that asked for none would still be queued one at a time.
   */
  minimumIntervalMs: number
  /**
   * Resolve one card, or `undefined` if the source has no image for it.
   *
   * Throwing and returning `undefined` mean different things to the store: a throw is "ask again
   * another time" (the network was down), `undefined` is "this card is not there" (a token, a
   * name this source does not know) and is remembered so it is not asked again.
   */
  resolve(request: ArtRequest, fetch: typeof globalThis.fetch): Promise<ArtImages | undefined>
}

/**
 * Scryfall, by exact name.
 *
 * The exact-name endpoint returns that source's default printing, which is the right answer for a
 * client that knows a card by identity and not by printing. ADR 0012 leaves room for an art map
 * that pins a specific set and collector number; nothing here needs to change to add one, because
 * a pinned printing is still one request for one card.
 *
 * The rate limit is Scryfall's own published guidance. It is enforced by the store rather than
 * here, because a limit that each source enforced for itself would be a limit the queue could
 * walk around by holding two of them.
 */
export const SCRYFALL: ArtSource = {
  id: 'scryfall',
  label: 'Scryfall',
  home: 'https://scryfall.com/docs/api',
  minimumIntervalMs: 100,

  async resolve({ name }, fetch) {
    const url = `https://api.scryfall.com/cards/named?exact=${encodeURIComponent(name)}`
    const response = await fetch(url, { headers: { Accept: 'application/json' } })

    // A name this source does not have is a miss, not a failure: it is the ordinary answer for a
    // token, for a card this project made up, and for anything printed since that source's data.
    if (response.status === 404) return undefined
    if (!response.ok) throw new Error(`art source answered ${response.status}`)

    return imagesOf((await response.json()) as ScryfallCard)
  },
}

/** Only the two fields this client reads. Everything else in the payload is ignored. */
interface ScryfallCard {
  image_uris?: { art_crop?: string; normal?: string }
  card_faces?: { image_uris?: { art_crop?: string; normal?: string } }[]
}

/**
 * The two images, from whichever shape the payload came in.
 *
 * A double-faced card carries its images per face rather than on the card, and the front face is
 * the one this client draws — SAGE's own view states what a permanent currently is, so choosing a
 * face here would be a presentation guess about a game fact the server already sent.
 */
function imagesOf(card: ScryfallCard): ArtImages | undefined {
  const uris = card.image_uris ?? card.card_faces?.[0]?.image_uris
  if (!uris) return undefined
  const images: ArtImages = { window: uris.art_crop, full: uris.normal }
  return images.window === undefined && images.full === undefined ? undefined : images
}
