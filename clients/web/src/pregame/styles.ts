/**
 * The pregame style barrel (issue #506).
 *
 * The pregame stylesheet is split in two along the one seam that does not cut a
 * rule in half — the shared stage and its surface/control vocabulary
 * (`pregame.module.css`) versus the places that speak it
 * (`pregamePlaces.module.css`) — so neither file runs past the AGENTS.md file-size
 * ceiling. The two class sets are disjoint, so the components see one flat map
 * and one import, exactly as they would from a single module.
 *
 * ADR 0019 discipline is unchanged: values live in `sceneTokens.ts` and reach
 * CSS as the `--pregame-*` properties `pregameScene.ts` publishes.
 */
import base from './pregame.module.css';
import places from './pregamePlaces.module.css';

const styles: { readonly [key: string]: string } = { ...base, ...places };

export default styles;
