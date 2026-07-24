import type { EntityId, GameView, PlayerId } from '../../protocol';

/**
 * Resolve the focused opponent at 3+ players (layout-model §Focus model) —
 * exactly one board is focused; the result is `undefined` only when fewer than
 * two opponents exist (no focus concept at two players).
 *
 * Precedence:
 * 1. **Manual focus** — the ephemeral seat the player activated, honored when it
 *    names a current opponent (eliminated seats stay focusable: their public
 *    zones remain browsable).
 * 2. **The first candidate-bearing board** — while a prompt's candidates sit on
 *    an opponent's battlefield, the far side stages the first such seat in seat
 *    order, for context only (candidates pierce every rung, so answering never
 *    *requires* this or any focus change).
 * 3. **Default relevance** — the active opponent during their turn; otherwise
 *    the next non-eliminated opponent in turn order after the active seat (the
 *    receiver, on their own turn).
 */
export function resolveFocusSeat(
  view: GameView,
  opponents: PlayerId[],
  manualFocus: PlayerId | undefined,
  candidates: Set<EntityId>,
): PlayerId | undefined {
  if (opponents.length < 2) return undefined;

  if (manualFocus !== undefined && opponents.includes(manualFocus)) return manualFocus;

  if (candidates.size > 0) {
    const bearing = opponents.find((seat) =>
      view.battlefield.some((p) => p.controller === seat && candidates.has(p.id)),
    );
    if (bearing !== undefined) return bearing;
  }

  const eliminated = new Set(view.opponents.filter((o) => o.eliminated).map((o) => o.player_id));
  const live = opponents.filter((seat) => !eliminated.has(seat));
  const pool = live.length > 0 ? live : opponents;
  if (pool.includes(view.active_player)) return view.active_player;

  // Next in turn order after the active seat, walking the table's seat order.
  const order = view.seat_order.length > 0 ? view.seat_order : [view.you, ...opponents];
  const from = order.indexOf(view.active_player);
  if (from >= 0) {
    for (let step = 1; step <= order.length; step += 1) {
      const seat = order[(from + step) % order.length]!;
      if (pool.includes(seat)) return seat;
    }
  }
  return pool[0];
}
