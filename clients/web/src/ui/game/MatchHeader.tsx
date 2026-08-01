/**
 * The band across the top: whose turn it is, what step it is, who may act, and what the match is
 * waiting for.
 *
 * Turn context is the one thing that must never require scrolling to find, because every other
 * question on the table is asked relative to it. It is also where anything true of the *match*
 * rather than of anything on the table is announced — a rejected submission, a lost connection,
 * an elimination, a finished game — since none of those belong to an object a player could look
 * at, and all of them have to be seen without hunting.
 *
 * The status line names who the game is waiting for rather than guessing why. "Waiting for Alice"
 * is a fact the view stated; "Alice is choosing blockers" would be an invention about a seat
 * whose actions this client cannot see.
 */
import { useEffect, useState } from 'react'

import type { GameView, Phase } from './../../protocol'
import { matchStatus, phaseLabel, statusLine, steps, type StopScope } from './../../turn'
import type { ConnectionStatus } from './../../socket'
import { TurnStrip } from './TurnStrip'

export function MatchHeader({
  view,
  label,
  sent,
  eliminated,
  connection,
  onStop,
}: {
  view: GameView
  label(id: string): string
  /** The label of a submission still in flight, if any. */
  sent?: string
  /** Whether the seat this client is playing is out of the game (CR 104.3). */
  eliminated: boolean
  connection: ConnectionStatus
  onStop(phase: Phase, scope: StopScope): void
}) {
  const status = matchStatus(view, sent)
  const remaining = useDeadline(view)
  const active = view.active_player
  const priority = view.priority_player

  return (
    <header className="match" aria-label="Match">
      <div className="match__line">
        <h1 className="match__turn">
          Turn {view.turn ?? 0} — {phaseLabel(view.phase)}
        </h1>
        <p className="match__state">
          {/* Two seats, two different facts: whose turn it is, and who may act inside it. They
              are usually the same player and occasionally not, which is exactly when a player
              needs to be able to tell them apart. */}
          <span className="match__who">
            Turn: <strong>{label(active ?? '')}</strong>
          </span>
          {priority !== undefined && (
            <span className="match__who">
              Priority: <strong>{label(priority)}</strong>
            </span>
          )}
          {view.format?.commander && <span className="match__who">Commander</span>}
        </p>
        <p className={`match__status match__status--${status.kind}`} role="status">
          {statusLine(status, label)}
          {remaining !== undefined && (
            <span className="match__clock">
              {remaining <= 0 ? ' · out of time' : ` · ${Math.ceil(remaining)}s to decide`}
            </span>
          )}
        </p>
      </div>

      <TurnStrip steps={steps(view)} onStop={onStop} />

      {connection !== 'open' && (
        <p role="status" className="notice match__notice">
          Connection lost — reconnecting. Your seat is held; this is the last state the server sent.
        </p>
      )}

      {view.action_rejected && (
        <p role="status" className="notice match__notice">
          That action could not be taken. This is the current state.
        </p>
      )}

      {eliminated && !view.result && (
        <p role="status" className="notice match__notice">
          You are out of this game. It continues without you.
        </p>
      )}

      {/* One line, not a panel: the result *panel* is `MatchResult`, over the board, and two
          regions announcing the same thing is one region too many. This is what remains once
          that has been pushed aside, so a finished game never looks live. */}
      {view.result && (
        <p role="status" className="notice match__notice">
          Game over — {view.result.winner ? `${label(view.result.winner)} wins` : 'no winner'} ·{' '}
          {view.result.reason}
        </p>
      )}
    </header>
  )
}

/**
 * The decision clock, ticking.
 *
 * `action_deadline` is seconds remaining at the moment the view was built, computed from an
 * absolute server deadline (`docs/protocol.md`). Redrawing that number unchanged until the next
 * frame would make a clock that lies for as long as nothing happens — which is precisely the
 * stretch a clock exists for. So it counts down locally from the arrival of the view that carried
 * it, and every new view resets it to the server's number: the display can drift by a frame's
 * latency, never past it, and it enforces nothing. Expiry is the server's to act on.
 */
function useDeadline(view: GameView): number | undefined {
  const deadline = view.action_deadline
  const [remaining, setRemaining] = useState(deadline)

  // Reset during render rather than in an effect, so the frame that carries a new view never
  // paints the previous view's count — the same rule the game screen settles its draft by.
  const [seen, setSeen] = useState(view)
  if (seen !== view) {
    setSeen(view)
    setRemaining(deadline)
  }

  useEffect(() => {
    if (deadline === undefined) return
    const from = Date.now()
    const timer = setInterval(
      () => setRemaining(Math.max(0, deadline - (Date.now() - from) / 1000)),
      250,
    )
    return () => clearInterval(timer)
    // Keyed on the view, not the number: two consecutive views can carry the same remaining
    // seconds, and the clock has to restart from each of them.
  }, [view, deadline])

  return remaining
}
