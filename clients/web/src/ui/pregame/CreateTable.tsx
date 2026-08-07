/**
 * Making a table, as a dialog over the list (`docs/client-design.md` §9.4).
 *
 * One form in the glass of §5.5: name, format, seats, access, and undo. Its footer restates what
 * will be made in one line, so the summary is read where the commit is, and the create button is
 * the only raised thing in it.
 *
 * Every choice comes from the `CatalogView` — the formats a table can be made with, and the seat
 * range each of those formats allows — which is why nothing here hardcodes a `game_setup` id or
 * a seat count. A client that has not been handed a catalog offers no format rather than
 * guessing at one.
 *
 * The same form serves editing a table, because `create_room` and `update_room` both carry a
 * whole `RoomConfig`.
 *
 * **Undo is a table rule the wire now carries** (issue #648): it is chosen here, reaches
 * `RoomConfig.undo_enabled`, and is shown to everyone at the table (§9.5). `Not allowed` is the
 * selected answer for a new table, because a table takes nothing back unless somebody asked it to
 * — and the note under it says what the choice costs rather than only what it does, since a
 * rollback restores hidden zones but nobody unlearns a card they were shown.
 */
import { useState } from 'react'

import type { Catalog } from './../../deck'
import type { RoomConfig } from './../../protocol'

export function CreateTable({
  catalog,
  initial,
  title,
  submit,
  onClose,
  onSubmit,
}: {
  catalog: Catalog
  /** The table being edited, or nothing when one is being made. */
  initial?: RoomConfig
  title: string
  submit: string
  onClose(): void
  onSubmit(config: RoomConfig): void
}) {
  const formats = catalog.formats
  const [name, setName] = useState(initial?.name ?? '')
  const [setup, setSetup] = useState(initial?.game_setup ?? formats[0]?.game_setup ?? '')
  const format = formats.find((entry) => entry.game_setup === setup)
  const [seats, setSeats] = useState(initial?.seats ?? format?.min_seats ?? 2)
  const [open, setOpen] = useState((initial?.visibility ?? 'public') === 'public')
  const [undo, setUndo] = useState(initial?.undo_enabled ?? false)

  // The bounds are the format's own, restated by the stepper rather than decided here.
  const min = format?.min_seats ?? seats
  const max = format?.max_seats ?? seats
  const bounded = Math.min(max, Math.max(min, seats))

  return (
    <div className="zone-view" onClick={onClose}>
      <form
        className="zone-panel new-table"
        role="dialog"
        aria-label={title}
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault()
          onSubmit({
            seats: bounded,
            game_setup: setup,
            ...(name.trim() === '' ? {} : { name: name.trim() }),
            ...(open ? {} : { visibility: 'private' }),
            ...(undo ? { undo_enabled: true } : {}),
          })
        }}
      >
        <div className="zone-head">
          <span className="zone-title">{title}</span>
          <span className="zone-tally" />
          <button type="button" className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>
        <div className="new-body">
          <label className="connect-field">
            <span className="connect-label">Name</span>
            <input
              className="connect-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="What to call it"
              maxLength={32}
              autoFocus
            />
          </label>

          <div className="connect-field">
            <span className="connect-label" id="new-format">
              Format
            </span>
            <span className="seg new-seg" role="radiogroup" aria-labelledby="new-format">
              {formats.map((entry) => (
                <button
                  key={entry.game_setup}
                  type="button"
                  role="radio"
                  aria-checked={setup === entry.game_setup}
                  className={`seg-btn${setup === entry.game_setup ? ' seg-on' : ''}`}
                  onClick={() => {
                    setSetup(entry.game_setup)
                    setSeats((current) =>
                      Math.min(entry.max_seats, Math.max(entry.min_seats, current)),
                    )
                  }}
                >
                  {entry.game_setup}
                </button>
              ))}
              {formats.length === 0 && <span className="new-note">No format yet.</span>}
            </span>
          </div>

          <div className="connect-field">
            <span className="connect-label">Seats</span>
            <span className="seat-step new-step">
              <button
                type="button"
                className="step-btn"
                title="Fewer seats"
                disabled={bounded <= min}
                onClick={() => setSeats((n) => Math.max(min, n - 1))}
              >
                −
              </button>
              <span className="seat-num">{bounded} players</span>
              <button
                type="button"
                className="step-btn"
                title="More seats"
                disabled={bounded >= max}
                onClick={() => setSeats((n) => Math.min(max, n + 1))}
              >
                +
              </button>
            </span>
          </div>

          <div className="connect-field">
            <span className="connect-label" id="new-access">
              Access
            </span>
            <span className="seg new-seg" role="radiogroup" aria-labelledby="new-access">
              <button
                type="button"
                role="radio"
                aria-checked={open}
                className={`seg-btn${open ? ' seg-on' : ''}`}
                onClick={() => setOpen(true)}
              >
                Open
              </button>
              <button
                type="button"
                role="radio"
                aria-checked={!open}
                className={`seg-btn${open ? '' : ' seg-on'}`}
                onClick={() => setOpen(false)}
              >
                Invite only
              </button>
            </span>
          </div>

          <div className="connect-field undo-field">
            <span className="connect-label" id="new-undo">
              Undo
            </span>
            <span className="seg new-seg" role="radiogroup" aria-labelledby="new-undo">
              <button
                type="button"
                role="radio"
                aria-checked={undo}
                className={`seg-btn${undo ? ' seg-on' : ''}`}
                onClick={() => setUndo(true)}
              >
                Allowed
              </button>
              <button
                type="button"
                role="radio"
                aria-checked={!undo}
                className={`seg-btn${undo ? '' : ' seg-on'}`}
                onClick={() => setUndo(false)}
              >
                Not allowed
              </button>
            </span>
            <span className="new-note">
              Any player may take the last action back, without asking the others. For casual play,
              testing, and fixing a misclick — a rollback puts the cards back, but nobody unsees one
              they were shown.
            </span>
          </div>
        </div>
        <div className="zone-foot">
          <span className="zone-hint">
            {setup || 'no format'} · {bounded} seats · {open ? 'anyone may join' : 'invite only'} ·{' '}
            {undo ? 'undo allowed' : 'no undo'}
          </span>
          <div className="zone-acts">
            <button className="action-done" disabled={setup === ''}>
              {submit}
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
