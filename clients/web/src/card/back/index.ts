/**
 * The card back (`docs/design/card-representation.md` §13) — the one image every
 * hidden card on this device shows.
 *
 * - `cardBack.ts` — the shipped skin set (read from the production manifest),
 *   §13.2's fallback rule, and the single CSS property hidden surfaces render
 *   through. Takes no card, no zone, and no seat, which is how §13.1's
 *   hidden-information requirement is enforced structurally rather than by
 *   review.
 * - `cardBackStore.ts` — the device-local preference and the observed-failure
 *   set. No protocol surface; not game state.
 * - `useCardBack.ts` — the React binding a hidden-card surface spreads.
 */
export {
  CARD_BACK_SKINS,
  DEFAULT_CARD_BACK_ID,
  cardBackSkin,
  cardBackVars,
  defaultCardBackSkin,
  isCardBackId,
  resolveCardBackSkin,
} from './cardBack';
export type { CardBackSkin, CardBackVars } from './cardBack';

export {
  CARD_BACK_KEY,
  activeCardBack,
  getCardBackId,
  getCardBackVersion,
  noteCardBackFailed,
  resetCardBackStore,
  setCardBackId,
  subscribeCardBack,
} from './cardBackStore';

export { useCardBack } from './useCardBack';
export type { CardBackBinding } from './useCardBack';
