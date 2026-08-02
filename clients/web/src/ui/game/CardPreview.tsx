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
 * right-click that pins a card in place. Neither costs a turn.
 *
 * It sits over the side column, never over the table: a panel that covered the board while a
 * player swept their pointer across it would hide the very thing they were reading. And it is
 * inert — `pointer-events: none` — because a preview the pointer can land on is a preview that
 * flickers as it steals the hover from the card that raised it.
 *
 * **Pinned, it stops being either.** A card the player parked here is the one thing on the screen
 * they asked to keep looking at, so it takes part in the column's layout rather than floating over
 * the log, it accepts the pointer so the release is a click and not a hunt for the key, and it is
 * announced — a hover preview is noise to a screen reader that has just read the same card off the
 * control the focus is on, and a deliberate pin is not.
 */
import type { CardFace } from './../../card-face'
import { Card } from './../Card'

export function CardPreview({ face, onUnpin }: { face: CardFace; onUnpin?(): void }) {
  if (onUnpin) {
    return (
      <div className="preview preview--pinned" aria-label={`${face.name} — pinned`}>
        <Card face={face} />
        {/* Named for what it does to the thing the player is looking at, and repeated in the
            gesture that made it: clicking the card again releases it too. */}
        <button type="button" className="preview__unpin" onClick={onUnpin}>
          Unpin
        </button>
      </div>
    )
  }
  return (
    // Hidden from assistive technology on purpose: it is raised by focus as well as by hover,
    // so a keyboard user has already been read this very card by the control they are on, and
    // announcing it twice is noise rather than help.
    <div className="preview" aria-hidden="true">
      <Card face={face} />
    </div>
  )
}
