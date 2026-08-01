/**
 * The answer to a viewport below the floor: a sentence, in place of a broken board.
 *
 * §1 gives Unsupported exactly one commitment — "say so plainly, in place of a broken board" —
 * and *in place of* is the whole of it. A notice layered over a table that is still being drawn
 * underneath costs the same layout, leaves a player poking at something that half works, and
 * makes "unsupported" a decoration rather than a decision. So this is what the game screen
 * returns instead of the table: no battlefields, no hand, no dock, nothing to click at.
 *
 * It says the number, because "too small" without one is a dead end. The two ways out are the
 * two that exist — a larger window, or less zoom — and they are the same fix seen from either
 * side, since a browser at 200% does not scale the page, it halves the layout viewport.
 *
 * Leaving is still offered. A player who cannot play here can still stand up from the table, and
 * a screen with no way off it is the one thing worse than a screen that says no.
 */
export function TooSmall({
  width,
  height,
  onLeave,
}: {
  width: number
  height: number
  onLeave(): void
}) {
  return (
    <div className="screen screen--unsupported">
      <section className="unsupported" aria-label="Unsupported screen">
        <h1 className="unsupported__head">This window is too small to play in.</h1>
        <p className="unsupported__note">
          The table needs at least 320 by 480. This one is {width} by {height} — either make the
          window larger or zoom out, which are the same thing: a browser at 200% does not shrink the
          page, it halves the space the page is given.
        </p>
        <p>
          <button type="button" onClick={onLeave}>
            Leave the table
          </button>
        </p>
      </section>
    </div>
  )
}
