/**
 * The first screen (`docs/client-design.md` §9.3).
 *
 * It exists because a player should arrive at the lobby already being somebody, rather than
 * finding an input box in the header asking who they are. One panel on the dark ground carrying
 * the wordmark, a name prefilled with the last one this device used, the server list, and one
 * button — cut into the panel as a recess while the form is incomplete and raised as a pane the
 * moment it is not, because a disabled state is a state of the material rather than a greyed-out
 * button.
 *
 * The gear is here too, so card art can be set up before ever joining a table.
 *
 * The server list is `connect.ts`'s: client-side configuration, and never a protocol directory.
 * Choosing `Another address` reveals its field directly beneath the list, so the address is
 * adjacent to the choice it belongs to rather than parked elsewhere in the form.
 *
 * Neither field is a wire change. `hello` carries a token and nothing else, and the name is set
 * by the command this client already sends.
 */
import { useState } from 'react'

import { CUSTOM, entryFor, hostOf, serverChoices } from './../connect'
import type { ConnectionStatus } from './../socket'

export function Connect({
  name: remembered,
  address,
  status,
  onConnect,
  onSettings,
}: {
  name: string
  address: string
  status: ConnectionStatus
  onConnect(name: string, address: string): void
  onSettings(): void
}) {
  const choices = serverChoices()
  const [name, setName] = useState(remembered)
  const [pick, setPick] = useState(() => entryFor(address, choices).id)
  const [custom, setCustom] = useState(() =>
    entryFor(address, choices).id === CUSTOM ? address : '',
  )

  const chosen = choices.find((entry) => entry.id === pick)
  const host = pick === CUSTOM ? custom.trim() : (chosen?.url ?? '')
  const ready = name.trim() !== '' && host !== ''

  return (
    <div className="connect">
      <div className="topbar bare-topbar">
        <span className="topbar-fill" />
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
      </div>
      <div className="connect-stage">
        <form
          className="connect-panel"
          onSubmit={(event) => {
            event.preventDefault()
            if (ready) onConnect(name.trim(), host)
          }}
        >
          <div className="connect-mark">
            <h1 className="connect-title">SAGE</h1>
            <div className="connect-sub">
              <b>S</b>erver <b>A</b>uthoritative <b>G</b>ame <b>E</b>ngine
            </div>
          </div>

          <label className="connect-field">
            <span className="connect-label">Name</span>
            <input
              className="connect-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="How the table sees you"
              maxLength={24}
              autoFocus
            />
          </label>

          <div className="connect-field">
            <span className="connect-label">Server</span>
            <div className="server-list" role="radiogroup" aria-label="Server">
              {choices.map((server) => (
                <button
                  key={server.id}
                  type="button"
                  role="radio"
                  aria-checked={pick === server.id}
                  className={`server-row${pick === server.id ? ' server-on' : ''}`}
                  onClick={() => setPick(server.id)}
                >
                  <span className="server-dot" />
                  <span className="server-name">{server.label}</span>
                  <span className="server-host">
                    {server.id === CUSTOM ? custom || '—' : hostOf(server.url)}
                  </span>
                  <span className="server-ping">{server.region ?? ''}</span>
                </button>
              ))}
            </div>
            {pick === CUSTOM && (
              <input
                className="connect-input"
                aria-label="Address"
                value={custom}
                onChange={(event) => setCustom(event.target.value)}
                placeholder="ws://host:port"
              />
            )}
          </div>

          {/* The socket is already open to the address this page resolved, so the only thing
              worth saying is when it is not: a player pressing Connect against a server that is
              not answering should be told rather than left waiting. */}
          {status !== 'open' && (
            <p className="connect-note" role="status">
              {status === 'connecting' ? 'Reaching the server…' : 'The server is not answering.'}
            </p>
          )}

          <button className="action-done connect-go" disabled={!ready}>
            Connect
          </button>
        </form>
      </div>
    </div>
  )
}
