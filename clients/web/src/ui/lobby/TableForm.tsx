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
 *
 * What a format *requires* rides on the format's own option rather than on a line under the
 * form (`docs/client-design.md` §9.2 rule 1). A deck minimum and a copy limit are the whole
 * substance of choosing between two formats, and printing them beside the control instead of on
 * the options makes a reader hold one in their head while they look at the other.
 */
import { useState } from 'react'

import type { CatalogFormat, RoomConfig } from './../../protocol'
import { deckRules } from './../../deck'
import { Choice, TextField } from './../controls'

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
      <p className="page__pending">
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
      <div className="table-form__group">
        <span className="field__label">Format</span>
        <Choice
          label="Format"
          columns
          value={gameSetup}
          options={formats.map((candidate) => {
            const rules = deckRules(candidate).join(' · ')
            return {
              value: candidate.game_setup,
              label: candidate.game_setup,
              ...(rules ? { detail: rules } : {}),
            }
          })}
          onChange={pickFormat}
        />
      </div>

      <div className="table-form__group">
        <span className="field__label">Seats</span>
        <Choice
          label="Seats"
          value={String(seats)}
          options={range.map((count) => ({ value: String(count), label: String(count) }))}
          onChange={(value) => setSeats(Number(value))}
        />
      </div>

      <TextField label="Table name" value={name} maxLength={32} onChange={setName} />

      <div className="table-form__group">
        <span className="field__label">Who can find it</span>
        <Choice
          label="Visibility"
          columns
          value={privateTable ? 'private' : 'public'}
          options={[
            { value: 'public', label: 'Public', detail: 'Listed for everybody' },
            { value: 'private', label: 'Private', detail: 'Unlisted, reachable only by its id' },
          ]}
          onChange={(value) => setPrivateTable(value === 'private')}
        />
      </div>

      <p className="table-form__controls">
        <button type="submit" className="page__lead">
          {submitLabel}
        </button>
        {onCancel && (
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        )}
      </p>
    </form>
  )
}
