/**
 * The art window of one card frame.
 *
 * Reserved by ADR 0012 and never empty: it draws the procedural composition seeded from the
 * card's own identity, and a player-supplied illustration covers that when one has resolved. The
 * window keeps its box either way, so nothing below it moves when an image arrives, fails, or is
 * switched off — and the procedural layer stays *underneath* rather than being replaced, so a
 * slow or broken image never leaves a hole.
 *
 * Decorative to assistive technology by design. Everything the picture could say — the name, the
 * cost, the type, the stat — is text elsewhere in the same frame, so an `alt` here would repeat
 * what a screen reader has already read.
 */
import type { CardFace } from './../card-face'
import { proceduralArt } from './../art/procedural'

/**
 * What the composition is seeded from.
 *
 * `artKey` is the card's stable identity across games and builds, so the same card looks the same
 * everywhere and in every session. A token has none (CR 111), and falls back to its name — which
 * is stable enough for the same reason it is the only identity a token has.
 */
const seedOf = (face: CardFace): string => face.artKey ?? face.name

export function CardArt({ face, url }: { face: CardFace; url?: string }) {
  return (
    <span className="card__art" aria-hidden="true" style={proceduralArt(seedOf(face))}>
      {url && (
        // Lazy because a hand, two battlefields, and an open graveyard are a lot of images at
        // once, and none of them is worth delaying a click for. `decoding="async"` for the same
        // reason: the board paints, and the pictures land when they land.
        <img className="card__illustration" src={url} alt="" loading="lazy" decoding="async" />
      )}
    </span>
  )
}
