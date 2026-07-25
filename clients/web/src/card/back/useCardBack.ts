/**
 * The React binding for the device's card back (`card-representation.md` §13).
 *
 * A surface that shows hidden cards calls {@link useCardBack} and spreads the
 * returned vars onto its root; every hidden pile beneath it then draws the same
 * back. Re-resolves without a re-mount when the preference changes or a skin's
 * image fails, so §13.2's fallback is live rather than first-render-only.
 */
import { useSyncExternalStore } from 'react';
import { cardBackVars, type CardBackSkin, type CardBackVars } from './cardBack';
import { activeCardBack, getCardBackVersion, subscribeCardBack } from './cardBackStore';

/** The resolved back plus the custom properties hidden surfaces render through. */
export interface CardBackBinding {
  /** The resolved skin, or `undefined` when nothing loaded (procedural back). */
  skin: CardBackSkin | undefined;
  /** The `--card-back-image` property, ready to spread onto a surface root. */
  vars: CardBackVars;
}

/** Subscribe to the device's card back. */
export function useCardBack(): CardBackBinding {
  useSyncExternalStore(subscribeCardBack, getCardBackVersion, getCardBackVersion);
  const skin = activeCardBack();
  return { skin, vars: cardBackVars(skin) };
}
