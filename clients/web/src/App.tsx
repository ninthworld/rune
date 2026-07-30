/**
 * The shell: pick a screen from whatever the server last sent.
 *
 * There is no router and no client-held phase. The server switches this socket from the lobby
 * contract to the in-game one by sending a different frame, so the screen follows the frame —
 * once a game view has arrived it wins, because that is the contract now in force.
 */
import type { ReactNode } from 'react'

import { Game } from './ui/Game'
import { Lobby } from './ui/Lobby'
import { useSession } from './useSession'

export function App() {
  const session = useSession()

  if (session.status === 'connecting' && !session.lobby) {
    return <Waiting>Connecting…</Waiting>
  }
  if (session.status === 'closed' || session.status === 'error') {
    return (
      <Waiting>
        Disconnected. The server holds your seat for a while — reload to reclaim it.
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
      {session.game ? (
        <Game view={session.game} send={session.send} />
      ) : session.lobby ? (
        <Lobby view={session.lobby} error={session.lobbyError} send={session.send} />
      ) : session.spectator ? (
        <p>Watching — the spectator screen is not built yet.</p>
      ) : (
        <p>Waiting for the server…</p>
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
