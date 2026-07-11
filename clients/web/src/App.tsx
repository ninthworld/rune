/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - One full-bleed Pixi canvas renders battlefield/hand/stack.
 * - React DOM islands render everything readable/clickable that is not a card.
 * - Both layers render from the latest GameView; no client-side game logic.
 *
 * The shell mounts either the {@link ConnectionScreen} or {@link Table} depending
 * on connection state. Opening the connection (issue #34) is left to the
 * environment, so before the first frame we show a connection screen.
 */
import { Table } from './table/Table';
import { ConnectionScreen } from './components/ConnectionScreen';
import { useGameStore } from './store';

export function App() {
  const view = useGameStore((state) => state.view);
  const status = useGameStore((state) => state.status);

  // Show connection screen when there's no game view or when not connected
  if (!view || status === 'idle' || status === 'closed') {
    return <ConnectionScreen />;
  }

  return <Table />;
}
