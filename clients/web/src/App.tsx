/**
 * The shell: pick a screen from whatever the server last sent.
 *
 * There is no router and no client-held phase. The server switches this socket from the lobby
 * contract to the in-game one by sending a different frame, so the screen follows the frame —
 * once a game view has arrived it wins, because that is the contract now in force.
 *
 * A dropped connection does not change that. The session reconnects on its own and the server
 * holds the seat open, so a game already on screen stays on screen with the last view the server
 * sent; the header says the connection is down, and the view is replaced the moment a real one
 * arrives. Blanking the board would throw away the only accurate picture the player has and
 * teach them to reload, which is the one thing reconnection exists to stop.
 */
import type { ReactNode } from 'react'

import { Game } from './ui/Game'
import { Lobby } from './ui/Lobby'
import { useSession } from './useSession'

export function App() {
  const session = useSession()

  if (!session.game && !session.lobby && !session.spectator) {
    return (
      <Waiting>
        {session.status === 'connecting' ? 'Connecting…' : 'Reconnecting to the server…'}
      </Waiting>
    )
  }

  return (
    <main>
      {session.unknownFrames > 0 && (
        <p role="status" className="notice">
          {session.unknownFrames} message{session.unknownFrames === 1 ? '' : 's'} from the server
          could not be read by this client. It may be newer than this build.
        </p>
      )}
      {/* The game screen is full-viewport and says this in its own header, where it costs no
          layout. Every other screen has room for a banner. */}
      {!session.game && session.status !== 'open' && (
        <p role="status" className="notice">
          Reconnecting to the server…
        </p>
      )}
      {session.game ? (
        <Game
          view={session.game}
          connection={session.status}
          epoch={session.epoch}
          send={session.send}
          leave={session.restart}
        />
      ) : session.lobby ? (
        <Lobby
          view={session.lobby}
          catalog={session.catalog}
          error={session.lobbyError}
          epoch={session.epoch}
          send={session.send}
        />
      ) : (
        <p>Watching — the spectator screen is not built yet.</p>
      )}
    </main>
  )
}

function Waiting({ children }: { children: ReactNode }) {
  return (
    <main>
      <h1>SAGE</h1>
      <p>{children}</p>
    </main>
  )
}
