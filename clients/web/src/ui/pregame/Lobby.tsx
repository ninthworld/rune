/**
 * The lobby: the tables you can reach (`docs/client-design.md` §9.4).
 *
 * Land on the list. Above it, a header saying what the list is, how much of it you are seeing,
 * and the one button that makes a new one. Below that, the filters in a recess: a search, the
 * formats as a segmented control, and an open-seats toggle. The list between them is the only
 * region that scrolls, and the page itself never does.
 *
 * **A row is a grid and every row uses the same columns**, so name, format, occupancy and the
 * button line up down the whole list. Occupancy is drawn as seat pips and a count — a table's
 * fullness is read without reading a number, and the number is there for when you want it. A
 * full table is muted throughout and its button is not pressable; a private one carries a lock
 * beside its name.
 *
 * **Which command a row leads with is the server's answer**, taken from `lobby.ts` unchanged: a
 * button exists because `join_room` or `spectate_room` is currently advertised, and occupancy
 * only chooses which of the two. Nothing here works out that a table looks joinable.
 *
 * The filters are the client's own reading of rows it was sent — a search over the name and the
 * format, and a toggle over a stated count. Neither asks the server anything.
 */
import { useState } from 'react'

import type { Catalog } from './../../deck'
import { tables, type TableEntry } from './../../lobby'
import type { LobbyCommand, LobbyView } from './../../protocol'
import { SidePanel } from './SidePanel'

/** One mark per seat, filled for taken. */
function Seats({ taken, seats }: { taken: number; seats: number }) {
  return (
    <span className="seats">
      <span className="seat-dots" aria-hidden="true">
        {Array.from({ length: seats }, (_, i) => (
          <span key={i} className={`seat-dot${i < taken ? ' seat-taken' : ''}`} />
        ))}
      </span>
      <span className="seat-count">
        {taken}/{seats}
      </span>
    </span>
  )
}

const ALL = 'All'

export function Lobby({
  view,
  catalog,
  name,
  server,
  sideOpen,
  onSide,
  onSettings,
  onDisconnect,
  onCreate,
  send,
}: {
  view: LobbyView
  catalog: Catalog
  /** What this device connected as, and where. */
  name: string
  server: string
  sideOpen: boolean
  onSide(open: boolean): void
  onSettings(): void
  onDisconnect(): void
  onCreate(): void
  send(command: LobbyCommand): void
}) {
  const [query, setQuery] = useState('')
  const [format, setFormat] = useState(ALL)
  const [openOnly, setOpenOnly] = useState(false)

  const rows = tables(view)
  const formats = [ALL, ...catalog.formats.map((entry) => entry.game_setup)]
  const needle = query.trim().toLowerCase()
  const shown = rows.filter(
    (row) =>
      (format === ALL || row.format === format) &&
      (!openOnly || row.open > 0) &&
      (needle === '' ||
        row.label.toLowerCase().includes(needle) ||
        row.format.toLowerCase().includes(needle)),
  )

  const reach = (row: TableEntry) => {
    if (row.reach === 'join_room') send({ type: 'join_room', room_id: row.roomId })
    else if (row.reach === 'spectate_room') send({ type: 'spectate_room', room_id: row.roomId })
  }

  return (
    <div className={`lobby${sideOpen ? '' : ' side-hidden'}`}>
      <div className="topbar lobby-topbar">
        <button className="view-btn" onClick={onDisconnect}>
          ← Disconnect
        </button>
        <span className="topbar-fill" />
        <span className="lobby-who">
          <b>{name}</b>
          <span className="lobby-server">{server}</span>
        </span>
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Chat and players"
          aria-expanded={sideOpen}
          onClick={() => onSide(!sideOpen)}
        >
          ☰
        </button>
      </div>

      <div className="lobby-main">
        <div className="lobby-head">
          <h1 className="lobby-title">Open tables</h1>
          <span className="lobby-tally">
            {shown.length} of {rows.length}
          </span>
          <button className="action-done lobby-new" onClick={onCreate}>
            + Create table
          </button>
        </div>

        <div className="filter-strip">
          <input
            className="connect-input filter-search"
            aria-label="Search tables"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search tables or formats"
          />
          <span className="seg" role="radiogroup" aria-label="Format">
            {formats.map((entry) => (
              <button
                key={entry}
                role="radio"
                aria-checked={format === entry}
                className={`seg-btn${format === entry ? ' seg-on' : ''}`}
                onClick={() => setFormat(entry)}
              >
                {entry}
              </button>
            ))}
          </span>
          <button
            className={`view-btn${openOnly ? ' view-on' : ''}`}
            aria-pressed={openOnly}
            onClick={() => setOpenOnly((on) => !on)}
          >
            Open seats only
          </button>
        </div>

        <div className="table-list" role="region" aria-label="Tables">
          {shown.map((row) => {
            const full = row.open === 0
            return (
              <div key={row.roomId} className={`table-row${full ? ' table-full' : ''}`}>
                <span className={`table-dot${full ? ' dot-full' : ''}`} />
                <span className="table-id">
                  <span className="table-name">
                    {row.label}
                    {row.private && (
                      <span className="table-lock" title="Invite only">
                        🔒
                      </span>
                    )}
                  </span>
                  <span className="table-host">
                    {row.stateLabel}
                    {row.spectators > 0 && ` · ${row.spectators} watching`}
                  </span>
                </span>
                <span className="table-format">{row.format}</span>
                <Seats taken={row.filled} seats={row.seats} />
                <button
                  className="action-done table-join"
                  disabled={row.reach === undefined}
                  onClick={() => reach(row)}
                >
                  {row.reach === 'join_room'
                    ? 'Join'
                    : row.reach === 'spectate_room'
                      ? 'Watch'
                      : 'Full'}
                </button>
              </div>
            )
          })}
          {shown.length === 0 && (
            <div className="zone-empty">
              {rows.length === 0
                ? 'No table has been made yet. Create one.'
                : 'No table matches those filters.'}
            </div>
          )}
        </div>
      </div>

      <SidePanel
        open={sideOpen}
        tabs={[
          {
            id: 'chat',
            label: 'Chat',
            chat: true,
            empty: 'The lobby carries no chat yet.',
          },
          {
            id: 'players',
            label: 'Players',
            empty: 'Who else is here is not carried on the wire yet.',
          },
        ]}
      />
    </div>
  )
}
