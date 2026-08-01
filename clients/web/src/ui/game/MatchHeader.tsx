/**
 * The band across the top: whose turn it is, what step it is, and who may act.
 *
 * Turn context is the one thing that must never require scrolling to find, because every other
 * question on the table is asked relative to it. It is also where a rejected submission and a
 * finished game are announced — both are statements about the match as a whole rather than
 * about anything on the table, and both have to be seen without hunting for them.
 */
import type { GameView } from './../../protocol'
import { phaseLabel } from './../../table'

export function MatchHeader({ view, label }: { view: GameView; label(id: string): string }) {
  return (
    <header className="match" aria-label="Match">
      <h1 className="match__turn">
        Turn {view.turn ?? 0} — {phaseLabel(view.phase)}
      </h1>
      <p className="match__state">
        Active: {label(view.active_player ?? '')}
        {view.priority_player !== undefined && <> · Priority: {label(view.priority_player)}</>}
        {view.action_deadline !== undefined && <> · {view.action_deadline}s to decide</>}
        {view.format?.commander && <> · Commander</>}
      </p>

      {view.action_rejected && (
        <p role="status" className="notice match__notice">
          That action could not be taken. This is the current state.
        </p>
      )}

      {view.result && (
        <section className="notice match__notice" aria-labelledby="result-heading">
          <h2 id="result-heading">Game over</h2>
          <p>
            {view.result.winner ? `${label(view.result.winner)} wins` : 'No winner'} ·{' '}
            {view.result.reason}
          </p>
        </section>
      )}
    </header>
  )
}
