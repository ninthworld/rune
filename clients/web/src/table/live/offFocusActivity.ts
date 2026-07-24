/**
 * The off-focus activity channel (issue #501).
 *
 * `docs/design/layout-model.md` §Focus model: **"off-focus activity is never
 * silent"** — a wing seat's action fires the quiet crest ping + log entry from
 * the motion grammar. This module owns the *spatial* half of that guarantee:
 * which seats earned a ping for one authoritative view transition, and where it
 * lands. Pacing (stops, dwell, auto-passed steps) is issue #455 and composes
 * with this without overlapping it.
 *
 * Everything here is a pure function of the two views already applied. It adds
 * no gameplay state, predicts nothing, and — like every other presentation
 * intent — is dropped and re-derived on the next view.
 */
import type { GameView, PlayerId } from '../../protocol';
import type { TransientInvocation } from '../effects';

/**
 * The seats credited with activity across one view transition, collected while
 * the adapter walks the new log entries and the view diff. A seat appears once
 * however many intents it earned: the ping is a single batched cue per seat,
 * never a stack of pulses (the ≤800 ms window / ≤80 ms stagger budget).
 */
export type SeatActivity = Set<PlayerId>;

/**
 * The quiet crest pings for one view transition: one per seat that acted and is
 * neither the receiver nor the focused opponent.
 *
 * The anchor is the seat reference `seat:<id>`, which the plane resolves to the
 * seat's crest cluster on wings (at every rung, digest included — the crest can
 * never degrade away) and to its summary tile's mini-crest on compact geometry.
 * One ref therefore satisfies both geometries with no branch here.
 *
 * A duel has no focus concept and no off-focus seat: both boards are staged, so
 * nothing is ever silent and no ping is emitted.
 */
export function offFocusPings(
  view: GameView,
  activity: ReadonlySet<PlayerId>,
  focusSeat: PlayerId | undefined,
  accentOf: (seat: PlayerId) => string,
): TransientInvocation[] {
  if (activity.size === 0) return [];
  const order =
    view.seat_order.length > 0
      ? view.seat_order
      : [view.you, ...view.opponents.map((opponent) => opponent.player_id)];
  const opponents = order.filter((seat) => seat !== view.you);
  if (opponents.length < 2) return [];
  return opponents
    .filter((seat) => seat !== focusSeat && activity.has(seat))
    .map((seat) => ({
      category: 'off-focus-ping' as const,
      target: { ref: `seat:${seat}` },
      accent: accentOf(seat),
    }));
}
