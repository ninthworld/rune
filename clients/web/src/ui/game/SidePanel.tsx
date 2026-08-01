/**
 * The column beside the table: what you are looking through, and what just happened.
 *
 * Everything here answers a question the board is not currently asking — a pile the player
 * opened, cards a prompt is showing them, steps the server passed on their behalf, the log.
 * That is why it is off to the side instead of on the table, and why it scrolls on its own, so
 * neither a long log nor a full graveyard changes the geometry of the game.
 *
 * An opened pile lands here rather than over the table on purpose. A pile is usually the thing a
 * player is choosing *from* while the dock asks the question, so a modal would make the two
 * halves of one decision take turns; beside the table, the board, the pile, and the controls are
 * all readable at once. It sits at the top of the column because it is the thing that was just
 * asked for.
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
import type { Surface } from './surface'
import { ZonePanel } from './ZonePanel'

/** A pile the player opened, resolved out of the current view by `Game.tsx`. */
export interface OpenZone {
  label: string
  note: string
  faces: readonly CardFace[]
}

export function SidePanel({
  zone,
  closeZone,
  revealed,
  settled,
  log,
  label,
  surface,
}: {
  zone?: OpenZone
  closeZone(): void
  revealed: readonly CardFace[]
  settled: readonly AutoPassedStep[]
  log: readonly GameLogEntry[]
  label(id: string): string
  surface: Surface
}) {
  return (
    <aside className="side">
      {zone && (
        <ZonePanel
          label={zone.label}
          note={zone.note}
          faces={zone.faces}
          surface={surface}
          onClose={closeZone}
        />
      )}

      {revealed.length > 0 && (
        // Only this seat receives these; the server decides that, and sends them to nobody
        // else. The same browsing surface as a pile, because it is the same thing to a player:
        // an ordered set of faces to read and choose from, kept beside the dock so the prompt
        // below has something legible to refer to. It cannot be closed — it is not something
        // the player opened, and it goes away when the choice it belongs to does.
        <ZonePanel label="Shown to you" faces={revealed} surface={surface} />
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
