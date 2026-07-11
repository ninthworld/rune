/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - One full-bleed Pixi canvas renders battlefield/hand/stack.
 * - React DOM islands render everything readable/clickable that is not a card.
 * - Both layers render from the latest GameView; no client-side game logic.
 *
 * The shell branches on the store's connection lifecycle (issue #103): before a
 * socket is open it shows the {@link ConnectionScreen} (URL entry / connecting /
 * closed-with-retry). Once the socket is `open` — including the brief window
 * before the first frame — it mounts the {@link Table}, which reconstructs the
 * whole UI from the latest GameView (and shows its own "waiting for first state"
 * fallback until that frame lands). The gate is purely presentational; the
 * GameView remains the one load-bearing piece of state.
 */
import { ConnectionScreen } from './ConnectionScreen';
import { useGameStore } from './store';
import { Table } from './table/Table';

export function App() {
  const status = useGameStore((state) => state.status);
  const view = useGameStore((state) => state.view);

  // Show the table once the socket is open (its fallback covers the pre-first-frame
  // wait) or whenever a view exists; otherwise drive the connection lifecycle.
  if (status === 'open' || view !== null) {
    return <Table />;
  }
  return <ConnectionScreen />;
}
