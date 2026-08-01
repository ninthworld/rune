/**
 * The column beside the table: what just happened, and what is being shown to you.
 *
 * Everything here answers a question about the recent past rather than the current board, which
 * is why it is off to the side instead of on the table. It scrolls on its own, so a long log
 * never changes the geometry of the game.
 *
 * The settle list is the point of the exercise, not decoration. The server advances the game on
 * your behalf through steps where you had nothing to do, so one message can cover a whole turn;
 * without saying where it acted, a player watches the game move and cannot tell what they
 * missed. It sits above the log because it is about the message that just arrived.
 */
import type { GameLogEntry, AutoPassedStep } from './../../protocol'
import type { CardFace } from './../../card-face'
import { describe } from './../../game-log'
import { phaseLabel } from './../../table'
import { Card } from './../Card'
import type { Surface } from './surface'

export function SidePanel({
  revealed,
  settled,
  log,
  label,
  surface,
}: {
  revealed: readonly CardFace[]
  settled: readonly AutoPassedStep[]
  log: readonly GameLogEntry[]
  label(id: string): string
  surface: Surface
}) {
  return (
    <aside className="side">
      {revealed.length > 0 && (
        <section className="side__block" aria-labelledby="revealed-heading">
          <h2 id="revealed-heading">Shown to you</h2>
          {/* Only this seat receives these; the server decides that, and sends them to nobody
              else. Kept beside the dock so the choice prompt below has something legible to
              refer to. */}
          <ul className="cards cards--compact">
            {revealed.map((face) => (
              <li key={face.id}>
                <Card
                  face={face}
                  variant="compact"
                  state={surface.stateOf(face)}
                  onInspect={surface.inspect}
                />
              </li>
            ))}
          </ul>
        </section>
      )}

      {settled.length > 0 && (
        <section className="side__block notice" aria-labelledby="settle-heading">
          <h2 id="settle-heading">Passed for you</h2>
          {/* A path, not a set: a genuinely revisited position appears twice, and each entry
              carries its own turn because an extra combat or cleanup phase revisits a step
              within one turn. Collapsing either would assert game structure the server did
              not state. */}
          <ol>
            {settled.map((step, index) => (
              <li key={`${step.turn}-${step.phase}-${index}`}>
                Turn {step.turn} — {phaseLabel(step.phase)}
              </li>
            ))}
          </ol>
        </section>
      )}

      <section className="side__block side__log" aria-labelledby="log-heading">
        <h2 id="log-heading">Log</h2>
        {log.length === 0 ? (
          <p>Nothing yet.</p>
        ) : (
          <ol className="log">
            {/* Newest last, as the server ordered it; the window is bounded server-side. */}
            {log.map((entry) => (
              <li key={entry.sequence}>{describe(entry.event, label)}</li>
            ))}
          </ol>
        )}
      </section>
    </aside>
  )
}
