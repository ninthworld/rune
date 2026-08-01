/**
 * The art window's own picture, when there is no picture.
 *
 * Every card frame reserves an art window (ADR 0012). Left empty it is a grey rectangle repeated
 * forty times across a board, which is worse than useless — a player scanning their own
 * battlefield gets no help at all from the largest region of every card. So the window draws
 * something: a small, deterministic composition seeded from the card's own identity and washed in
 * the frame's tint, which gives every distinct card a distinct silhouette at a glance while
 * claiming nothing about what the card depicts.
 *
 * Three rules make this safe to keep forever:
 *
 * - **It is generated, never fetched and never bundled.** Nothing here downloads, nothing here
 *   ships an image, and the output is a handful of numbers the stylesheet turns into gradients.
 *   The project's distribution posture is unchanged.
 * - **It is a function of the seed alone.** The same card looks the same in the hand, on the
 *   board, in the stack, and in the deck builder, in this session and the next one, because there
 *   is no state and no randomness — a card a player learns to recognise stays recognisable.
 * - **It is the floor, not the ceiling.** When a player turns on the art source of ADR 0012, a
 *   real illustration covers this. It is what the window falls back to per card, which is why an
 *   unavailable image can never block play.
 *
 * The seed is the `functional_id` where there is one and the name otherwise, so a token — which
 * has no card identity at all (CR 111) — still gets a stable face rather than sharing one with
 * every other token on the table.
 */
import type { CSSProperties } from 'react'

/**
 * FNV-1a, 32-bit.
 *
 * Any stable string hash would do. What matters is that it is *stable*: written out here rather
 * than taken from a library so the same card cannot start looking different because a dependency
 * changed its mind about how to mix bits.
 */
function hash(seed: string): number {
  let value = 0x811c9dc5
  for (let index = 0; index < seed.length; index += 1) {
    value ^= seed.charCodeAt(index)
    value = Math.imul(value, 0x01000193)
  }
  return value >>> 0
}

/** Successive independent-enough numbers from one hash, each in `[0, 1)`. */
function* stream(seed: string): Generator<number> {
  let value = hash(seed)
  for (;;) {
    // xorshift32: cheap, deterministic, and good enough to place four blobs.
    value ^= value << 13
    value ^= value >>> 17
    value ^= value << 5
    value >>>= 0
    yield value / 0x100000000
  }
}

/**
 * The composition, as the custom properties `cards.css` draws it from.
 *
 * Positions and angles only. Every colour stays in the stylesheet, resolved from the frame's
 * tint, so the palette has one home and a card in a hand and the same card on the board cannot
 * be washed differently.
 */
export function proceduralArt(seed: string): CSSProperties {
  const next = stream(seed)
  const percent = (low: number, high: number) =>
    `${Math.round(low + next.next().value * (high - low))}%`

  return {
    '--art-x1': percent(15, 60),
    '--art-y1': percent(10, 45),
    '--art-r1': percent(45, 85),
    '--art-x2': percent(45, 90),
    '--art-y2': percent(50, 90),
    '--art-r2': percent(35, 70),
    '--art-angle': `${Math.round(next.next().value * 360)}deg`,
    // A band across the window, which is what stops two cards with similar blobs reading as the
    // same picture. Kept wide and low-contrast so it never competes with the name above it.
    '--art-band': percent(30, 70),
  } as CSSProperties
}
