import { normalizeGameView } from '../wire';
import type { CardView, GameView } from '../protocol';

/** How one seat is described to {@link clusterTable}. */
export interface ClusterSeatSpec {
  /** Player id. */
  id: string;
  /** Display name (absent ⇒ `player_names` has no entry, so `Seat N` applies). */
  name?: string;
  /** Life total. */
  life?: number;
  /** Hand size (the receiver's becomes `my_hand.length`). */
  hand?: number;
  /** Library size. */
  library?: number;
  /** Free-form status strings, verbatim (opponents only on the wire). */
  statuses?: string[];
  /** Whether the seat is eliminated (opponents only on the wire). */
  eliminated?: boolean;
  /** Mana costs of the cards in this seat's command zone (drives the gem). */
  command?: string[];
}

/** Everything the seat-identity tests vary about a view. */
export interface ClusterTableSpec {
  /** The seats, receiver first unless {@link you} says otherwise. */
  seats: ClusterSeatSpec[];
  /** The receiver's id; omit for a receiver-less (spectator-shaped) view. */
  you?: string;
  /** `seat_order`, defaulting to the seats in the order given. */
  seatOrder?: string[];
  /** The active player. */
  active?: string;
  /** The priority holder. */
  priority?: string;
  /** Attacks as `[attacker's controller, defending seat]` pairs. */
  attacks?: [string, string][];
  /** Commander damage tallies as `[commander, damaged, amount]`. */
  commanderDamage?: [string, string, number][];
  /** Commander tax as `[commander, tax]`. */
  commanderTax?: [string, number][];
  /** `auto_passed` on this view. */
  autoPassed?: boolean;
  /** `action_deadline` on this view. */
  deadline?: number;
}

function card(id: string, cost: string): CardView {
  return { id, name: `Commander ${id}`, type_line: 'Legendary Creature — Avatar', mana_cost: cost };
}

/**
 * A normalized view built for the seat-identity tests: names, life, hands,
 * libraries, statuses, elimination, command zones, commander damage and tax,
 * attacks, priority, and the two receiver-only transients — everything
 * `docs/design/seat-identity.md` §10 names as an existing field, and nothing
 * else. The staging fixture (`plane.fixture.ts`) covers board contents; this one
 * covers player state.
 */
export function clusterTable(spec: ClusterTableSpec): GameView {
  const you = spec.you ?? spec.seats[0]?.id;
  const opponents = spec.seats.filter((seat) => seat.id !== you);
  const local = spec.seats.find((seat) => seat.id === you);
  const names: Record<string, string> = {};
  for (const seat of spec.seats) if (seat.name !== undefined) names[seat.id] = seat.name;
  const command = spec.seats
    .filter((seat) => (seat.command ?? []).length > 0)
    .map((seat) => ({
      player_id: seat.id,
      cards: (seat.command ?? []).map((cost, i) => card(`${seat.id}_cmd_${i}`, cost)),
    }));
  return normalizeGameView({
    ...(you === undefined ? {} : { you }),
    my_hand: Array.from({ length: local?.hand ?? 0 }, (_, i) => card(`${you}_hand_${i}`, '{1}')),
    me: { life: local?.life ?? 20, library_size: local?.library ?? 40 },
    opponents: opponents.map((seat) => ({
      player_id: seat.id,
      hand_size: seat.hand ?? 3,
      life: seat.life ?? 20,
      library_size: seat.library ?? 40,
      graveyard_size: 0,
      ...(seat.statuses ? { statuses: seat.statuses } : {}),
      ...(seat.eliminated ? { eliminated: true } : {}),
    })),
    battlefield: (spec.attacks ?? []).map(([controller, defender], i) => ({
      id: `atk_${i}`,
      controller,
      owner: controller,
      attacking: true,
      attacking_player: defender,
      card: { id: `atk_${i}`, name: 'Attacker', type_line: 'Creature — Soldier' },
    })),
    ...(command.length > 0 ? { command } : {}),
    commander_damage: (spec.commanderDamage ?? []).map(([commander, damaged, amount]) => ({
      commander,
      damaged,
      amount,
    })),
    ...(spec.commanderTax
      ? { commander_tax: spec.commanderTax.map(([commander, tax]) => ({ commander, tax })) }
      : {}),
    player_names: names,
    phase: 'precombat_main',
    active_player: spec.active ?? spec.seats[0]?.id,
    priority_player: spec.priority ?? spec.seats[0]?.id,
    seat_order: spec.seatOrder ?? spec.seats.map((seat) => seat.id),
    valid_actions: [],
    ...(spec.autoPassed ? { auto_passed: true } : {}),
    ...(spec.deadline !== undefined ? { action_deadline: spec.deadline } : {}),
  });
}
