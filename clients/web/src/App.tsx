/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - One full-bleed Pixi canvas renders battlefield/hand/stack.
 * - React DOM islands render everything readable/clickable that is not a card.
 * - Both layers render from the latest GameView; no client-side game logic.
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
 *   the {@link Table}, which reconstructs the whole UI from that view.
 *
 * The gates are purely presentational; the `GameView`/`LobbyView` remain the only
 * load-bearing state, and a disconnect from either screen falls back to an
 * interactive screen (the connection screen), never a dead one.
 */
import { useEffect } from 'react';
import { ConnectionScreen } from './ConnectionScreen';
import { LobbyScreen } from './LobbyScreen';
import { useGameStore } from './store';
import { Table } from './table/Table';
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

  // A GameView means the game has been constructed: mount the table (in-game
  // contract for the life of the game).
  if (view !== null) {
    return <Table />;
  }
  // A SpectatorView means this connection is watching a live game (ADR 0022, issue
  // #351): mount the read-only spectate mode.
  if (spectatorView !== null) {
    return <SpectatorTable view={spectatorView} />;
  }
  // Socket open (or a lobby frame already in hand): drive the pre-game lobby. The
  // lobby screen covers the pre-first-frame wait with its own fallback.
  if (status === 'open' || lobby !== null) {
    return <LobbyScreen />;
  }
  return <ConnectionScreen />;
}
