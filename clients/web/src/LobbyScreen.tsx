/**
 * The pre-game lobby screen (issue #114; identity redesign #300; migrated onto
 * the 2.5D visual system by #506 under `docs/design/front-door-and-lobby.md`).
 *
 * The screen shown between the {@link ConnectionScreen} landing and the match:
 * after the socket opens, the store greets the server (`Hello`) and this screen
 * renders the resulting {@link LobbyView} — browse the room directory (the
 * primary "find a game" path), create a room or join one by id (the secondary
 * paths, now one card with two modes), pick a starter deck, submit it, and ready
 * up. When every seat is filled, decked, and ready the server constructs the
 * game and pushes the first `GameView`; the app then switches to the table.
 *
 * **This module is a mount point, not the composition.** The design document's
 * §7 requires the ~1 070-line original to land split along its seams, so the
 * composition lives in `src/pregame/` (stage shell, front door, lobby entry,
 * room, roster, ready bar) and is re-exported from `pregame/index.ts`. This file
 * keeps the stable import path and mounts the place inside the shared
 * {@link PregameStage} — the environment the front door and the match share.
 *
 * Hard rules (AGENTS.md, ADR 0012) — unchanged by the restyle:
 * - **Reconstruct from one `LobbyView`.** Every control is derived from the
 *   store's latest view; nothing about the lobby is load-bearing across
 *   messages. Local state is ephemeral form input plus the explicitly-ephemeral
 *   last-match ribbon, which the lobby renders identically without.
 * - **`valid_commands` is the only source of interactivity.** Client-session
 *   actions (connect, disconnect, open settings, dismiss the ribbon) are the
 *   only controls that do not come from the view.
 * - **No card logic**; the server validates a submitted deck authoritatively.
 * - **Never a dead screen.** Before the first `LobbyView`, and on every error, an
 *   interactive control is always on screen.
 */
import { useGameStore } from './store';
import { LobbyContent, PregameStage } from './pregame';

export function LobbyScreen() {
  const lobby = useGameStore((state) => state.lobby);
  // Lobby and Room are two places on one stage; which one shows is derived from
  // the view, never stored.
  const place = lobby?.room !== undefined ? 'room' : 'lobby';

  return (
    <PregameStage place={place}>
      <LobbyContent />
    </PregameStage>
  );
}
