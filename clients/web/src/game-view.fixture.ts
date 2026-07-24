/**
 * Barrel for the shared `GameView` wire/normalized test fixtures. The fixtures were
 * split into focused modules under `game-view.fixtures/` (issue #438), grouped by the
 * client concern they exercise; this root re-exports every one so existing imports
 * from `game-view.fixture` keep working unchanged. See each module for the fixtures it
 * owns:
 *
 * - `sample`    — the representative base frame (wire JSON + fully-normalized object).
 * - `combat`    — targeting and attacker/blocker/combat declaration frames, incl.
 *                 multiplayer and the four-player split-attack scene.
 * - `prompts`   — decision/prompt frames (option, select_from_zone, order): mulligan,
 *                 bottom, mode, order, non-board zone select, discard.
 * - `zones`     — populated public zones and the commander-format frame.
 * - `game-over` — terminal frames and the shared game-over builder.
 */
export * from './game-view.fixtures/sample';
export * from './game-view.fixtures/combat';
export * from './game-view.fixtures/prompts';
export * from './game-view.fixtures/zones';
export * from './game-view.fixtures/game-over';
