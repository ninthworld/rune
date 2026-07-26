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
 * **This module is a mount point, not the composition.** The composition lives
 * in `src/pregame/` — the shared stage, the edge-anchored `MenuFrame`, the
 * server lobby, the create-table setup, and the ready room — and is re-exported
 * from `pregame/index.ts`. This file keeps the stable import path and mounts the
 * place inside the shared {@link PregameStage}, the environment the front door
 * and the match share.
 *
 * Issue #546 rebuilt those places against the approved 2.5D menu baselines in
 * `docs/ui-concepts/rune-pregame-*.jpg`: open games are the arena's focus and
 * selecting a table is what promotes Join to the one blue primary, the create
 * setup is a destination rather than an embedded form, and the room is a seat
 * ring rather than a roster. The hard rules below are unchanged by that.
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
 * - **One control family.** Every control here is a `ControlButton`/`IconButton`
 *   from `table/controls` (`docs/design/control-language.md` §3); the pregame
 *   draws surfaces of its own, never a second button.
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
