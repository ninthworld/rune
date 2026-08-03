/**
 * The end of the match, and the way out of it.
 *
 * A finished game is the one moment the board stops being the point, so this is the one panel
 * that layers over it — in the glass every dialog here is made of (§5.5). It can be dismissed,
 * because the final board is worth reading and a panel that cannot be closed hides the thing the
 * result is about.
 *
 * **Leaving is a new session, not a message.** An in-game socket speaks the game contract for the
 * life of that game and has no "leave" to send, so returning to the lobby means forgetting the
 * token that holds the seat and connecting as somebody new. Saying so plainly is better than a
 * button that appears to do something lighter than it does.
 */
import type { GameResult } from './../../protocol'

const REASONS: Record<string, string> = {
  life_zero: 'life reached zero',
  decked: 'a draw from an empty library',
  concede: 'a concession',
  commander_damage: '21 commander damage',
}

export function MatchResult({
  result,
  label,
  you,
  onLeave,
  onDismiss,
}: {
  result: GameResult
  label(id: string): string
  /** This client's own seat, so the outcome can be stated in the second person. */
  you?: string
  onLeave(): void
  onDismiss(): void
}) {
  const winner = result.winner
  const outcome =
    winner === undefined
      ? 'The game ended with no winner.'
      : you !== undefined && winner === you
        ? 'You win.'
        : `${label(winner)} wins.`

  return (
    <div className="zone-view" onClick={onDismiss}>
      <div
        className="zone-panel result-panel"
        role="dialog"
        aria-label="Game over"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">Game over</span>
          <span className="zone-tally" />
          <button className="zone-close" onClick={onDismiss} title="Look at the board">
            ✕
          </button>
        </div>
        <div className="result-body">
          <p className="result-outcome">{outcome}</p>
          {/* An unrecognized reason is shown as the server sent it rather than dropped: a newer
              server may end a game in a way this build has no sentence for. */}
          <p className="result-reason">By {REASONS[result.reason] ?? result.reason}.</p>
          {result.losers !== undefined && result.losers.length > 0 && (
            <p className="result-losers">Out: {result.losers.map(label).join(', ')}</p>
          )}
        </div>
        <div className="zone-foot">
          <span className="zone-hint">Leaving ends this session and starts a new one.</span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onDismiss}>
              Look at the board
            </button>
            <button className="action-done" onClick={onLeave}>
              Back to the lobby
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
