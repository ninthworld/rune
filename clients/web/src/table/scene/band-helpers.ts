import type { GameView, PlayerId } from '../../protocol';
import { playerName } from '../../playerNames';
import { deriveColorIdentity } from '../colorIdentity';
import type { ZoneCounts } from './types';

/**
 * The receiver's own seat id, taken straight from `view.you`. An older server
 * may send it empty; treat that as "unknown" (`undefined`) so band ordering and
 * `isLocal` degrade the same way they did before the field existed.
 */
export function localPlayerIdOf(view: GameView): PlayerId | undefined {
  return view.you || undefined;
}

/**
 * The opponents in the table's stable **seat order** (`view.seat_order`, issue
 * #345), excluding the receiver — the exact order the scene lays opponent panels
 * and the shell carves opponent frames, so an index into this list addresses one
 * opponent frame. Falls back to `view.opponents` order where the server sent no
 * seat order, and appends any opponent the seat order omits so none is dropped.
 * Shared by the scene builder and the table's focus/expansion mapping (issue #400).
 */
export function orderedOpponentIds(view: GameView): PlayerId[] {
  const localPlayerId = localPlayerIdOf(view);
  const opponentIds = view.opponents.map((o) => o.player_id);
  const opponentSet = new Set(opponentIds);
  const seatOrderOpponents = view.seat_order.filter(
    (id) => id !== localPlayerId && opponentSet.has(id),
  );
  return seatOrderOpponents.length > 0
    ? [...seatOrderOpponents, ...opponentIds.filter((id) => !seatOrderOpponents.includes(id))]
    : opponentIds;
}

/**
 * The band's display label. Names the *controller* of the permanents (zone
 * placement follows control, ui-requirements §2) by their **display name** (issue
 * #294 — players are people, never seat ids, §Identity), falling back to the raw
 * id only when the server sent no name. The local band is marked "(you)" so a
 * newcomer can tell their area from the opponent's.
 */
export function bandLabel(view: GameView, playerId: PlayerId, isLocal: boolean): string {
  const name = playerName(view, playerId);
  return isLocal ? `${name} (you)` : name;
}

/**
 * A controller's library/graveyard/exile pile counts, read straight from the view
 * (the same fields the player tiles show). The local library comes from `me`;
 * an opponent's from its redacted `OpponentView`. Missing piles count as zero.
 */
export function zoneCountsOf(view: GameView, playerId: PlayerId, isLocal: boolean): ZoneCounts {
  const library = isLocal
    ? view.me.library_size
    : (view.opponents.find((o) => o.player_id === playerId)?.library_size ?? 0);
  const graveyardCards = view.graveyards.find((g) => g.player_id === playerId)?.cards ?? [];
  const exile = view.exile.find((e) => e.player_id === playerId)?.cards.length ?? 0;
  const command = (view.command ?? []).find((c) => c.player_id === playerId)?.cards.length ?? 0;
  const topCard = graveyardCards.length > 0 ? graveyardCards[graveyardCards.length - 1] : undefined;
  return {
    library,
    graveyard: graveyardCards.length,
    exile,
    command,
    graveyardTop: topCard
      ? { name: topCard.name, colorIdentity: deriveColorIdentity(topCard) }
      : undefined,
  };
}
