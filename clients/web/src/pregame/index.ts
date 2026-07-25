/**
 * The pregame places (issues #506 and #546) — the front door, the server lobby,
 * the create-table setup, and the ready room, all composed on **one**
 * `PregameStage` and **one** edge-anchored `MenuFrame`, and re-exported from
 * here.
 *
 * The composition answers the four approved baselines in
 * `docs/ui-concepts/rune-pregame-*.jpg`; the root `LobbyScreen.tsx` and
 * `ConnectionScreen.tsx` remain the stable mount points that wrap a place in the
 * shared stage.
 */
export { PregameStage } from './PregameStage';
export {
  MenuFrame,
  Plaque,
  Lockup,
  ServerPlaque,
  ChatEdge,
  SessionMenu,
  type MenuFrameProps,
  type PlaqueProps,
  type SessionMenuProps,
} from './MenuFrame';
export { FrontDoor } from './FrontDoor';
export { DEFAULT_SERVER_URL, initialServerUrl, serverLabel } from './serverIdentity';
export { LobbyContent, LobbyPlace, IdentityRow } from './LobbyPlace';
export { RoomPlace } from './RoomPlace';
export { RoomDirectory, RoomDirectoryRow, DirectorySkeleton } from './RoomDirectory';
export { CreateGame, type CreatePrefill } from './CreateGame';
export { SeatPlaque, CrestChip } from './SeatPlaque';
export { DeckChoice, type DeckChoiceProps } from './DeckChoice';
export {
  deckOptions,
  deckOptionById,
  optionCounts,
  useSavedDecks,
  type DeckChoiceOption,
} from './deckChoice';
export { LastMatchRibbon } from './LastMatchRibbon';
export { commanderName } from './deckPresentation';
export { readyGate, waitingForNames, type ReadyGateState, type ReadyGateGold } from './readyGate';
export { seatFilled, seatMonogram } from './seatIdentity';
export { seatRing, RING_RX, RING_RY, type SeatSlot } from './seatRing';
export { GAME_SETUPS, SEAT_COUNTS, setupLabel, type GameSetupOption } from './gameSetups';
export {
  pregameSceneVars,
  pregamePlace,
  seatAccent,
  seatAccentVars,
  type PregamePlace,
  type SceneVars,
} from './pregameScene';
