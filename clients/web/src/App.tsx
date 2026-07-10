/**
 * RUNE web client shell.
 *
 * Architecture (see AGENTS.md in this package):
 * - One full-bleed Pixi canvas renders battlefield/hand/stack.
 * - React DOM islands render everything readable/clickable that is not a card.
 * - Both layers render from the latest GameView; no client-side game logic.
 */
export function App() {
  return <main>RUNE client scaffold — see clients/web/AGENTS.md</main>;
}
