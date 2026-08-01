/**
 * The first screen: who you are, and where you are playing.
 *
 * It exists so a player arrives at the lobby **already being somebody**, rather than finding an
 * input box in the header asking who they are — `docs/client-design.md` §9.2 rule 5, setup
 * happens in setup. The name is prefilled with the last one this device used, so a returning
 * player presses one key; the server list carries each entry's region, because "which of these
 * is near me" is the only question a list of addresses raises.
 *
 * **Neither field is a wire change.** `hello` carries a token and nothing else, and the name is
 * set by `set_name`, the command this client already sent — from the header of every screen,
 * forever, which is the thing being fixed.
 *
 * The gear is here too, so card art can be chosen before ever joining a table. That is the whole
 * of why settings is a destination rather than a dialog: it has to be reachable from a screen
 * that has no rail yet.
 */
import { useState } from 'react'

import { CUSTOM, entryFor, serverChoices, type ServerEntry } from './../connect'
import type { ConnectionStatus } from './../socket'
import { Choice, TextField } from './controls'

export function Connect({
  name: remembered,
  address,
  status,
  onConnect,
  onSettings,
}: {
  /** The last name this device used, or empty. */
  name: string
  /** The address this client would open with — configuration, or what was chosen last. */
  address: string
  status: ConnectionStatus
  onConnect(name: string, address: string): void
  onSettings(): void
}) {
  const [choices] = useState<readonly ServerEntry[]>(serverChoices)
  const opening = entryFor(address, choices)
  const [name, setName] = useState(remembered)
  const [chosen, setChosen] = useState(opening.id)
  // Seeded with the address only when it is *already* a custom one, so choosing the custom entry
  // opens an empty field rather than one prefilled with somebody else's server.
  const [typed, setTyped] = useState(opening.id === CUSTOM ? address : '')

  const entry = choices.find((candidate) => candidate.id === chosen) ?? opening
  const url = entry.id === CUSTOM ? typed.trim() : entry.url
  const connect = () => url.length > 0 && onConnect(name.trim(), url)

  return (
    <div className="connect">
      <div className="connect__panel">
        <header className="connect__head">
          <h1>SAGE</h1>
          <button
            type="button"
            className="connect__gear"
            aria-label="Settings"
            onClick={onSettings}
          >
            <span aria-hidden="true">⚙</span>
          </button>
        </header>

        <TextField
          label="Name"
          value={name}
          maxLength={32}
          autoFocus
          placeholder="what the table calls you"
          onChange={setName}
          onEnter={connect}
        />

        <div className="connect__servers">
          <span className="field__label">Server</span>
          <Choice
            label="Server"
            columns
            value={entry.id}
            options={choices.map((candidate) => ({
              value: candidate.id,
              label: candidate.label,
              ...(candidate.region ? { detail: candidate.region } : {}),
            }))}
            onChange={setChosen}
          />
        </div>

        {entry.id === CUSTOM && (
          <TextField
            label="Address"
            value={typed}
            placeholder="ws://host:9000"
            onChange={setTyped}
            onEnter={connect}
          />
        )}

        <p className="connect__go">
          <button
            type="button"
            className="connect__connect"
            disabled={url.length === 0}
            onClick={connect}
          >
            Connect
          </button>
          {/* The connection's own state, where it belongs — on the screen whose one button
              opens one. It is drawn only when it is not the ordinary answer. */}
          {status !== 'open' && (
            <span role="status" className="connect__status">
              {status === 'connecting' ? 'Reaching the server…' : 'The server is not answering'}
            </span>
          )}
        </p>
      </div>
    </div>
  )
}
