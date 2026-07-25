/**
 * The card-art primitive (issue #527) — the ONE element that paints a card
 * illustration anywhere in the client. Every art surface (the frame's art
 * window at the field/hand tiers, the chip's full-bleed digest, ADR 0024
 * full-card mode, the inspect tier, and the inspect panel) renders this
 * component; nothing else emits an art `<img>` and nothing else sizes one.
 *
 * Why a primitive rather than per-surface CSS: containment has to be
 * *structural*. A replaced element that is not given both dimensions falls back
 * to the image's intrinsic size, so any surface that forgets one declaration
 * lets a 4096 px file decide a card's footprint. Concentrating the sizing in one
 * component + one stylesheet (`card-art.module.css`, which carries the
 * contract in full) means a new art surface cannot opt out of it: the mode is
 * the only knob, and every mode fixes a box.
 *
 * The box is therefore identical with no image, a loading image, a
 * synchronously-cached image, a late image, a failed image, a 4096 px image, a
 * 1 px image, and either extreme ratio — the card's footprint comes from its
 * tier alone (`theme.ts`'s `faceFootprint`). One `<img>`, so the ≤ 12-node
 * battlefield face budget (presentation-budgets §Performance) is unchanged.
 *
 * No game logic and no fetching: the consumer resolves an already-published URL
 * from the art store (ADR 0024) and hands it over.
 */
import { cx } from '../../chrome/cx';
import { cardArtVars } from './theme';
import a from './card-art.module.css';

/**
 * How one image is contained:
 * - `window`    — the frame's art mask, sized from the frame box (field/hand).
 * - `full`      — a whole-card image filling the frame (ADR 0024 full-card).
 * - `panel`     — a screen-space art window at the declared panel ratio.
 * - `panelFull` — a whole-card image shown whole at the declared card ratio.
 */
export type CardArtMode = 'window' | 'full' | 'panel' | 'panelFull';

/** Props for {@link CardArt}. */
export interface CardArtProps {
  /** Object/asset URL of an already-published image (never fetched here). */
  url: string;
  /** Which containment mode the surface needs. */
  mode: CardArtMode;
  /** Optional decoration (border/radius) from the consuming surface's chrome.
   * Sizing is NOT a consumer concern — the mode owns the whole box. */
  className?: string;
  /** Optional test hook for the consuming surface. */
  testId?: string;
}

/** Mode → containment class. */
const MODE_CLASS: Record<CardArtMode, string> = {
  window: a.window,
  full: a.full,
  panel: a.panel,
  panelFull: a.panelFull,
};

/**
 * One contained card illustration. `data-art-mode` is the stable hook for
 * consumers and tests — the class names are CSS-module-local.
 */
export function CardArt({ url, mode, className, testId }: CardArtProps) {
  return (
    <img
      className={cx(a.art, MODE_CLASS[mode], className)}
      style={cardArtVars()}
      src={url}
      alt=""
      aria-hidden="true"
      draggable={false}
      decoding="async"
      data-art-mode={mode}
      data-testid={testId}
    />
  );
}
