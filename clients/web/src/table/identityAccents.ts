/**
 * Per-player identity accent colors (`docs/design/ui-design-notes.md` §Identity).
 *
 * Each seated player gets one accent, worn by their board region border, their
 * nameplate, and their HUD tile — **never by their cards** (a permanent's frame
 * color is game information; identity and frame are separate channels). At a
 * glance the region answers "whose stuff" and the card answers "what stuff".
 *
 * Assignment is deterministic from the view's explicit `seat_order` (issue #345):
 * every client — including a spectator or a mid-game reconnect — derives the same
 * accent for the same player, with no client state. Views that omit `seat_order`
 * fall back to the receiver-then-opponents order the scene also uses.
 *
 * The palette is chosen to stay clear of the meaning-bearing hues: the WUBRG card
 * frames, selection blue, targeting orange, and the gold playable accent. Color is
 * never the only carrier of identity (names ride every surface), so approximate
 * distinctness at 5+ players is acceptable (ui-requirements §Cards and inspection).
 */
import type { GameView, PlayerId } from '../protocol';

/** The seat-indexed accent cycle (muted jewel tones; see module doc). */
export const IDENTITY_ACCENTS = [
  '#3E9C9C', // teal
  '#C2698F', // rose
  '#7B77C9', // periwinkle
  '#9BA65A', // olive
  '#5E8FA3', // slate cyan
  '#B38467', // clay
  '#8A9E7B', // sage
  '#A97BA1', // heather
] as const;

/** The subset of a view the accent assignment reads (structurally — any object
 * carrying a seat order, a receiver id, and opponent ids qualifies). */
interface SeatedView {
  seat_order: GameView['seat_order'];
  you: GameView['you'];
  opponents: ReadonlyArray<{ player_id: PlayerId }>;
}

/** The stable seat list accents index into. */
function seatList(view: SeatedView): PlayerId[] {
  if (view.seat_order.length > 0) return view.seat_order;
  const seats = view.you ? [view.you] : [];
  return [...seats, ...view.opponents.map((o) => o.player_id)];
}

/**
 * The accent for one player. Unknown ids (an older server, a mid-update race)
 * get the first accent rather than crashing — the accent is presentation only.
 */
export function identityAccent(view: SeatedView, playerId: PlayerId): string {
  const index = seatList(view).indexOf(playerId);
  return IDENTITY_ACCENTS[(index < 0 ? 0 : index) % IDENTITY_ACCENTS.length]!;
}

/** An accent at a given hex alpha (0–255), as a #rrggbbaa color. */
export function accentAlpha(accent: string, alpha: number): string {
  const a = Math.max(0, Math.min(255, Math.round(alpha)))
    .toString(16)
    .padStart(2, '0');
  return `${accent}${a}`;
}
