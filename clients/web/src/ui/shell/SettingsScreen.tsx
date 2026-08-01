/**
 * Settings: one destination for everything about this device, sectioned.
 *
 * `docs/client-design.md` §9.6. Card art used to be a top-level button in the lobby header,
 * competing with the game for the most expensive space on the screen to reach one preference —
 * and it was never one preference. Obtaining and managing art is a real surface with real state:
 * which source, what that source is allowed to be asked for, what is on this device, and how to
 * get rid of it. That was never going to fit behind a header button, and this is where it lives.
 *
 * The rules of ADR 0012 are unchanged and this is where they become visible to the player: the
 * fetch is theirs, the cache is their device's, and nothing is bundled, served, proxied, or
 * redistributed.
 *
 * It is reachable from the connect screen as well as from the rail, which is why `onBack` is
 * optional — from the rail there is a rail to go back with, and from connect there is not.
 */
import { useState } from 'react'

import { ArtControls } from './../ArtSettings'
import { TextField } from './../controls'

export function SettingsScreen({
  name,
  onName,
  onBack,
}: {
  /** The name the server accepted, when there is a connection whose name can be changed. */
  name?: string
  /** Absent when the server is not offering `set_name`, and on the connect screen. */
  onName?: (name: string) => void
  /** Drawn only where there is no rail to return by. */
  onBack?: () => void
}) {
  const [typed, setTyped] = useState(name ?? '')

  return (
    <div className="page">
      <header className="page__head">
        <h1>Settings</h1>
        {onBack && (
          <button type="button" onClick={onBack}>
            Back
          </button>
        )}
      </header>

      {onName && (
        <section className="page__section" aria-labelledby="settings-you">
          <h2 id="settings-you">You</h2>
          <div className="page__row">
            <TextField
              label="Display name"
              value={typed}
              maxLength={32}
              onChange={setTyped}
              onEnter={() => onName(typed.trim())}
            />
            <button
              type="button"
              disabled={typed.trim().length === 0 || typed.trim() === name}
              onClick={() => onName(typed.trim())}
            >
              Change it
            </button>
          </div>
        </section>
      )}

      <section className="page__section" aria-labelledby="settings-art">
        <h2 id="settings-art">Card art</h2>
        <ArtControls />
      </section>

      {/* Two things §9.6 puts in this section that have no implementation behind them yet. They
          are named rather than mocked up: a control that does nothing is worse than an empty
          shelf, and an empty shelf is where the next person looks. */}
      <section className="page__section" aria-labelledby="settings-backs">
        <h2 id="settings-backs">Card backs</h2>
        <p className="page__pending">
          SAGE draws one back. Choosing between them is not built yet.
        </p>
      </section>

      <section className="page__section" aria-labelledby="settings-symbols">
        <h2 id="settings-symbols">Symbols</h2>
        <p className="page__pending">
          Mana and tap symbols are the project’s own discs, drawn in CSS. Nothing is downloaded and
          there is nothing to choose yet.
        </p>
      </section>
    </div>
  )
}
