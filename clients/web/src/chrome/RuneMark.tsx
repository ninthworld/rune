/**
 * The RUNE brand mark — the product's procedural identity motif (issue #300).
 *
 * Per the hard rules (AGENTS.md) and `docs/design/ui-design-notes.md` (§Identity),
 * RUNE's look is **procedural geometry only**: no card images, official frames,
 * symbols, or WotC branding anywhere. This is the pre-game analogue of the card
 * renderer's monogram/frame geometry (`card/cardFactory.ts`) — an angular rune
 * glyph carved inside a hexagonal ring, drawn as pure inline-SVG vector strokes so
 * it needs no bundled asset and inherits its color from the surrounding chrome
 * (`color` → `currentColor`, which the caller points at a chrome accent token).
 *
 * It is decorative: the visible RUNE wordmark carries the accessible name, so the
 * mark is `aria-hidden` and adds nothing for assistive tech to announce twice.
 */

/** Props for {@link RuneMark}: the rendered edge length in CSS pixels. */
export interface RuneMarkProps {
  /** Square edge length in px. Defaults to 40. */
  readonly size?: number;
  /** Optional class applied to the `<svg>` (e.g. to set its `color`). */
  readonly className?: string;
}

/**
 * The procedural RUNE mark: a hexagonal ring enclosing an angular rune glyph. All
 * geometry is expressed in a fixed `0 0 48 48` viewBox and scaled by `size`;
 * strokes use `currentColor` so the mark tints to whatever chrome color the caller
 * sets. Nothing here is a game or card asset — it is brand geometry, drawn live.
 */
export function RuneMark({ size = 40, className }: RuneMarkProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 48 48"
      fill="none"
      aria-hidden="true"
      focusable="false"
    >
      {/* Hexagonal ring — the carved-token frame, drawn faint. */}
      <polygon
        points="24,3 42.19,13.5 42.19,34.5 24,45 5.81,34.5 5.81,13.5"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinejoin="round"
        opacity={0.5}
      />
      {/* The rune glyph: a stem, an upper bowl, and an angled leg. */}
      <path
        d="M19 13 L19 35 M19 13 L30 17 L19 22 M19 22 L30 35"
        stroke="currentColor"
        strokeWidth={2.6}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
