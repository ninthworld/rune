/**
 * The loyalty symbol a planeswalker's ability is activated for.
 *
 * A card prints the cost inside a shape rather than as text, and the shape is what a player reads
 * first: pointing up to add loyalty, down to spend it, flat for the ability that costs nothing.
 * The number inside is the server's, unchanged (`card-face.ts`, `loyaltyCost`).
 *
 * Drawn in its own 100×100 grid like a pip, and sized off the text around it, so it grows and
 * shrinks with rules text that has already been fitted to its box.
 */

/** Pointing up, down, or neither — the three shapes a printed loyalty cost comes in. */
const UP = 'M 50 2 L 96 32 V 92 A 6 6 0 0 1 90 98 H 10 A 6 6 0 0 1 4 92 V 32 Z'
const DOWN = 'M 10 2 H 90 A 6 6 0 0 1 96 8 V 68 L 50 98 L 4 68 V 8 A 6 6 0 0 1 10 2 Z'
const FLAT = 'M 50 2 L 96 22 V 78 L 50 98 L 4 78 V 22 Z'

export function Loyalty({ cost }: { cost: string }) {
  const shape = cost.startsWith('+') ? UP : cost.startsWith('−') ? DOWN : FLAT
  // Two glyphs fill the shape; a third has to give way rather than run over the edge.
  const size = cost.length > 2 ? 40 : 52

  return (
    <svg
      className="c-loyalty"
      viewBox="0 0 100 100"
      role="img"
      aria-label={`${cost} loyalty`}
      aria-hidden={false}
    >
      <path d={shape} fill="#17150f" />
      <path d={shape} fill="none" stroke="rgba(255, 255, 255, 0.22)" strokeWidth="4" />
      <text
        x="50"
        y="50"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize={size}
        fontWeight="700"
        fill="#f6f4ee"
      >
        {cost}
      </text>
    </svg>
  )
}
