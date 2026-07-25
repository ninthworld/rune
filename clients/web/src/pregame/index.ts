/**
 * The pregame places (issue #506) — the front door, lobby, and room on the 2.5D
 * visual system, split along the seams `docs/design/front-door-and-lobby.md` §7
 * names (stage shell, front door, lobby entry, room, roster, ready bar) and
 * re-exported from here.
 *
 * The composition is the design document's §5; the root `LobbyScreen.tsx` and
 * `ConnectionScreen.tsx` remain the stable mount points that wrap a place in the
 * shared stage.
 */
export { PregameStage } from './PregameStage';
export { PregameHeader } from './PregameHeader';
export { SessionMenu } from './SessionMenu';
export { FrontDoor, DEFAULT_SERVER_URL } from './FrontDoor';
export { LobbyContent, LobbyPlace, IdentityRow } from './LobbyPlace';
export { RoomPlace } from './RoomPlace';
export { RoomDirectory, RoomDirectoryRow, DirectorySkeleton } from './RoomDirectory';
export { StartGameCard, type StartMode, type StartRequest } from './StartGameCard';
export { Roster, RosterRow, CrestChip } from './Roster';
export { ReadyBar, type ReadyBarProps } from './ReadyBar';
export { AiSeatingCard } from './AiSeatingCard';
export { LastMatchRibbon } from './LastMatchRibbon';
export { DeckGrid, DeckTile } from './DeckPicker';
export { commanderName, deckLandGlyphs, BASIC_LAND_GLYPHS } from './deckPresentation';
export { readyGate, waitingForNames, type ReadyGateState, type ReadyGateGold } from './readyGate';
export { seatFilled, seatMonogram } from './seatIdentity';
export { GAME_SETUPS, SEAT_COUNTS, setupLabel, type GameSetupOption } from './gameSetups';
export {
  pregameSceneVars,
  pregamePlace,
  seatAccent,
  seatAccentVars,
  type PregamePlace,
  type SceneVars,
} from './pregameScene';
