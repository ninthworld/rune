/**
 * Scene-token wiring for the pregame places — front door, lobby, and room
 * (issue #506, `docs/design/front-door-and-lobby.md` §5.0).
 *
 * The screens around the match speak the same 2.5D visual language as the match
 * itself: the scene palette (`sceneTokens.ts`), the elevation ladder (§3), the
 * default environment theme's six slots (§4), and the motion grammar (§8) reach
 * the CSS-module layer as `--pregame-*` custom properties assembled here — the
 * ADR 0019 pattern the DOM card face established and `deck/deckScene.ts` already
 * follows for the deck surfaces.
 *
 * The reduced-motion collapse is wired at the token level (the carried
 * contract): every duration resolves through {@link sceneMotionMs}, so under
 * reduced motion the durations are `0` and every place change, lift, shimmer,
 * and ribbon snaps to its end state with no layout or state difference.
 *
 * No new token values live here. If a value is needed and absent it is added to
 * `sceneTokens.ts` under its lockstep/contrast test, never invented in CSS — so
 * the pregame CSS modules introduce no literal hex and no literal duration.
 *
 * Pure functions over constants: no game logic, no I/O.
 */
import type { CSSProperties } from 'react';
import {
  DEFAULT_SCENE_THEME,
  SCENE_ELEVATION,
  SCENE_HUES,
  SCENE_MOTION,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
  SCENE_THEMES,
  sceneMotionMs,
} from '../sceneTokens';

/** Custom-property style object usable as an inline `style`. */
export type SceneVars = CSSProperties & Record<`--${string}`, string | number>;

/**
 * The four pregame places of `front-door-and-lobby.md` §2. `Match` and
 * `Watching` are their own compositions and never mount this stage.
 */
export type PregamePlace = 'front-door' | 'lobby' | 'room';

/**
 * Which place the store's state names. Derived, never stored: the flow is
 * reconstructable from the socket status plus the latest `LobbyView` alone.
 */
export function pregamePlace(status: string, hasRoom: boolean, hasLobby: boolean): PregamePlace {
  if (status !== 'open' && !hasLobby) return 'front-door';
  return hasRoom ? 'room' : 'lobby';
}

/**
 * The scene-token CSS custom properties the pregame surfaces render through:
 * the dark table-world neutrals, the disciplined interaction hues, the six seat
 * accents, the elevation-ladder shadows (rest → lifted → held → screen), the
 * default theme's environment slots, and the motion-grammar durations/easings
 * for the `staging` (place change), `micro` (hover / selection / relabel), and
 * `tapUntap` (the ≤ 200 ms rejected-command shake) classes.
 *
 * Pass the environment's composed `prefers-reduced-motion` result; the
 * durations then collapse to `0`.
 */
export function pregameSceneVars(reducedMotion: boolean): SceneVars {
  const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
  return {
    // §2 foundation neutrals — the surfaces panels and rows are built from.
    '--pregame-ink': SCENE_NEUTRALS.ink,
    '--pregame-surface-top': SCENE_NEUTRALS.surfaceTop,
    '--pregame-surface': SCENE_NEUTRALS.surfaceBase,
    '--pregame-raised': SCENE_NEUTRALS.raised,
    '--pregame-line': SCENE_NEUTRALS.lineFaint,
    '--pregame-line-strong': SCENE_NEUTRALS.lineStrong,
    '--pregame-text': SCENE_NEUTRALS.text,
    '--pregame-text-muted': SCENE_NEUTRALS.textMuted,

    // §2 semantic hue families. Gold is the one advance-the-game accent; blue
    // is selection; red/green carry the ribbon's outcome families.
    '--pregame-gold': SCENE_HUES.gold.value,
    '--pregame-blue': SCENE_HUES.blue.value,
    '--pregame-red': SCENE_HUES.red.value,
    '--pregame-green': SCENE_HUES.green.value,

    // §2 seat identity accents, by seat index — worn by roster stripes, crest
    // rings, and directory occupancy pips. Never by text (§5.10).
    '--pregame-seat-0': SCENE_SEAT_ACCENTS[0],
    '--pregame-seat-1': SCENE_SEAT_ACCENTS[1],
    '--pregame-seat-2': SCENE_SEAT_ACCENTS[2],
    '--pregame-seat-3': SCENE_SEAT_ACCENTS[3],
    '--pregame-seat-4': SCENE_SEAT_ACCENTS[4],
    '--pregame-seat-5': SCENE_SEAT_ACCENTS[5],

    // §3 elevation ladder: rows rest, lift on hover/focus, hold when selected;
    // panels and the ready bar sit in screen space.
    '--pregame-elev-rest': SCENE_ELEVATION.rest.shadow,
    '--pregame-elev-lifted': SCENE_ELEVATION.lifted.shadow,
    '--pregame-elev-held': SCENE_ELEVATION.held.shadow,
    '--pregame-elev-screen': SCENE_ELEVATION.screen.shadow,

    // §4 the default environment theme's ambient accent. The BACKDROP itself is
    // no longer assembled here: the shared `table/environment` stack (issue
    // #530) mounts the ADR 0030 layer-1 L0–L3 composition and publishes its own
    // `--env-*` properties, so the pregame and the match cannot drift. This one
    // slot stays because the places' own accents read it.
    '--pregame-glow': theme.glow,

    // §8 motion grammar. `staging` runs the place changes; `micro` runs every
    // lift, selection, relabel, shimmer, and ribbon; `tapUntap` (200 ms) is the
    // rejected-command shake's window.
    '--pregame-motion-staging': `${sceneMotionMs('staging', reducedMotion)}ms`,
    '--pregame-ease-staging': SCENE_MOTION.staging.ease,
    '--pregame-motion-micro': `${sceneMotionMs('micro', reducedMotion)}ms`,
    '--pregame-ease-micro': SCENE_MOTION.micro.ease,
    '--pregame-motion-reject': `${sceneMotionMs('tapUntap', reducedMotion)}ms`,
    '--pregame-ease-reject': SCENE_MOTION.tapUntap.ease,
  };
}

/**
 * A seat's identity accent, indexed by its **room seat number** — the same
 * index the match uses (`SCENE_SEAT_ACCENTS[seat_order.indexOf(player) % n]`,
 * and the server builds `seat_order` in room-seat order), so a seat's color
 * never changes as the game starts (§4.3, criterion 6).
 */
export function seatAccent(seat: number): string {
  const index =
    ((seat % SCENE_SEAT_ACCENTS.length) + SCENE_SEAT_ACCENTS.length) % SCENE_SEAT_ACCENTS.length;
  return SCENE_SEAT_ACCENTS[index]!;
}

/** A seat's accent as the inline custom property the roster/pip CSS reads. */
export function seatAccentVars(seat: number): SceneVars {
  return { '--pregame-accent': seatAccent(seat) };
}
