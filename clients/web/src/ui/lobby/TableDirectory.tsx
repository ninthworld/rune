/**
 * The table directory: every browsable room, as rows you can act on.
 *
 * A row's button is there because the server advertised the command it sends (`lobby.ts`), so
 * this draws what it was handed and gates nothing itself. The room id stays visible on every
 * row: a private table is reachable only by an id its host shares out of band, and the box at
 * the bottom is where that id goes.
 */
import { useState } from 'react'

import type { TableEntry } from './../../lobby'

export function TableDirectory({
  entries,
  joinById,
  onReach,
  onJoinById,
}: {
  entries: readonly TableEntry[]
  /** Whether the server is offering `join_room` at all; the id box is pointless without it. */
  joinById: boolean
  onReach(entry: TableEntry): void
  onJoinById(roomId: string): void
}) {
  const [id, setId] = useState('')

  return (
    <section aria-labelledby="directory-heading" className="directory">
      <h2 id="directory-heading">Tables</h2>

      {entries.length === 0 ? (
        <p className="directory__empty">No public tables right now. Create one.</p>
      ) : (
        <ul className="directory__list">
          {entries.map((entry) => (
            <li key={entry.roomId} className={`table-row table-row--${entry.state}`}>
              <span className="table-row__label">{entry.label}</span>
              <span className="table-row__format">{entry.format}</span>
              <span className="table-row__seats">
                {entry.filled}/{entry.seats} seats
                {entry.open > 0 && <> · {entry.open} open</>}
              </span>
              <span className="table-row__state">{entry.stateLabel}</span>
              <span className="table-row__watchers">
                {entry.spectators > 0 && <>{entry.spectators} watching</>}
              </span>
              <span className="table-row__id" title="Table id">
                <code>{entry.roomId}</code>
              </span>
              <span className="table-row__reach">
                {entry.reach && (
                  <button type="button" onClick={() => onReach(entry)}>
                    {entry.reach === 'join_room' ? 'Join' : 'Watch'}
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}

      {joinById && (
        <p className="directory__by-id">
          <label>
            Have a table id?{' '}
            <input
              aria-label="Table id"
              placeholder="r_204"
              value={id}
              onChange={(event) => setId(event.target.value)}
            />
          </label>{' '}
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
