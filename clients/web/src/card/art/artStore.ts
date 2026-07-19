/**
 * The client-local card-art store (ADR 0024): the one place that turns a card's
 * stable `functional_id` (the presentation identity the protocol reserved for
 * exactly this) into a loaded image texture for the Pixi factory and an object
 * URL for DOM surfaces.
 *
 * Pipelines (selected by the player, `artSettings.ts`):
 * - `procedural` — nothing loads; every card keeps the vector frame. Default.
 * - `bundled`    — project-owned illustrations shipped with the client under
 *                  `/card-art/<functional_id>.jpg`, listed by `/card-art/manifest.json`,
 *                  always rendered inside RUNE's frame.
 * - `scryfall`   — real card images the player opted into, fetched by their
 *                  browser from Scryfall (rate-limited), cached in IndexedDB on
 *                  their device. Two presentation styles: `window` downloads the
 *                  bare illustration (`art_crop`) for RUNE's frame; `full`
 *                  downloads the entire official card image (`normal`) which
 *                  replaces the procedural face wholesale.
 *
 * Everything here is presentation cache and preference — never game state. The
 * whole store can be cleared at any time and the UI remains fully reconstructable
 * from the next `GameView` (the client hard rule): cards simply render their
 * procedural faces again.
 *
 * The store is a module singleton with injectable effects (`configureArtStore`)
 * and a full reset (`resetArtStore`) so tests run deterministic and offline.
 */
import { Texture } from 'pixi.js';
import type { GameView } from '../../protocol';
import artMapJson from './artMap.json';
import {
  loadArtSource,
  loadArtStyle,
  saveArtSource,
  saveArtStyle,
  type ArtSource,
  type ArtStyle,
} from './artSettings';
import { openArtCache, type ArtBlobCache } from './artCache';
import {
  fetchImageBlob,
  resolveCardImage,
  SCRYFALL_REQUEST_SPACING_MS,
  type FetchLike,
  type PrintingRef,
} from './scryfall';

/**
 * The functional_id → printing-pin mapping. The embedded catalog now ships real
 * cards (functional data only — no Oracle text, art, or branding; ADR 0026), so
 * a card resolves by its own name and this map is empty by default. An entry may
 * still pin a specific printing (`set` + collector `number`) to select a
 * deliberate version — e.g. a full-art land — instead of Scryfall's default
 * printing; absent an entry, lookup falls back to the card's own name.
 */
const ART_MAP: Record<string, PrintingRef> = artMapJson;

/** A decoded image: the texture for Pixi plus a URL for DOM surfaces. */
export interface LoadedArt {
  /** Decoded texture the card factory draws. */
  texture: Texture;
  /** Object URL (or asset URL) for `<img>` surfaces like the inspector. */
  url: string;
}

/** A published image: the decoded art plus how the face should present it. */
export interface PublishedArt extends LoadedArt {
  /**
   * Whether this is an ENTIRE official card image (full-card mode): the factory
   * renders it as the whole face and suppresses RUNE's procedural name band,
   * pips, and type line. `false` means an illustration for RUNE's art window.
   */
  full: boolean;
}

/**
 * One loading pipeline: a source plus (for Scryfall) its presentation style.
 * Kept separate per style because the two styles download different images.
 */
type ArtPipeline = 'bundled' | 'scryfall:window' | 'scryfall:full';

/** Per-card load state under one pipeline. */
interface ArtEntry {
  state: 'loading' | 'loaded' | 'failed';
  art?: PublishedArt;
  /** Unique key for the published texture, embedded in the card's visual signature. */
  key?: string;
}

/** Injectable effects, all defaulted for the browser and overridable in tests. */
export interface ArtStoreDeps {
  /** Network access; tests inject a stub and never touch the real API. */
  fetchLike: FetchLike;
  /** Device-local blob cache for downloaded art. */
  cache: ArtBlobCache;
  /** Decode a blob into a texture + display URL; `null` on failure. */
  loadArt: (blob: Blob) => Promise<LoadedArt | null>;
  /** Waiter used to space Scryfall requests; tests resolve immediately. */
  delay: (ms: number) => Promise<void>;
  /** Wall clock for cache provenance stamps. */
  now: () => number;
}

/** Browser defaults: real fetch, IndexedDB cache, object-URL texture decode. */
function defaultDeps(): ArtStoreDeps {
  return {
    fetchLike: (url, init) => fetch(url, init),
    cache: openArtCache(),
    loadArt: async (blob) => {
      let url: string;
      try {
        url = URL.createObjectURL(blob);
      } catch {
        return null;
      }
      try {
        return { texture: await Texture.fromURL(url), url };
      } catch {
        URL.revokeObjectURL(url);
        return null;
      }
    },
    delay: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    now: () => Date.now(),
  };
}

/** The whole mutable store state, swapped atomically by {@link resetArtStore}. */
interface StoreState {
  deps: ArtStoreDeps;
  source: ArtSource;
  style: ArtStyle;
  /** Cards the current views want art for: functional_id → display name. */
  wanted: Map<string, string>;
  /** Load state per pipeline so switching source/style re-resolves cleanly. */
  entries: Map<ArtPipeline, Map<string, ArtEntry>>;
  /** Published textures by signature key (what the card factory looks up). */
  textures: Map<string, PublishedArt>;
  /** Monotonic counter making every published texture key unique. */
  generation: number;
  /** Change counter for `useSyncExternalStore` subscribers. */
  version: number;
  listeners: Set<() => void>;
  /** Tail of the Scryfall request chain (spacing per API guidelines). */
  scryfallQueue: Promise<void>;
  /** Bundled manifest ids once fetched; `null` before/without one. */
  bundledManifest: Set<string> | null;
  bundledManifestRequested: boolean;
}

/** Fresh state with lazy default deps (constructed on first use, not import). */
function freshState(deps?: Partial<ArtStoreDeps>): StoreState {
  return {
    deps: { ...defaultDeps(), ...deps },
    source: loadArtSource(),
    style: loadArtStyle(),
    wanted: new Map(),
    entries: new Map(),
    textures: new Map(),
    generation: 0,
    version: 0,
    listeners: new Set(),
    scryfallQueue: Promise.resolve(),
    bundledManifest: null,
    bundledManifestRequested: false,
  };
}

let state: StoreState | null = null;

/** The live store state, constructed on first access. */
function store(): StoreState {
  state ??= freshState();
  return state;
}

/** Replace effect implementations (tests). Resets all load state. */
export function configureArtStore(deps: Partial<ArtStoreDeps>): void {
  state = freshState(deps);
}

/** Drop every entry, texture, and listener — a fresh store (tests). */
export function resetArtStore(): void {
  state = null;
}

/** Notify subscribers that art availability changed. */
function bump(s: StoreState): void {
  s.version += 1;
  for (const listener of s.listeners) listener();
}

/** Subscribe to art changes; returns the unsubscribe function. */
export function subscribeArt(listener: () => void): () => void {
  const s = store();
  s.listeners.add(listener);
  return () => s.listeners.delete(listener);
}

/** Monotonic change counter (the `useSyncExternalStore` snapshot). */
export function getArtVersion(): number {
  return store().version;
}

/** The active art source. */
export function getArtSource(): ArtSource {
  return store().source;
}

/** The active presentation style (meaningful under the `scryfall` source). */
export function getArtStyle(): ArtStyle {
  return store().style;
}

/** The pipeline the active source/style selects, or `null` under procedural. */
function activePipeline(s: StoreState): ArtPipeline | null {
  if (s.source === 'procedural') return null;
  if (s.source === 'bundled') return 'bundled';
  return s.style === 'full' ? 'scryfall:full' : 'scryfall:window';
}

/**
 * Switch the active art pipeline (the settings surface's radio). Persists the
 * device preference and kicks loads for every wanted card under the new source.
 */
export function setArtSource(source: ArtSource): void {
  const s = store();
  if (s.source === source) return;
  s.source = source;
  saveArtSource(source);
  for (const [functionalId, name] of s.wanted) ensureLoading(s, functionalId, name);
  bump(s);
}

/**
 * Switch the presentation style: illustration in RUNE's frame (`window`) or the
 * entire official card image (`full`). Persists the device preference; under
 * the `scryfall` source the newly-needed image kind starts loading for every
 * wanted card (the styles cache independently, so switching back is instant).
 */
export function setArtStyle(style: ArtStyle): void {
  const s = store();
  if (s.style === style) return;
  s.style = style;
  saveArtStyle(style);
  for (const [functionalId, name] of s.wanted) ensureLoading(s, functionalId, name);
  bump(s);
}

/** The per-pipeline entry map, created on demand. */
function entriesFor(s: StoreState, pipeline: ArtPipeline): Map<string, ArtEntry> {
  let map = s.entries.get(pipeline);
  if (!map) {
    map = new Map();
    s.entries.set(pipeline, map);
  }
  return map;
}

/**
 * Tell the store which cards the UI is currently showing (called from the view
 * render path). Idempotent and cheap; under an active source each new card
 * starts loading in the background and republishes via the subscription.
 */
export function noteCards(cards: { functionalId?: string; name: string }[]): void {
  const s = store();
  for (const card of cards) {
    if (!card.functionalId) continue;
    if (!s.wanted.has(card.functionalId)) s.wanted.set(card.functionalId, card.name);
    ensureLoading(s, card.functionalId, card.name);
  }
}

/**
 * The signature key for a card's currently-published image, or `undefined`
 * when none (procedural source, still loading, or failed). The key goes into
 * `CardDisplayData.artKey`, so a card's visual signature changes — and the
 * reconciler rebuilds it — exactly when its art (or the presentation style)
 * changes.
 */
export function artKeyFor(functionalId: string | undefined): string | undefined {
  if (!functionalId) return undefined;
  const s = store();
  const pipeline = activePipeline(s);
  if (!pipeline) return undefined;
  return entriesFor(s, pipeline).get(functionalId)?.key;
}

/** The published image for a signature key (the card factory's lookup). */
export function textureForArtKey(artKey: string | undefined): PublishedArt | undefined {
  if (!artKey) return undefined;
  return store().textures.get(artKey);
}

/** Display URL for DOM surfaces (the inspector), under the active pipeline. */
export function artUrlFor(functionalId: string | undefined): string | undefined {
  if (!functionalId) return undefined;
  const s = store();
  const pipeline = activePipeline(s);
  if (!pipeline) return undefined;
  return entriesFor(s, pipeline).get(functionalId)?.art?.url;
}

/** Progress over the wanted set under the active pipeline (settings panel). */
export function artStatus(): { total: number; loaded: number; failed: number; pending: number } {
  const s = store();
  const pipeline = activePipeline(s);
  const map = pipeline ? entriesFor(s, pipeline) : new Map<string, ArtEntry>();
  let loaded = 0;
  let failed = 0;
  let pending = 0;
  for (const functionalId of s.wanted.keys()) {
    const entry = map.get(functionalId);
    if (entry?.state === 'loaded') loaded += 1;
    else if (entry?.state === 'failed') failed += 1;
    else pending += 1;
  }
  return { total: s.wanted.size, loaded, failed, pending };
}

/**
 * Clear every downloaded image from the device cache and drop the published
 * Scryfall textures (both presentation styles). Cards fall back to procedural
 * faces on the next render; nothing else changes (presentation cache only).
 */
export async function clearDownloadedArt(): Promise<void> {
  const s = store();
  try {
    await s.deps.cache.clear();
  } catch {
    // A failed clear leaves stale cache entries; the UI state still resets.
  }
  for (const pipeline of ['scryfall:window', 'scryfall:full'] as const) {
    const map = entriesFor(s, pipeline);
    for (const entry of map.values()) {
      if (entry.key) s.textures.delete(entry.key);
      if (entry.art) revokeUrl(entry.art.url);
    }
    map.clear();
  }
  bump(s);
}

/** Best-effort object-URL revocation (asset URLs and jsdom tolerate failure). */
function revokeUrl(url: string): void {
  try {
    URL.revokeObjectURL(url);
  } catch {
    // Not an object URL, or the environment lacks revocation — harmless.
  }
}

/** Device storage usage/quota estimate for the settings panel; `null` when unknown. */
export async function storageEstimate(): Promise<{ usage: number; quota: number } | null> {
  try {
    const estimate = await navigator.storage.estimate();
    if (estimate.usage === undefined || estimate.quota === undefined) return null;
    return { usage: estimate.usage, quota: estimate.quota };
  } catch {
    return null;
  }
}

/** Publish a decoded image for a card under a pipeline and notify. */
function publish(
  s: StoreState,
  pipeline: ArtPipeline,
  functionalId: string,
  art: LoadedArt | null,
): void {
  const entry: ArtEntry = art
    ? {
        state: 'loaded',
        art: { ...art, full: pipeline === 'scryfall:full' },
        key: `${pipeline}:${functionalId}#${(s.generation += 1)}`,
      }
    : { state: 'failed' };
  entriesFor(s, pipeline).set(functionalId, entry);
  if (entry.key && entry.art) s.textures.set(entry.key, entry.art);
  bump(s);
}

/** Start (once) the background load of one card's art under the active pipeline. */
function ensureLoading(s: StoreState, functionalId: string, name: string): void {
  const pipeline = activePipeline(s);
  if (!pipeline) return;
  const map = entriesFor(s, pipeline);
  if (map.has(functionalId)) return;
  map.set(functionalId, { state: 'loading' });
  const job =
    pipeline === 'bundled'
      ? loadBundled(s, functionalId)
      : loadScryfall(s, pipeline, functionalId, name);
  void job.catch(() => publish(s, pipeline, functionalId, null));
}

/** Load one bundled illustration (`/card-art/<id>.jpg`, gated by the manifest). */
async function loadBundled(s: StoreState, functionalId: string): Promise<void> {
  const manifest = await bundledManifest(s);
  if (!manifest.has(functionalId)) {
    publish(s, 'bundled', functionalId, null);
    return;
  }
  const blob = await fetchImageBlob(s.deps.fetchLike, `/card-art/${functionalId}.jpg`);
  publish(s, 'bundled', functionalId, blob ? await s.deps.loadArt(blob) : null);
}

/** Fetch (once) the bundled-art manifest; an unreachable manifest means "none". */
async function bundledManifest(s: StoreState): Promise<Set<string>> {
  if (s.bundledManifest) return s.bundledManifest;
  if (!s.bundledManifestRequested) {
    s.bundledManifestRequested = true;
    try {
      const response = await s.deps.fetchLike('/card-art/manifest.json', {
        headers: { Accept: 'application/json' },
      });
      const ids = response.ok ? await response.json() : [];
      s.bundledManifest = new Set(Array.isArray(ids) ? ids.map(String) : []);
    } catch {
      s.bundledManifest = new Set();
    }
  }
  return s.bundledManifest ?? new Set();
}

/**
 * Load one Scryfall image for a pipeline: device cache first, then a
 * rate-limited resolve + download, persisting the blob so the next session
 * skips the network. The two styles download different image kinds and cache
 * under different keys, so each is fetched at most once per device.
 */
async function loadScryfall(
  s: StoreState,
  pipeline: 'scryfall:window' | 'scryfall:full',
  functionalId: string,
  name: string,
): Promise<void> {
  const full = pipeline === 'scryfall:full';
  const cacheKey = `${functionalId}#${full ? 'full' : 'crop'}`;
  let cached;
  try {
    cached = await s.deps.cache.get(cacheKey);
  } catch {
    cached = undefined;
  }
  if (cached) {
    publish(s, pipeline, functionalId, await s.deps.loadArt(cached.blob));
    return;
  }

  const ref: PrintingRef = ART_MAP[functionalId] ?? { name };
  const blob = await enqueueScryfall(s, async () => {
    const imageUrl = await resolveCardImage(s.deps.fetchLike, ref, full ? 'normal' : 'art_crop');
    if (!imageUrl) return null;
    await s.deps.delay(SCRYFALL_REQUEST_SPACING_MS);
    return fetchImageBlob(s.deps.fetchLike, imageUrl);
  });
  if (blob) {
    try {
      await s.deps.cache.put(cacheKey, {
        blob,
        source: 'scryfall',
        sourceName: ref.name,
        fetchedAt: s.deps.now(),
      });
    } catch {
      // Persisting is best-effort; the session still gets its texture.
    }
  }
  publish(s, pipeline, functionalId, blob ? await s.deps.loadArt(blob) : null);
}

/**
 * Every face-up card a view shows, as (functional_id, name) pairs for
 * {@link noteCards}: the receiver's hand, all permanents, and the public
 * graveyard/exile piles. Pure projection of the view — no rules, no filtering
 * beyond "has a face the client renders".
 */
export function collectArtCards(view: GameView): { functionalId?: string; name: string }[] {
  const cards: { functionalId?: string; name: string }[] = [];
  const push = (card: { functional_id?: string; name: string }): void => {
    cards.push({ functionalId: card.functional_id, name: card.name });
  };
  for (const card of view.my_hand) push(card);
  for (const permanent of view.battlefield) push(permanent.card);
  for (const pile of view.graveyards) for (const card of pile.cards) push(card);
  for (const pile of view.exile) for (const card of pile.cards) push(card);
  return cards;
}

/** Chain a job onto the Scryfall queue with the mandated request spacing. */
function enqueueScryfall<T>(s: StoreState, job: () => Promise<T>): Promise<T> {
  const run = s.scryfallQueue.then(job);
  s.scryfallQueue = run
    .then(() => s.deps.delay(SCRYFALL_REQUEST_SPACING_MS))
    .catch(() => undefined);
  return run;
}
