/**
 * What the settle did, said once, above the bar that asks the next question
 * (`docs/client-design.md` §6.9, issue #709).
 *
 * The band is the answer to *what happened while I was not being asked*. It sits between the
 * board and the action bar because that is the reading order of the moment: the board is the new
 * state, this is how it got there, and the bar is what to do about it.
 *
 * **It draws only what the current view describes** (`settle.ts`), so there is nothing to cancel
 * on a reconnect and nothing to interrupt when a newer view arrives — the next view either
 * describes its own settle or describes none, and this follows. It holds no timer: a band that
 * expired on a clock would be state the view could not reconstruct, and a player who looked away
 * would lose the only account of what happened.
 *
 * **It is words, not motion.** A player with reduced motion turned on reads exactly this, in
 * exactly this place. `Motion.tsx` is the other half and is independent by construction.
 *
 * One line, fixed height, never scrolling (§3): at most three events and a count of the rest,
 * which is the log's cue rather than a summary that tries to replace it.
 */
import type { Settle } from './../../settle'

export function SettleBand({ settle }: { settle?: Settle }) {
  if (!settle) return null
  return (
    <div
      className={`settle-band${settle.crossedTurn ? ' settle-crossed' : ''}`}
      role="status"
      aria-live="polite"
    >
      {/* Where the game went. Named by where it ended, because "where am I now" is the question
          a player actually has, with the length of the run behind it answering "how far". */}
      <span className="settle-path">{settle.path}</span>
      {settle.events.map((event) => (
        <span key={event.sequence} className={`settle-event log-${event.kind}`}>
          {event.text}
        </span>
      ))}
      {settle.more > 0 && (
        <span className="settle-more" title="The whole sequence is in the log">
          +{settle.more} more
        </span>
      )}
    </div>
  )
}
