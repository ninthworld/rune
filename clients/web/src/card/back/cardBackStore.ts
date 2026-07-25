/**
 * The device-local card-back preference and its failure fallback
 * (`docs/design/card-representation.md` §13.2).
 *
 * An observable singleton in the same idiom as `presentationSettings.ts` and the
 * ADR 0024 art store: guarded `localStorage`, a `useSyncExternalStore`
 * subscription so a change applies without a reload, and no protocol surface at
 * all. Nothing here is game state — the whole UI still rebuilds from one
 * `GameView` with this store empty, because an unresolved back leaves every
 * hidden pile on its procedural treatment.
 *
 * The store holds exactly two things: which skin the device chose, and which
 * skins have been observed to fail. It is never told what card a back hides, so
 * §13.1's hidden-information requirement cannot be violated by anything here.
 */
import {
  DEFAULT_CARD_BACK_ID,
  isCardBackId,
  resolveCardBackSkin,
  type CardBackSkin,
} from './cardBack';

/** The `localStorage` key §13.2 names. */
export const CARD_BACK_KEY = 'rune.cardBackSkin';

interface StoreState {
  /** The chosen skin id; the default until a preference is stored. */
  id: string;
  /** Skins whose image failed to load on this device, this session. */
  failed: Set<string>;
  listeners: Set<() => void>;
  /** Bumped on every change — the `useSyncExternalStore` snapshot. */
  version: number;
}

let state: StoreState | null = null;

/**
 * Read the stored preference, applying the same stale-key rule the environment
 * theme uses: an id that no longer names a shipped skin resolves to the default
 * **and rewrites the stored key**, so a removed skin self-heals on first read.
 */
function loadId(): string {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(CARD_BACK_KEY);
  } catch {
    return DEFAULT_CARD_BACK_ID;
  }
  if (raw === null) return DEFAULT_CARD_BACK_ID;
  if (isCardBackId(raw)) return raw;
  try {
    localStorage.removeItem(CARD_BACK_KEY);
  } catch {
    // Storage went away between the read and the write — nothing to correct.
  }
  return DEFAULT_CARD_BACK_ID;
}

function store(): StoreState {
  state ??= { id: loadId(), failed: new Set(), listeners: new Set(), version: 0 };
  return state;
}

/** Drop the preference and every observed failure — a fresh store (tests). */
export function resetCardBackStore(): void {
  state = null;
}

function bump(s: StoreState): void {
  s.version += 1;
  for (const listener of s.listeners) listener();
}

/** Subscribe to card-back changes; returns the unsubscribe function. */
export function subscribeCardBack(listener: () => void): () => void {
  const s = store();
  s.listeners.add(listener);
  return () => s.listeners.delete(listener);
}

/** Monotonic change counter (the `useSyncExternalStore` snapshot). */
export function getCardBackVersion(): number {
  return store().version;
}

/** The chosen skin id, whether or not its image has resolved. */
export function getCardBackId(): string {
  return store().id;
}

/**
 * Choose a card-back skin; persists and republishes. An unknown id is ignored
 * rather than stored, exactly as an unknown environment theme is.
 */
export function setCardBackId(id: string): void {
  if (!isCardBackId(id)) return;
  const s = store();
  if (s.id === id) return;
  s.id = id;
  try {
    localStorage.setItem(CARD_BACK_KEY, id);
  } catch {
    // Storage unavailable — the choice simply doesn't survive a reload.
  }
  bump(s);
}

/**
 * Record that a skin's image failed to load. The next resolution falls back to
 * the default with no layout change (§13.2); a failed default leaves every
 * hidden surface on its procedural back rather than on a hole.
 */
export function noteCardBackFailed(id: string): void {
  const s = store();
  if (s.failed.has(id)) return;
  s.failed.add(id);
  bump(s);
}

/**
 * The back every hidden card on this device shows right now, or `undefined`
 * when nothing resolved. Takes no card, no zone, and no seat — one device, one
 * back (§13.1).
 */
export function activeCardBack(): CardBackSkin | undefined {
  const s = store();
  return resolveCardBackSkin(s.id, s.failed);
}
