/**
 * The card back (`docs/design/card-representation.md` §13): the one image every
 * hidden card on this device shows.
 *
 * **Hidden-information safety is the whole point of this module, so it is
 * expressed as a shape rather than as a review note.** Nothing here accepts a
 * card, a zone, a seat, a count, or a view — the only input is the device's own
 * skin preference. A back therefore *cannot* vary with the card it hides,
 * because no function on this path is ever told what that card is. §13.1's
 * further requirement, that the back not leak through its **rotation**, is met
 * the same way: the back is published as one CSS custom property for the whole
 * surface, carries no transform of its own, and is applied to piles that are
 * never rotated (`zone-geography.md` §1 fact 2).
 *
 * The skin set is **read** from the shipped production manifest
 * (`src/assets/productionManifest.ts`), never transcribed, so the content hashes
 * cannot rot. §13.2's contract, in full:
 *
 * - selection is a device-local presentation preference (`rune.cardBackSkin`),
 *   in the ADR 0024 / ADR 0027 idiom, and claims nothing about any other player;
 * - the protocol is untouched — per-player backs visible to opponents would need
 *   an explicit future wire field and are **never inferred**;
 * - a missing, malformed, or failed skin falls back to the default **with no
 *   layout change**, which is why the fallback here changes one URL and nothing
 *   else, and why a total failure publishes *no* image rather than an empty one:
 *   the pile keeps its procedural back and its rect is untouched either way.
 *
 * Face-down *permanents* are not this module's business: no `GameView` field
 * distinguishes one (issue #551), and face-down-ness is never inferred.
 */
import type { CSSProperties } from 'react';
import {
  PRODUCTION_CARD_BACKS,
  PRODUCTION_CARD_BACK_DEFAULT,
} from '../../assets/productionManifest';

/** One card-back skin as the shipped manifest records it. */
export interface CardBackSkin {
  /** Stable skin id — the manifest key and the stored preference value. */
  id: string;
  /** Display label. */
  label: string;
  /** Resolved, content-hashed URL. */
  src: string;
  /** Intrinsic width in px. */
  width: number;
  /** Intrinsic height in px. */
  height: number;
}

/** Every shipped skin, in manifest order, id included. */
export const CARD_BACK_SKINS: readonly CardBackSkin[] = Object.entries(PRODUCTION_CARD_BACKS).map(
  ([id, skin]) => ({ id, ...skin }),
);

/** The id of the default back — the fallback for every failure mode of §13.2. */
export const DEFAULT_CARD_BACK_ID = PRODUCTION_CARD_BACK_DEFAULT;

/** Whether an id names a shipped skin. */
export function isCardBackId(value: unknown): value is string {
  return typeof value === 'string' && CARD_BACK_SKINS.some((skin) => skin.id === value);
}

/**
 * The skin an id names, or `undefined`. Total and pure; resolution of an
 * *unknown* id to the default is {@link resolveCardBackSkin}'s job, so a caller
 * can tell "not shipped" from "shipped and chosen".
 */
export function cardBackSkin(id: string | null | undefined): CardBackSkin | undefined {
  if (typeof id !== 'string') return undefined;
  return CARD_BACK_SKINS.find((skin) => skin.id === id);
}

/** The default skin, or `undefined` if no card back shipped at all. */
export function defaultCardBackSkin(): CardBackSkin | undefined {
  return cardBackSkin(DEFAULT_CARD_BACK_ID);
}

/**
 * Resolve a requested skin against §13.2's fallback rule: an unknown, malformed,
 * or failed id yields the **default** back, and a failed default yields
 * `undefined` — the surface then keeps its procedural back, which is a colour
 * change and never a layout change.
 */
export function resolveCardBackSkin(
  id: string | null | undefined,
  failed: ReadonlySet<string> = new Set(),
): CardBackSkin | undefined {
  const requested = cardBackSkin(id);
  if (requested && !failed.has(requested.id)) return requested;
  const fallback = defaultCardBackSkin();
  if (fallback && !failed.has(fallback.id)) return fallback;
  return undefined;
}

/** Custom-property style object usable as an inline `style`. */
export type CardBackVars = CSSProperties & Record<`--${string}`, string | number>;

/**
 * The CSS custom properties a hidden-card surface renders through.
 *
 * One property, one value, for the whole surface — which is exactly why a back
 * cannot encode anything: there is no per-card channel to encode it on. With no
 * skin resolved the property is the CSS-wide `none`, so the `background`
 * shorthand that layers it over the procedural back simply draws no image and
 * the pile's box is byte-identical.
 */
export function cardBackVars(skin: CardBackSkin | undefined): CardBackVars {
  return { '--card-back-image': skin === undefined ? 'none' : `url("${skin.src}")` };
}
