/**
 * One form for creating a table and for editing one.
 *
 * `create_room` and `update_room` both carry a **whole** `RoomConfig` rather than a patch, which
 * is the protocol saying a table is described, not adjusted — so one surface serves both and the
 * only difference is which command the composer sends and what the form opened with.
 *
 * Every choice offered here comes from the catalog the server published: the formats are its
 * `game_setup` ids, and the seat counts are that format's own advertised range. Nothing is
 * hardcoded, so a format the server adds appears here without a client change — and a client
 * that has not been handed a catalog offers no format at all rather than guessing one.
 */
import { useState } from 'react'

import type { CatalogFormat, RoomConfig } from './../../protocol'
import { deckRules, seatRange } from './../../deck'

export function TableForm({
  formats,
  initial,
  submitLabel,
  onSubmit,
  onCancel,
}: {
  formats: readonly CatalogFormat[]
  /** The config to open with, when editing an existing table. */
  initial?: RoomConfig
  submitLabel: string
  onSubmit(config: RoomConfig): void
  onCancel?(): void
}) {
  const [gameSetup, setGameSetup] = useState(initial?.game_setup ?? formats[0]?.game_setup ?? '')
  const [seats, setSeats] = useState(initial?.seats ?? formats[0]?.min_seats ?? 2)
  const [name, setName] = useState(initial?.name ?? '')
  const [privateTable, setPrivateTable] = useState(initial?.visibility === 'private')

  const format = formats.find((candidate) => candidate.game_setup === gameSetup)
  // The range is the format's own; a format the catalog has not described contributes none, and
  // the seat count stays whatever it opened with rather than being invented.
  const range = format
    ? Array.from(
        { length: format.max_seats - format.min_seats + 1 },
        (_, i) => format.min_seats + i,
      )
    : [seats]

  if (formats.length === 0) {
    return (
      <p className="table-form__waiting">
        Waiting for the server’s format list. Tables are created from the formats it advertises.
      </p>
    )
  }

  const pickFormat = (id: string) => {
    setGameSetup(id)
    const picked = formats.find((candidate) => candidate.game_setup === id)
    // Carry the seat count across only where the new format allows it; otherwise take its floor.
    if (picked && (seats < picked.min_seats || seats > picked.max_seats)) setSeats(picked.min_seats)
  }

  return (
    <form
      className="table-form"
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit({
          seats,
          game_setup: gameSetup,
          ...(name.trim() ? { name: name.trim() } : {}),
          ...(privateTable ? { visibility: 'private' as const } : {}),
        })
      }}
    >
      <label>
        Format{' '}
        <select value={gameSetup} onChange={(event) => pickFormat(event.target.value)}>
          {formats.map((candidate) => (
            <option key={candidate.game_setup} value={candidate.game_setup}>
              {candidate.game_setup}
            </option>
          ))}
        </select>
      </label>

      <label>
        Seats{' '}
        <select value={seats} onChange={(event) => setSeats(Number(event.target.value))}>
          {range.map((count) => (
            <option key={count} value={count}>
              {count}
            </option>
          ))}
        </select>
      </label>

      <label>
        Table name{' '}
        <input
          value={name}
          placeholder="optional"
          maxLength={32}
          onChange={(event) => setName(event.target.value)}
        />
      </label>

      <label>
        <input
          type="checkbox"
          aria-label="Private table"
          checked={privateTable}
          onChange={(event) => setPrivateTable(event.target.checked)}
        />{' '}
        Private — unlisted, reachable only by its id
      </label>

      <p className="table-form__rules">
        {[seatRange(format), ...deckRules(format)].filter(Boolean).join(' · ')}
      </p>

      <p className="table-form__controls">
        <button type="submit">{submitLabel}</button>{' '}
        {onCancel && (
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        )}
      </p>
    </form>
  )
}
