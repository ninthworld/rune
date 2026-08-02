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
import { describe, kindOf } from './../../game-log'
import { passedRuns } from './../../turn'
import { CardPreview } from './CardPreview'
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
  preview,
  onUnpin,
  revealed,
  settled,
  missed,
  log,
  label,
  onClose,
  surface,
}: {
  zone?: OpenZone
  closeZone(): void
  /** The face of whatever the player is currently looking at, if anything. */
  preview?: CardFace
  /**
   * Present when the player parked that face here rather than merely passing over it, and the
   * whole of the difference: a pinned card takes part in the column instead of floating over it.
   */
  onUnpin?(): void
  revealed: readonly CardFace[]
  settled: readonly AutoPassedStep[]
  /** What happened while the settle was acting for this seat (`turn.ts`). */
  missed: readonly GameLogEntry[]
  log: readonly GameLogEntry[]
  label(id: string): string
  /**
   * Present when this column is a drawer over the board rather than a column beside it (§3,
   * step 8) — and it is the whole of the difference. Nothing about what the column *holds*
   * changes with the room available; only whether it is standing open.
   */
  onClose?(): void
  surface: Surface
}) {
  return (
    <aside className={`side${onClose ? ' side--drawer' : ''}`} aria-label="History">
      {onClose && (
        <p className="side__dismiss">
          <button type="button" onClick={onClose}>
            Close
          </button>
        </p>
      )}

      {/* Over this column rather than in it: a preview that took part in the layout would make
          the log jump every time the pointer crossed a card, and one over the table would hide
          the board being read. A *pinned* card is the exception, and for the same reason — the
          column moving once, when the player asked it to, is not the jitter this is avoiding. */}
      {preview && <CardPreview face={preview} onUnpin={onUnpin} />}

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
          <h2 id="settle-heading">While you were passed</h2>

          {/* What happened, before where it happened. A player whose creature died during a
              settle does not need the step list — they need the spell that killed it, and
              until this existed the only way to get it was to read the whole log after the
              fact. The events are the server's own (`auto_passed_from`), filtered to the ones
              this seat was never shown; nothing here is inferred from the step path. */}
          {missed.length > 0 && (
            <ol className="log side__missed">
              {missed.map((entry) => (
                <li
                  key={entry.sequence}
                  className={`log__entry log__entry--${kindOf(entry.event)}`}
                >
                  {describe(entry.event, label)}
                </li>
              ))}
            </ol>
          )}

          <p className="side__note">
            The server had nothing to ask you here, so it acted for you. Set a stop on the turn
            strip to be asked there next time.
          </p>
          {/* A path, not a set: a genuinely revisited position appears twice, and each entry
              carries its own turn because an extra combat or cleanup phase revisits a step
              within one turn. Collapsing either would assert game structure the server did
              not state. */}
          {/* The ground it covered, under what happened on it. Still a path and not a set: a
              genuinely revisited position appears twice, because collapsing it would quietly
              shorten how far the game moved unasked. */}
          <ol className="side__path">
            {passedRuns(settled).map((run, index) => (
              <li key={`${run.turn}-${index}`}>
                Turn {run.turn} — {run.steps.map((step) => step.label).join(' → ')}
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
            {/* Newest last, as the server ordered it; the window is bounded server-side. The
                class is the entry's kind, so a step change divides the column into turns and a
                death or a result carries weight — reading, not meaning. */}
            {log.map((entry) => (
              <li key={entry.sequence} className={`log__entry log__entry--${kindOf(entry.event)}`}>
                {describe(entry.event, label)}
              </li>
            ))}
          </ol>
        )}
      </section>
    </aside>
  )
}
