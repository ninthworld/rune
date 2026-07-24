/**
 * The front-door landing screen (issue #103; identity redesign #300; migrated
 * onto the 2.5D visual system by #506 under
 * `docs/design/front-door-and-lobby.md` §5.1).
 *
 * This is the only UI shown before the first {@link LobbyView} arrives. It is
 * the product's front door, not an IP-entry form: the brand lockup leads, one
 * gold **Play** affordance connects, and the server address is a default (from
 * `VITE_RUNE_SERVER_URL`) tucked behind a "Server settings" disclosure.
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
