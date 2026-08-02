/**
 * The root: which contract the server put this connection on, and which destination the player
 * asked for.
 *
 * **Which destination you are on is the client's answer. Which contract you are on is the
 * server's.** That is the whole rule this file implements, and it is the sharper form of "there
 * is no router and no client-held phase". A `GameView` arriving replaces everything below,
 * because the contract changed and the shell is not the table. Choosing Decks replaces nothing,
 * because nothing about the connection changed — a destination is a question about what the
 * player is looking at, never a second opinion about what the server said. Nothing derived from
 * a view is held here; the destination is not derived from a view at all.
 *
 * Before either of those there is **connect** (`docs/client-design.md` §9.3): a name, a server,
 * and a gear that reaches settings, so a player arrives at the lobby already being somebody. The
 * socket opens to the resolved address as it always has — the precedence in `socket.ts` is
 * untouched — and the connect screen is what stands in front of the shell until the player has
 * said who they are. **A tab that already holds a session token skips it**, because that tab is
 * already somebody: a reload mid-match must land back in the match, and asking a returning
 * player to re-introduce themselves would be the reconnect failing in a politer way.
 *
 * A dropped connection changes none of it. The session reconnects on its own and the server
 * holds the seat open, so a game already on screen stays on screen with the last view the server
 * sent; the banner says the connection is down, and the view is replaced the moment a real one
 * arrives. Blanking the board would throw away the only accurate picture the player has and
 * teach them to reload, which is the one thing reconnection exists to stop.
 */
import { useCallback, useState } from 'react'

import { initialAddress, readConnection, writeConnection } from './connect'
import { list } from './normalize'
import type { Destination } from './shell'
import { Game } from './ui/Game'
import { Lobby } from './ui/Lobby'
import { Connect } from './ui/Connect'
import { Shell } from './ui/shell/Shell'
import { SettingsScreen } from './ui/shell/SettingsScreen'
import { heldSession, useSession } from './useSession'

/** Absent in a browser with storage disabled, which is a normal way to run. */
const deviceStorage = (): Storage | undefined => {
  try {
    return globalThis.localStorage
  } catch {
    return undefined
  }
}

export function App() {
  const [address, setAddress] = useState(() => initialAddress(deviceStorage()))
  const [name, setName] = useState(() => readConnection(deviceStorage()).name)
  // A tab holding a token is a tab that has already introduced itself.
  const [connected, setConnected] = useState(() => heldSession() !== undefined)
  const [destination, setDestination] = useState<Destination>('play')
  // Settings before there is a rail to reach it by. It is the same screen either way.
  const [settingsFromConnect, setSettingsFromConnect] = useState(false)

  const session = useSession(address)

  // A game view is the server saying this connection is seated. Whoever it belongs to has
  // plainly already connected, so leaving that game returns them to the shell rather than to a
  // screen asking them to introduce themselves to the server they are still talking to. Set
  // during render rather than in an effect: it is state adjusted from what just arrived, and an
  // effect would draw the connect screen for one frame first.
  if (session.game && !connected) setConnected(true)

  const enter = useCallback((chosen: string, chosenAddress: string) => {
    writeConnection(deviceStorage(), { name: chosen, server: chosenAddress })
    setName(chosen)
    setAddress(chosenAddress)
    setConnected(true)
    setSettingsFromConnect(false)
  }, [])

  const rename = useCallback((next: string) => {
    writeConnection(deviceStorage(), { name: next, server: initialAddress(deviceStorage()) })
    setName(next)
  }, [])

  // The contract in force outranks everything: a game view is the server saying this socket is
  // no longer speaking the lobby's language.
  if (session.game) {
    return (
      <main>
        <Game
          view={session.game}
          connection={session.status}
          epoch={session.epoch}
          send={session.send}
          leave={session.restart}
        />
      </main>
    )
  }

  if (session.spectator) {
    return (
      <main>
        <p>Watching — the spectator screen is not built yet.</p>
      </main>
    )
  }

  const notices = (
    <>
      {session.unknownFrames > 0 && (
        <p role="status" className="notice">
          {session.unknownFrames} message{session.unknownFrames === 1 ? '' : 's'} from the server
          could not be read by this client. It may be newer than this build.
        </p>
      )}
      {connected && session.status !== 'open' && (
        <p role="status" className="notice">
          Reconnecting to the server…
        </p>
      )}
    </>
  )

  return (
    <main>
      <div className="app">
        {notices}
        {!connected ? (
          settingsFromConnect ? (
            <SettingsScreen onBack={() => setSettingsFromConnect(false)} />
          ) : (
            <Connect
              name={name}
              address={address}
              status={session.status}
              onConnect={enter}
              onSettings={() => setSettingsFromConnect(true)}
            />
          )
        ) : (
          <Shell
            destination={destination}
            onDestination={setDestination}
            identity={session.lobby?.name ?? (name || session.lobby?.you) ?? '…'}
          >
            {destination === 'settings' ? (
              <SettingsScreen
                name={session.lobby?.name ?? name}
                {...(list(session.lobby?.valid_commands).includes('set_name')
                  ? { onName: rename }
                  : {})}
              />
            ) : (
              <Lobby
                view={session.lobby}
                catalog={session.catalog}
                error={session.lobbyError}
                epoch={session.epoch}
                name={name}
                destination={destination}
                onDestination={setDestination}
                send={session.send}
              />
            )}
          </Shell>
        )}
      </div>
    </main>
  )
}
