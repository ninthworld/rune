/**
 * The tables list: the screen Play lands on.
 *
 * `docs/client-design.md` §9.5 states the row in one line — **what the table is, how full it is,
 * and one button** — and everything that used to ride along is gone. The room id is the clearest
 * case: it was printed on every row, and a player has no use for an identifier they never type.
 * It has exactly one use, reaching an unlisted table somebody shared out of band, and that use
 * has a box at the bottom of this list.
 *
 * How full a table is is drawn rather than counted out: one pip per seat, filled or open. A
 * player scanning for somewhere to sit is asking "is there room", and a row of pips answers that
 * without being read. The count is still the accessible name, because a pip is not a fact
 * assistive technology can perceive.
 *
 * A row's button is there because the server advertised the command it sends (`lobby.ts`), and
 * occupancy only chooses which of the two advertised commands it leads with. This gates nothing
 * itself.
 */
import { useState } from 'react'

import type { TableEntry } from './../../lobby'

export function TableDirectory({
  entries,
  joinById,
  canCreate,
  onCreate,
  onReach,
  onJoinById,
}: {
  entries: readonly TableEntry[]
  /** Whether the server is offering `join_room` at all; the id box is pointless without it. */
  joinById: boolean
  canCreate: boolean
  onCreate(): void
  onReach(entry: TableEntry): void
  onJoinById(roomId: string): void
}) {
  const [id, setId] = useState('')

  return (
    <section aria-label="Tables" className="page">
      <header className="page__head">
        <h1>Tables</h1>
        {canCreate && (
          <button type="button" onClick={onCreate}>
            New table
          </button>
        )}
      </header>

      {entries.length === 0 ? (
        <p className="page__pending">No tables yet.</p>
      ) : (
        <ul className="tables">
          {entries.map((entry) => (
            <li key={entry.roomId} className={`table table--${entry.state}`}>
              <span className="table__main">
                <span className="table__name">{entry.label}</span>
                <span className="table__about">
                  <span className="table__format">{entry.format}</span>
                  {entry.state === 'in_progress' && (
                    <span className="table__state">In progress</span>
                  )}
                  {entry.spectators > 0 && (
                    <span className="table__watchers">{entry.spectators} watching</span>
                  )}
                </span>
              </span>
              <span
                className="table__seats"
                aria-label={`${entry.filled} of ${entry.seats} seats taken`}
              >
                {Array.from({ length: entry.seats }, (_, index) => (
                  <span
                    key={index}
                    aria-hidden="true"
                    className={`pip${index < entry.filled ? ' pip--taken' : ''}`}
                  />
                ))}
              </span>
              <span className="table__reach">
                {entry.reach && (
                  <button
                    type="button"
                    // Getting into a table that can still be joined is the one thing this
                    // screen is for. Watching a game already running is a real thing to do and
                    // not that thing, so it does not take the accent.
                    className={entry.reach === 'join_room' ? 'page__lead' : undefined}
                    onClick={() => onReach(entry)}
                  >
                    {entry.reach === 'join_room' ? 'Join' : 'Watch'}
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}

      {/* The one place a room id is worth a control: a private table is unlisted, and the id its
          host shared is the only way in. */}
      {joinById && (
        <p className="tables__by-id">
          <input
            aria-label="Table id"
            placeholder="Table id"
            value={id}
            onChange={(event) => setId(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && id.trim()) onJoinById(id.trim())
            }}
          />
          <button
            type="button"
            disabled={id.trim().length === 0}
            onClick={() => onJoinById(id.trim())}
          >
            Join by id
          </button>
        </p>
      )}
    </section>
  )
}
