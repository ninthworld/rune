/**
 * Seat **portrait plate assignment** (`docs/design/seat-identity.md` §1.3, §10.1).
 *
 * The production plates shipped with issue #555 and are declared in
 * `public/assets/manifest.json`: one `local` plate (the deliberately faceless
 * hooded figure of the baseline's panels 3–4) and eight `opponents` plates. The
 * manifest is imported rather than fetched so a plate's content-hashed URL can
 * never drift from the file that ships it, and so staging stays **pure and
 * synchronous** — the cluster is a function of the view, not of a network race.
 *
 * Three rules govern the assignment, and all three are §1.3's:
 *
 * - **There is no protocol field selecting a portrait.** Assignment is
 *   presentation, keyed by the seat's index in `seat_order`.
 * - **It is stable for the whole game.** `seat_order` is server-authoritative,
 *   carries every seat including eliminated ones, and never reorders, so a
 *   seat's face cannot change between views. Nothing here reads `opponents[]`
 *   array order, which a mid-game elimination can shorten.
 * - **The receiver's plate is the `local` one**; every other seat draws from
 *   `opponents[]`. A spectator has no receiver, so every seat draws from
 *   `opponents[]` — which is exactly what §2's spectator row requires.
 *
 * When no plate resolves — an older manifest, a ninth opponent, a seat the
 * server left out of `seat_order` — the aperture falls back to the procedural
 * **rune monogram**, which is likewise derived from the player id alone and is
 * therefore just as stable. That path is permanent: it is what a portrait-less
 * build renders, and it never becomes a bare counter.
 */
import assetManifest from '../../public/assets/manifest.json';
import type { PlayerId } from '../protocol';

/** One assigned portrait plate: its manifest key and the URL it ships at. */
export interface SeatPortrait {
  /** The manifest key — stable identity, independent of the content hash. */
  key: string;
  /** The plate's URL, straight from the shipped manifest. */
  src: string;
}

interface PortraitManifest {
  fallback: string | null;
  local?: SeatPortrait;
  opponents?: SeatPortrait[];
}

const PORTRAITS: PortraitManifest = (assetManifest as { portraits?: PortraitManifest })
  .portraits ?? {
  fallback: null,
};

/** The receiver's plate, or `undefined` where the manifest ships none. */
export const LOCAL_PORTRAIT: SeatPortrait | undefined = PORTRAITS.local;

/** The opponent plate cycle, in manifest order. */
export const OPPONENT_PORTRAITS: readonly SeatPortrait[] = PORTRAITS.opponents ?? [];

/**
 * The procedural monogram alphabet — Elder Futhark runes, which read as marks
 * rather than as letters and so never look like a truncated name. One is chosen
 * per player id, so the fallback is as stable as a plate.
 */
export const PORTRAIT_MONOGRAMS = [
  'ᚠ',
  'ᚢ',
  'ᚦ',
  'ᚨ',
  'ᚱ',
  'ᚲ',
  'ᚷ',
  'ᚹ',
  'ᚺ',
  'ᚾ',
  'ᛁ',
  'ᛃ',
] as const;

/**
 * FNV-1a over the player id — a stable, view-independent index source for the
 * seats `seat_order` does not place (an older server, a mid-update race). It is
 * presentation only: nothing about the game depends on which face a seat wears.
 */
export function seatHash(seat: PlayerId): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < seat.length; i += 1) {
    hash ^= seat.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** The stable seat index a portrait is keyed by (§1.3), or `-1` when unplaced. */
export function portraitSeatIndex(seatOrder: readonly PlayerId[], seat: PlayerId): number {
  return seatOrder.indexOf(seat);
}

/**
 * The plate one seat wears. `local` is the receiver's own seat (`view.you`);
 * every other seat, including every seat a spectator sees, draws from the
 * opponent cycle at its `seat_order` index — a value that is fixed for the whole
 * game, so the face a player learns stays that player's face.
 */
export function portraitFor(
  seat: PlayerId,
  seatOrder: readonly PlayerId[],
  local: boolean,
): SeatPortrait | undefined {
  if (local) return LOCAL_PORTRAIT;
  if (OPPONENT_PORTRAITS.length === 0) return undefined;
  const index = portraitSeatIndex(seatOrder, seat);
  const key = index >= 0 ? index : seatHash(seat);
  return OPPONENT_PORTRAITS[key % OPPONENT_PORTRAITS.length];
}

/** The procedural monogram a portrait-less aperture draws (§1.3 fallback). */
export function monogramFor(seat: PlayerId): string {
  return PORTRAIT_MONOGRAMS[seatHash(seat) % PORTRAIT_MONOGRAMS.length]!;
}
