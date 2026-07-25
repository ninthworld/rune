/**
 * `GameView` → one seat's identity facts, the binding data-source column of
 * `docs/design/seat-identity.md` §10 made executable (issue #532).
 *
 * Every value below maps to a field that exists in `rune-protocol` today. What
 * does not exist is **not derived**: §11's standing prohibition is honoured by
 * construction, so this module never parses a number out of `statuses`, never
 * counts `log[]` entries, never infers a threshold, and never fills a dormant
 * slot with a zero or an "unknown".
 *
 * The dormant set, and the issue that lights each one up:
 *
 * | Slot | Needs | Issue |
 * | --- | --- | --- |
 * | Poison chip, tray Poison/Counters segments | structured `PlayerCounter[]` | #544 |
 * | The receiver's own named statuses | `statuses` on `SelfView` | #544 |
 * | Status labels/descriptions in the popover | `PlayerStatus.label` | #544 |
 * | Broken-link (disconnected) glyph | a per-seat connection flag | #553 |
 * | The **local** seat's eliminated treatment | `eliminated` on `SelfView` | #553 |
 * | A colour identity that survives the command zone | per-player identity | #553 |
 * | The commander's name on the nameplate | as above | #553 |
 * | The AI-seat marker | `ai` carried into `GameView` | #553 |
 *
 * Two rankings happen here and neither is game logic: the **worst** incoming
 * commander damage is the maximum of a server-provided list (§5.4), and the
 * attacker count is a filter over `battlefield[]` (§6.3). No legality, cost,
 * effect, or terminality is computed — 21 stays the server's call through
 * `result.reason`.
 */
import type { CommanderDamage, GameView, PlayerId } from '../protocol';
import type { ColorIdentity } from '../tokens';
import { SCENE_SEAT_ACCENTS } from '../sceneTokens';
import { deriveColorIdentity } from './colorIdentity';
import { portraitFor } from './seatPortraits';
import type { SeatClusterFacts } from './plane/cluster';

/** The stable seat list accents and portraits index into (`seat_order`, #345). */
export function seatOrderOf(view: GameView): PlayerId[] {
  if (view.seat_order.length > 0) return view.seat_order;
  const seats = view.you ? [view.you] : [];
  return [...seats, ...view.opponents.map((o) => o.player_id)];
}

/**
 * The seat's accent from `sceneTokens` (visual-system §2), indexed by its
 * `seat_order` position so every client — spectator or mid-game reconnect —
 * derives the same colour with no client state.
 *
 * `table/identityAccents.ts` still ships a different eight-value cycle for the
 * legacy panels; the two palettes disagree, which `seat-identity.md` §13
 * conflict 2 records as needing one owner. The scene-side surfaces read the
 * scene token, per this client's "all scene values through `sceneTokens.ts`".
 */
export function seatAccent(seatOrder: readonly PlayerId[], seat: PlayerId): string {
  const index = seatOrder.indexOf(seat);
  return SCENE_SEAT_ACCENTS[(index < 0 ? 0 : index) % SCENE_SEAT_ACCENTS.length]!;
}

/**
 * The nameplate's label (§7's "no name" row). The shared `playerName` falls back
 * to the raw opaque id, which is right for a log line and wrong for a nameplate:
 * §7 requires `Seat N` from the seat's 1-based `seat_order` index — never blank,
 * never a raw `p{N}`. The id is still never *parsed*; only its position is read.
 *
 * The receiver's "(you)" marker is deliberately NOT part of it: §2's local row
 * reads "`You` or name", the local cluster is unambiguous where it sits, and the
 * suffix would spend four of the plate's thirteen graphemes. It rides the
 * accessible sentence instead, where it costs nothing.
 */
export function seatNameplateLabel(
  view: GameView,
  seatOrder: readonly PlayerId[],
  seat: PlayerId,
): string {
  const name = view.player_names[seat];
  if (name !== undefined && name.length > 0) return name;
  return `Seat ${Math.max(0, seatOrder.indexOf(seat)) + 1}`;
}

/**
 * The seat's colour identity, derived from the cards in its **command zone**.
 *
 * `undefined` — the gem is not drawn — whenever the command zone does not name
 * this seat, which is exactly the §11 dormancy rule: there is no per-player
 * colour-identity field, so the gem is honestly absent rather than guessed from
 * the battlefield or held over from an earlier view (#553).
 */
export function seatColorIdentity(view: GameView, seat: PlayerId): ColorIdentity | undefined {
  const cards = (view.command ?? []).find((pile) => pile.player_id === seat)?.cards ?? [];
  if (cards.length === 0) return undefined;
  const identities = new Set(cards.map((card) => deriveColorIdentity(card)));
  if (identities.size === 1) {
    const [only] = identities;
    return only;
  }
  return 'M';
}

/** The single worst incoming commander damage (§5.4), when at least one is ≥ 1. */
export function worstCommanderDamage(
  damage: readonly CommanderDamage[],
  seat: PlayerId,
): { amount: number; from: PlayerId } | undefined {
  let worst: CommanderDamage | undefined;
  for (const entry of damage) {
    if (entry.damaged !== seat || entry.amount < 1) continue;
    if (worst === undefined || entry.amount > worst.amount) worst = entry;
  }
  return worst === undefined ? undefined : { amount: worst.amount, from: worst.commander };
}

/** How many permanents are attacking one seat (§6.3 — a filter, not combat). */
export function attackerCountOn(
  view: GameView,
  seat: PlayerId,
  receiver: PlayerId | undefined,
  duel: boolean,
): number {
  let count = 0;
  for (const perm of view.battlefield) {
    if (perm.attacking_player !== undefined) {
      if (perm.attacking_player === seat) count += 1;
      continue;
    }
    // The documented two-player fallback (§10.3): older servers omit
    // `attacking_player` because the sole opponent is the only legal defender.
    if (!duel || perm.attacking !== true) continue;
    const defender = perm.controller === receiver ? view.opponents[0]?.player_id : receiver;
    if (defender === seat) count += 1;
  }
  return count;
}

/** What {@link seatIdentityFacts} needs beyond the view itself. */
export interface SeatIdentityRequest {
  /** The seat. */
  seat: PlayerId;
  /** Life total, already read from `me` or the matching `OpponentView`. */
  life: number;
  /** Visible hand count. */
  handCount: number;
  /** Library count, for the accessible sentence (the pile owns the badge). */
  libraryCount: number;
  /** Whether this seat is the receiver. */
  local: boolean;
  /** Whether the seat has been eliminated (opponents only — §11, #553). */
  eliminated: boolean;
  /** Whether the seat holds priority. */
  priority: boolean;
  /** Whether the seat is the active player. */
  active: boolean;
  /** Whether the seat is the focused opponent. */
  focused: boolean;
  /** Whether any attacker is attacking the seat. */
  attacked: boolean;
  /** How many attackers (already filtered by the stage). */
  attackedCount: number;
  /**
   * Whether the seat's command-zone pile is **not** drawn at this rung. §5.3
   * puts the commander tax on the pile, and lets the cluster duplicate it only
   * where the pile has collapsed out of the rung.
   */
  commandPileHidden: boolean;
}

/** Assemble one seat's displayed facts (§10's binding column, in full). */
export function seatIdentityFacts(view: GameView, request: SeatIdentityRequest): SeatClusterFacts {
  const seatOrder = seatOrderOf(view);
  const opponent = view.opponents.find((entry) => entry.player_id === request.seat);
  const taxEntry = (view.commander_tax ?? []).find((tax) => tax.commander === request.seat);
  const commandPile = (view.command ?? []).find((pile) => pile.player_id === request.seat);
  return {
    label: seatNameplateLabel(view, seatOrder, request.seat),
    local: request.local,
    life: request.life,
    handCount: request.handCount,
    libraryCount: request.libraryCount,
    gem: seatColorIdentity(view, request.seat),
    commanderPresent: taxEntry !== undefined || (commandPile?.cards.length ?? 0) > 0,
    commanderTax:
      request.commandPileHidden && taxEntry?.tax !== undefined ? taxEntry.tax : undefined,
    commanderDamage: worstCommanderDamage(view.commander_damage, request.seat),
    // Free-form display text, opponents only: `SelfView` carries no `statuses`
    // (§11, issue #544). The array order is the server's and is kept verbatim.
    statuses: request.local ? [] : (opponent?.statuses ?? []),
    attackedCount: request.attackedCount,
    autoPassed: request.local && view.auto_passed === true,
    deadline: request.local && view.action_deadline !== undefined,
    portrait: portraitFor(request.seat, seatOrder, request.local),
    accent: seatAccent(seatOrder, request.seat),
    eliminated: request.eliminated,
    priority: request.priority,
    active: request.active,
    focused: request.focused,
    attacked: request.attacked,
  };
}
