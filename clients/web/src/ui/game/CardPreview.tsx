/**
 * The card under the pointer, at full size.
 *
 * The table's frames are small because a board has to fit on a screen, so every one of them
 * clamps something — the rules text on a permanent, the cost on the battlefield, the type line
 * on a compact row. This is where the clamped version is redeemed continuously: look at any
 * object and its whole face appears here, with no click spent and nothing dismissed afterwards.
 *
 * That is what makes a one-click action safe (`interaction.ts`). A click that used to select an
 * object so the dock could offer its single action now takes that action, and the reading the
 * click used to be worth has to come from somewhere — it comes from here, and from the
 * right-click that opens the inspector. Neither costs a turn.
 *
 * It sits over the side column, never over the table: a panel that covered the board while a
 * player swept their pointer across it would hide the very thing they were reading. And it is
 * inert — `pointer-events: none` — because a preview the pointer can land on is a preview that
 * flickers as it steals the hover from the card that raised it.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../Card'

export function CardPreview({ face }: { face: CardFace }) {
  return (
    // Hidden from assistive technology on purpose: it is raised by focus as well as by hover,
    // so a keyboard user has already been read this very card by the control they are on, and
    // announcing it twice is noise rather than help.
    <div className="preview" aria-hidden="true">
      <Card face={face} variant="inspect" />
    </div>
  )
}
