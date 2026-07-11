/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - One full-bleed Pixi canvas renders battlefield/hand/stack.
 * - React DOM islands render everything readable/clickable that is not a card.
 * - Both layers render from the latest GameView; no client-side game logic.
 *
 * The shell mounts the {@link Table}, which reconstructs the entire UI from the
 * store's latest GameView. Opening the connection (issue #34) is left to the
 * environment, so before the first frame the table shows a waiting state.
 */
import { Table } from './table/Table';

export function App() {
  return <Table />;
}
