/**
 * The bars the frame is built out of: the title, the type line, and the stat plaque.
 *
 * Shared because the title bar is drawn in more than one place — on the card, over a full-art
 * face, and on its own in the deck builder's title list — and a second construction of the same
 * bar is how those drift apart.
 */

/**
 * A bar is a sharp-edged rectangle whose short ends are circular arcs bulging outward by `b`;
 * the arc meets the straight edges at hard corners — this is not a `border-radius`.
 */
export function barPath(w: number, h: number, b: number): string {
  const x0 = b
  const x1 = w - b
  const chord = h - 1
  const r = (chord * chord) / (8 * b) + b / 2
  return (
    `M ${x0} 0.5 L ${x1} 0.5 A ${r} ${r} 0 0 1 ${x1} ${h - 0.5} ` +
    `L ${x0} ${h - 0.5} A ${r} ${r} 0 0 1 ${x0} 0.5 Z`
  )
}

/** The title bar, and where it sits in the card's 207×291 grid. */
export const TITLE_W = 183
export const TITLE_H = 16.5
export const TITLE_X = 12
export const TITLE_Y = 9.5
export const TITLE = barPath(TITLE_W, TITLE_H, 2.5)
