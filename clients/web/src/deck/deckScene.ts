/**
 * Scene-token wiring for the deck builder and lobby deck-selection surfaces
 * (issue #508). These pre-game surfaces now speak the same 2.5D visual language as
 * the match: the scene palette (`sceneTokens.ts`), the elevation ladder (§3), and
 * the motion grammar (§8) reach the CSS-module layer as `--deck-*` custom
 * properties assembled here — the ADR 0019 pattern the DOM card face established.
 *
 * The reduced-motion collapse is wired at the token level (the carried contract):
 * every motion duration is resolved through {@link sceneMotionMs}, so under reduced
 * motion the durations are `0` and every add/remove/designate transition snaps to
 * its end state — matching the global neutralizer in `chrome/base.css`. No game
 * logic, no I/O — a pure style-vars builder.
 */
import type { CSSProperties } from 'react';
import {
  SCENE_ELEVATION,
  SCENE_HUES,
  SCENE_MOTION,
  SCENE_NEUTRALS,
  sceneMotionMs,
} from '../sceneTokens';
import type { CardFaceArt } from '../card/dom';
import { textureForArtKey } from '../card/art/artStore';

/** Custom-property style object usable as an inline `style`. */
type SceneVars = CSSProperties & Record<`--${string}`, string | number>;

/**
 * The scene-token CSS custom properties the deck surfaces render through: the dark
 * table-world neutrals, the disciplined interaction hues, the elevation-ladder
 * shadows (rest → lifted → held), and the motion-grammar durations/easings for the
 * zone-travel (add/remove) and micro (hover/press) classes. Pass the environment's
 * `prefers-reduced-motion` result; the durations then collapse to `0`.
 */
export function deckSceneVars(reducedMotion: boolean): SceneVars {
  return {
    '--deck-ink': SCENE_NEUTRALS.ink,
    '--deck-surface-top': SCENE_NEUTRALS.surfaceTop,
    '--deck-surface': SCENE_NEUTRALS.surfaceBase,
    '--deck-raised': SCENE_NEUTRALS.raised,
    '--deck-line': SCENE_NEUTRALS.lineFaint,
    '--deck-line-strong': SCENE_NEUTRALS.lineStrong,
    '--deck-text': SCENE_NEUTRALS.text,
    '--deck-gold': SCENE_HUES.gold.value,
    '--deck-blue': SCENE_HUES.blue.value,
    '--deck-red': SCENE_HUES.red.value,
    '--deck-elev-rest': SCENE_ELEVATION.rest.shadow,
    '--deck-elev-lifted': SCENE_ELEVATION.lifted.shadow,
    '--deck-elev-held': SCENE_ELEVATION.held.shadow,
    '--deck-motion-zone': `${sceneMotionMs('zoneTravel', reducedMotion)}ms`,
    '--deck-ease-zone': SCENE_MOTION.zoneTravel.ease,
    '--deck-motion-micro': `${sceneMotionMs('micro', reducedMotion)}ms`,
    '--deck-ease-micro': SCENE_MOTION.micro.ease,
  };
}

/**
 * Resolve already-loaded player-side art (ADR 0024) for a card face without ever
 * fetching — the renderer only *looks up* a published texture. Absent renders the
 * procedural face, so the builder is fully usable with the art store empty.
 */
export function deckCardArt(artKey: string | undefined): CardFaceArt | undefined {
  const published = textureForArtKey(artKey);
  return published ? { url: published.url, full: published.full } : undefined;
}
