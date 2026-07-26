/**
 * In-match presentation metadata (issue #553): the match **format** signal and the
 * per-seat **commander identity**.
 *
 * Both are public (spectator-safe) and both are server-computed. They exist so the
 * client can render a Commander game's presentation without inferring anything: the
 * command zone, the tax list and the damage tally are all legitimately empty in
 * ordinary Commander states, and a commander's name/colors vanish from `command[]`
 * the moment it is cast.
 */

import type { GameSetupId } from './lobby.js';
import type { PlayerId } from './index.js';

/**
 * Every {@link Color} in WUBRG order — the single source of truth for the closed
 * set, so the union below can never drift from the runtime validator that filters
 * a wire `color_identity`. Mirrors the `COLORS` constant in
 * `crates/rune-protocol/src/presentation.rs`.
 */
export const COLORS = ['W', 'U', 'B', 'R', 'G'] as const;

/** One of Magic's five colors (CR 105.1); one of {@link COLORS}. */
export type Color = (typeof COLORS)[number];

/**
 * The **format** a match is played under (issue #553), carried on every
 * {@link GameView} and {@link SpectatorView}.
 *
 * The authoritative answer to "is this a Commander game?", which no client can
 * infer: a game whose commanders are all on the battlefield has an empty `command`,
 * an elided all-zero `commander_tax`, and an empty `commander_damage`. Absent from
 * the wire means **unknown format, not Commander** — an older server's frames read
 * exactly as they did before this field existed.
 */
export interface MatchFormat {
  /**
   * The room's `game_setup` identifier (e.g. `"standard"`, `"commander"`). Free
   * form — the server's format registry may grow — so the client keys presentation
   * off {@link MatchFormat.commander} and uses this only as a label. Omitted when
   * empty.
   */
  id?: GameSetupId;
  /**
   * Whether this match is played under the Commander rules (CR 903): the typed
   * signal the client keys commander-specific presentation off, rather than
   * string-matching {@link MatchFormat.id} or guessing from zone contents. Omitted
   * (treated as `false`) for every other format.
   */
  commander?: boolean;
}

/**
 * One seat's **commander identity** (CR 903.3/903.4, issue #553): its commander's
 * display name and color identity.
 *
 * Keyed by the owning player's id — the same designation key {@link CommanderDamage}
 * and {@link CommanderTax} use — so the entry is **stable for the whole game** and
 * does not change when the commander is cast, dies, is exiled, or returns to the
 * command zone. That stability is the point: the `command` pile, the only previous
 * source, disappears the instant the commander leaves it, which made a seat's
 * identity gems and nameplate flicker with the commander's location.
 *
 * Public information; server-computed. The client never derives a color identity
 * from a mana cost or a name.
 */
export interface CommanderIdentity {
  /** The commander this describes, named by its owning player's id. */
  commander: PlayerId;
  /** The commander card's display name (CR 903.3). Omitted only for an unresolvable card. */
  name?: string;
  /**
   * The commander's color identity (CR 903.4) in WUBRG order — what the deck was
   * validated against. Omitted for a **colorless** commander, which is a real value
   * (an empty identity), not a missing one.
   */
  color_identity?: Color[];
}
