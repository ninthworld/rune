/**
 * The root: which contract the server put this connection on, and the two things that sit over
 * every screen whatever it is.
 *
 * **Which screen you are on is the server's answer.** A `GameView` arriving replaces everything
 * below, because the contract changed. There is no router, no client-held phase, and — since
 * `docs/client-design.md` §9.0 — no shell either: the topbar of each screen is its navigation,
 * and settings is a dialog over whatever you were already on, which is that guarantee in its
 * strongest form.
 *
 * Before either contract there is **connect** (§9.3): a name, a server, and a gear, so a player
 * arrives at the lobby already being somebody. The socket opens to the resolved address as it
 * always has — the precedence in `socket.ts` is untouched — and the connect screen is what stands
 * in front of the lobby until the player has said who they are. **A tab that already holds a
 * session token skips it**, because that tab is already somebody: a reload mid-match must land
 * back in the match, and asking a returning player to re-introduce themselves would be the
 * reconnect failing in a politer way.
 *
 * A dropped connection changes none of it. The session reconnects on its own and the server
 * holds the seat open, so a game already on screen stays on screen with the last view the server
 * sent; the banner says the connection is down, and the view is replaced the moment a real one
 * arrives. Blanking the board would throw away the only accurate picture the player has and
 * teach them to reload, which is the one thing reconnection exists to stop.
 *
 * Three things are mounted here rather than per screen, because a card is a card everywhere: the
 * pip gradients every pip on the page points at, the press-to-read gesture, and the card it
 * opens — which is also where a card with two faces turns over (§6.7).
 */
import { useCallback, useState } from 'react'

import type { CardFace } from './card-face'
import { initialAddress, readConnection, writeConnection } from './connect'
import { list } from './normalize'
import { Board } from './ui/game/Board'
import { Connect } from './ui/Connect'
import { Pregame } from './ui/pregame/Pregame'
import { Settings } from './ui/Settings'
import { Peek } from './ui/card/Peek'
import { PipDefs } from './ui/card/Pips'
import { PeekContext } from './ui/card/peek'
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
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [peeked, setPeeked] = useState<CardFace | undefined>(undefined)

  const session = useSession(address)

  // A game view is the server saying this connection is seated. Whoever it belongs to has
  // plainly already connected, so leaving that game returns them to the lobby rather than to a
  // screen asking them to introduce themselves to the server they are still talking to. Set
  // during render rather than in an effect: it is state adjusted from what just arrived, and an
  // effect would draw the connect screen for one frame first.
  if (session.game && !connected) setConnected(true)

  const enter = useCallback((chosen: string, chosenAddress: string) => {
    writeConnection(deviceStorage(), { name: chosen, server: chosenAddress })
    setName(chosen)
    setAddress(chosenAddress)
    setConnected(true)
  }, [])

  const settings = settingsOpen && (
    <Settings cards={list(session.catalog?.cards)} onClose={() => setSettingsOpen(false)} />
  )

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

  const screen = session.game ? (
    <Board
      view={session.game}
      connection={session.status}
      epoch={session.epoch}
      send={session.send}
      leave={session.restart}
      onSettings={() => setSettingsOpen(true)}
    />
  ) : session.spectator ? (
    <div className="lobby">
      <div className="lobby-main">
        <div className="zone-empty">Watching — the spectator screen is not built yet.</div>
      </div>
    </div>
  ) : !connected ? (
    <Connect
      name={name}
      address={address}
      status={session.status}
      onConnect={enter}
      onSettings={() => setSettingsOpen(true)}
    />
  ) : (
    <Pregame
      {...(session.lobby === undefined ? {} : { view: session.lobby })}
      {...(session.catalog === undefined ? {} : { catalog: session.catalog })}
      {...(session.lobbyError === undefined ? {} : { error: session.lobbyError })}
      epoch={session.epoch}
      name={session.lobby?.name ?? name}
      server={address}
      onSettings={() => setSettingsOpen(true)}
      onDisconnect={() => {
        setConnected(false)
        session.restart()
      }}
      send={session.send}
    />
  )

  return (
    <PeekContext.Provider value={setPeeked}>
      {screen}
      {notices}
      {settings}
      {/* Remounted per card, so a card pinned after a two-faced one opens on the side that is
          up rather than on whatever the last one was turned to. */}
      {peeked && <Peek key={peeked.id} face={peeked} onClose={() => setPeeked(undefined)} />}
      <PipDefs />
    </PeekContext.Provider>
  )
}
