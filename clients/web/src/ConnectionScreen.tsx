/**
 * The front-door landing screen (issue #103; identity redesign #300; migrated
 * onto the 2.5D visual system by #506 under
 * `docs/design/front-door-and-lobby.md` §5.1).
 *
 * This is the only UI shown before the first {@link LobbyView} arrives. It is
 * the product's front door, not an IP-entry form: the wordmark leads, the
 * already-chosen server is named on its plaque, one blue **Connect** primary
 * reaches it, and the address (from `VITE_RUNE_SERVER_URL`) is tucked behind a
 * quiet "Change server" disclosure — the approved
 * `docs/ui-concepts/rune-pregame-server-connection.jpg` baseline (issue #546).
 *
 * **This module is a mount point, not the composition.** The front door itself
 * is `pregame/FrontDoor.tsx`; this file keeps the stable import path and mounts
 * it inside the shared {@link PregameStage}, so the environment behind the front
 * door is the same node that carries through the lobby, the room, and — as the
 * match's own backdrop — into the game.
 */
import { FrontDoor, PregameStage } from './pregame';

export { DEFAULT_SERVER_URL } from './pregame';

export function ConnectionScreen() {
  return (
    <PregameStage place="front-door">
      <FrontDoor />
    </PregameStage>
  );
}
