/**
 * The end of the match, and the way out of it.
 *
 * A finished game is the one moment the board stops being the point, so this is the one panel
 * that layers over it — the same treatment the card inspector gets, and for the same reason: it
 * is what the player is looking at now. It can be dismissed, because the final board is worth
 * reading and a modal that cannot be closed hides the thing the result is about; the header keeps
 * saying the game is over once it is gone.
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
    <div className="inspector-backdrop" role="presentation" onClick={onDismiss}>
      <section
        className="inspector result"
        aria-labelledby="match-result-heading"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id="match-result-heading">Game over</h2>
        <p className="result__outcome">{outcome}</p>
        {/* An unrecognized reason is shown as the server sent it rather than dropped: a newer
            server may end a game in a way this build has no sentence for. */}
        <p className="result__reason">By {REASONS[result.reason] ?? result.reason}.</p>
        {result.losers !== undefined && result.losers.length > 0 && (
          <p className="result__losers">Out: {result.losers.map(label).join(', ')}</p>
        )}
        <p className="result__controls">
          <button type="button" onClick={onLeave}>
            Back to the lobby
          </button>{' '}
          <button type="button" onClick={onDismiss}>
            Look at the board
          </button>
        </p>
        <p className="result__note">Leaving ends this session and starts a new one.</p>
      </section>
    </div>
  )
}
