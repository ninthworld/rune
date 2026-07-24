/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - The shipped match is the ADR 0030 2.5D composition: battlefield cards render
 *   in a DOM scene plane with Pixi kept as a passive effects overlay (#494 retired
 *   the legacy ADR 0003 Pixi match table). The read-only spectate mode rides the
 *   same DOM scene plane, staged receiver-less (#504 retired the legacy stack).
 * - React DOM owns screen chrome and every readable/clickable surface.
 * - Every layer renders from the latest GameView; no client-side game logic.
 *
 * The shell branches on the store's lifecycle, walking the never-a-dead-screen
 * flow address → lobby → game:
 *
 * - Before a socket is open it shows the {@link ConnectionScreen} (URL entry /
 *   connecting / closed-with-retry) — issue #103.
 * - Once the socket is `open` it shows the {@link LobbyScreen} (issue #114): the
 *   store greets the server and this screen reconstructs the pre-game UI (room,
 *   seat roster, deck, ready) from the latest `LobbyView`, with its own
 *   interactive "entering the lobby…" fallback before the first frame.
 * - The instant the first `GameView` arrives (the game is constructed) it mounts
 *   the selected table composition, which reconstructs the whole UI from that view.
 *
 * The gates are purely presentational; the `GameView`/`LobbyView` remain the only
 * load-bearing state, and a disconnect from either screen falls back to an
 * interactive screen (the connection screen), never a dead one.
 */
import { useEffect } from 'react';
import { FrontDoor, LobbyContent, PregameStage, pregamePlace } from './pregame';
import { useGameStore } from './store';
import { LiveMatchTable } from './table/live';
import { SpectatorTable } from './table/SpectatorTable';

export function App() {
  const status = useGameStore((state) => state.status);
  const view = useGameStore((state) => state.view);
  const spectatorView = useGameStore((state) => state.spectatorView);
  const lobby = useGameStore((state) => state.lobby);

  // On mount — including a hard page reload — try to reclaim a held seat from a
  // persisted session token before falling back to the connection screen (issue #254).
  // A no-op when there is nothing stored or a socket is already live, so it is safe to
  // run once per mount.
  useEffect(() => {
    useGameStore.getState().restoreSession();
  }, []);

  // A GameView means the game has been constructed: mount the 2.5D match table
  // (in-game contract for the life of the game).
  if (view !== null) {
    return <LiveMatchTable />;
  }
  // A SpectatorView means this connection is watching a live game (ADR 0022, issue
  // #351): mount the read-only spectate mode.
  if (spectatorView !== null) {
    return <SpectatorTable view={spectatorView} />;
  }

  // Front door, Lobby, and Room are three places on ONE stage
  // (`docs/design/front-door-and-lobby.md` §4.1). The stage — and with it the
  // environment backdrop — is mounted here, once, so a place change moves
  // content and never re-mounts the world; that is what makes the crossing into
  // the match invisible. Which place shows is derived from the socket status
  // plus the latest `LobbyView`, never stored.
  const place = pregamePlace(status, lobby?.room !== undefined, lobby !== null);
  return (
    <PregameStage place={place}>
      {place === 'front-door' ? <FrontDoor /> : <LobbyContent />}
    </PregameStage>
  );
}
