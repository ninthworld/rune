/**
 * Pure GameView -> presentation-intent adapter (issue #492).
 *
 * This module describes changes that are already authoritative in `current`.
 * It never predicts a rule outcome or keeps a gameplay queue. Structured log
 * entries provide semantic causes where the protocol has them; view diffs
 * provide the documented generic fallback for tap/counter/zone changes.
 *
 * Both passes also credit the acting seat, which feeds the off-focus activity
 * channel (`./offFocusActivity`, issue #501): activity by a seat that is
 * neither the receiver nor the focused opponent earns one quiet crest ping, so
 * a wing or tile seat is never silent.
 */
import type { EntityId, GameLogEntry, GameView, Permanent, PlayerId } from '../../protocol';
import {
  SCENE_BATCH,
  SCENE_HUES,
  SCENE_MOTION,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
} from '../../sceneTokens';
import type {
  EffectQuality,
  EndpointKind,
  PersistentEffect,
  TransientInvocation,
} from '../effects';
import { applyRelationshipEmphasis } from '../relationshipEmphasis';
import type { Rect } from '../scene';
import { offFocusPings, type SeatActivity } from './offFocusActivity';
import { momentAccent, momentBudgetMs, verdictMoment } from './sessionMoments';

/** A generic motion class understood by the DOM plane and screen-space consumers. */
export type GameViewMotionCategory =
  | 'draw'
  | 'play'
  | 'cast'
  | 'zone-travel'
  | 'battlefield-entry'
  | 'tap'
  | 'untap'
  | 'resolve'
  | 'counter'
  | 'fizzle'
  | 'attack'
  | 'block'
  | 'damage'
  | 'heal'
  | 'death'
  | 'counter-change'
  | 'token-batch'
  | 'priority'
  | 'phase'
  | 'turn'
  | 'focus'
  // Session moments (visual-system §8, issue #509): the pre-game mulligan pair
  // and the terminal verdict. They describe events the server already recorded
  // in the log — the client never decides a keep, a bottoming, or a winner.
  | 'mulligan'
  | 'hand-kept'
  | 'verdict';

/** One deterministic, interruptible motion request. */
export interface GameViewMotionIntent {
  /** Stable identity inside this view delta. */
  id: string;
  category: GameViewMotionCategory;
  /** Object moving or receiving feedback, when there is one. */
  entityId?: EntityId;
  /** Live effect-layer source reference (entity, seat, pile, hand, or stack). */
  from?: string;
  /** Live effect-layer destination reference. */
  to?: string;
  /** Duration inside the normative motion-class budget. */
  durationMs: number;
  /** Simultaneous-batch delay, capped by the total presentation window. */
  delayMs: number;
  /** Display magnitude such as damage, life, or counter delta. */
  magnitude?: number;
}

/** A targeting path supplied by the server-driven interaction session (#491). */
export interface TargetingPresentationPath {
  id: string;
  from: EntityId;
  to: EntityId | PlayerId;
  /**
   * Whether this path is the slot the player is answering **right now**
   * (`stack-and-relationships.md` §4.4 `pending`, which dash-crawls) rather than
   * a slot already answered but not yet submitted (`provisional`, whose dashes
   * stand still). Both are dashed; only the crawl separates them, and only the
   * crawl is what reduced motion drops.
   */
  pending?: boolean;
  /**
   * The destination's 1-based place in the action's own requirement order — the
   * §4.5 ordering channel, shared with the prompt's slot progress. Never derived
   * from screen position.
   */
  numeral?: number;
}

/** Ephemeral staging inputs; none are authoritative gameplay state. */
export interface PresentationStaging {
  /** The seat the plane focused for the previous view (staging cue only). */
  previousFocusSeat?: PlayerId;
  /**
   * The focused opponent the plane **resolved** for this view (manual focus or
   * the derived default, `StagedPlane.focusSeat`) — the staging cue's
   * destination, and the seat the off-focus channel excludes.
   */
  focusSeat?: PlayerId;
  targetingPaths?: readonly TargetingPresentationPath[];
  /**
   * The object the player has isolated (selected / focused). Focus isolates one
   * object's relationships and calms the rest (§4.4, §9.3) — the generalisation
   * of the shipped combat-link isolation to every relationship kind. Ephemeral
   * staging, never authoritative.
   */
  isolatedId?: EntityId;
  /** Quality controls batch density, never scene/state fidelity. */
  quality?: EffectQuality;
  /** Reduced motion snaps every batch with no stagger. */
  reducedMotion?: boolean;
}

/** Complete passive presentation work for one authoritative view transition. */
export interface GameViewPresentation {
  motions: GameViewMotionIntent[];
  transients: TransientInvocation[];
  persistent: PersistentEffect[];
  /**
   * The authoritative log entries this transition introduced — the exact
   * entries the motion/transient passes above consumed, exposed once so a
   * secondary channel reads the same stream instead of re-diffing the log.
   *
   * The sound/haptic hooks (`../audio`, issue #507) need it for the session
   * moments the scene deliberately leaves to their own chrome: `game_over` and
   * `player_eliminated` produce no motion, but they are the taxonomy's
   * `victory`/`destroy` cues. Empty on a first mount or a reconnect rebuild,
   * where nothing is replayed.
   */
  events: GameLogEntry[];
}

interface ZoneLocation {
  zone: 'hand' | 'battlefield' | 'stack' | 'graveyard' | 'exile' | 'command';
  seat: PlayerId;
}

const entityRef = (id: EntityId): string => id;
const seatRef = (seat: PlayerId): string => `seat:${seat}`;
const pileRef = (seat: PlayerId): string => `pile:${seat}`;
const handRef = (seat: PlayerId): string => `hand:${seat}`;
const stackRef = (id: EntityId): string => `stack:${id}`;

function seatAccent(view: GameView, seat: PlayerId): string {
  const index = Math.max(0, view.seat_order.indexOf(seat));
  return SCENE_SEAT_ACCENTS[index % SCENE_SEAT_ACCENTS.length]!;
}

function locationAnchor(location: ZoneLocation, id: EntityId): string {
  switch (location.zone) {
    case 'hand':
      return handRef(location.seat);
    case 'battlefield':
      return entityRef(id);
    case 'stack':
      return stackRef(id);
    default:
      return pileRef(location.seat);
  }
}

function visibleLocations(view: GameView): Map<EntityId, ZoneLocation> {
  const locations = new Map<EntityId, ZoneLocation>();
  for (const card of view.my_hand) locations.set(card.id, { zone: 'hand', seat: view.you });
  for (const permanent of view.battlefield) {
    locations.set(permanent.id, { zone: 'battlefield', seat: permanent.controller });
  }
  for (const item of view.stack) {
    locations.set(item.id, { zone: 'stack', seat: item.controller });
  }
  const addPiles = (zone: ZoneLocation['zone'], piles: GameView['graveyards']): void => {
    for (const pile of piles) {
      for (const card of pile.cards) locations.set(card.id, { zone, seat: pile.player_id });
    }
  };
  addPiles('graveyard', view.graveyards);
  addPiles('exile', view.exile);
  addPiles('command', view.command ?? []);
  return locations;
}

function motionDuration(category: GameViewMotionCategory): number {
  // Session moments read their window from the §8 token rows (issue #509). The
  // verdict's window depends on the outcome, so it is passed explicitly.
  if (category === 'mulligan') return momentBudgetMs('mulligan');
  if (category === 'hand-kept') return momentBudgetMs('hand-kept');
  if (category === 'tap' || category === 'untap') return SCENE_MOTION.tapUntap.ms;
  if (category === 'priority' || category === 'phase' || category === 'turn') {
    return SCENE_MOTION.turnFlow.ms;
  }
  if (category === 'focus') return SCENE_MOTION.staging.ms;
  if (
    category === 'damage' ||
    category === 'heal' ||
    category === 'resolve' ||
    category === 'counter' ||
    category === 'fizzle'
  ) {
    return SCENE_MOTION.resolution.ms;
  }
  return SCENE_MOTION.zoneTravel.ms;
}

/** Motion inputs a caller supplies, with an optional per-intent duration. */
type MotionInput = Omit<GameViewMotionIntent, 'id' | 'category' | 'durationMs' | 'delayMs'> & {
  /** Overrides the category default (the verdict window varies by outcome). */
  durationMs?: number;
};

function pushMotion(
  motions: GameViewMotionIntent[],
  category: GameViewMotionCategory,
  key: string,
  input: MotionInput = {},
): void {
  const id = `${category}:${key}`;
  if (motions.some((motion) => motion.id === id)) return;
  const { durationMs, ...rest } = input;
  motions.push({
    id,
    category,
    durationMs: durationMs ?? motionDuration(category),
    delayMs: 0,
    ...rest,
  });
}

function counterTotal(permanent: Permanent): number {
  return (permanent.counters ?? []).reduce((sum, counter) => sum + counter.count, 0);
}

function newLogEntries(previous: GameView, current: GameView): GameLogEntry[] {
  const previousSequence = Math.max(0, ...(previous.log ?? []).map((entry) => entry.sequence));
  return (current.log ?? []).filter((entry) => entry.sequence > previousSequence);
}

function transient(
  transients: TransientInvocation[],
  category: TransientInvocation['category'],
  target: string,
  accent: string,
  magnitude?: number,
): void {
  const invocation: TransientInvocation = {
    category,
    target: { ref: target },
    accent,
    ...(magnitude === undefined ? {} : { magnitude: Math.max(0.5, Math.abs(magnitude)) }),
  };
  const key = JSON.stringify(invocation);
  if (!transients.some((candidate) => JSON.stringify(candidate) === key)) {
    transients.push(invocation);
  }
}

function addLogIntents(
  entries: readonly GameLogEntry[],
  previous: GameView,
  current: GameView,
  motions: GameViewMotionIntent[],
  transients: TransientInvocation[],
  activity: SeatActivity,
): void {
  const previousPermanents = new Map(
    previous.battlefield.map((permanent) => [permanent.id, permanent]),
  );
  for (const { sequence, event } of entries) {
    const key = String(sequence);
    // Credit the acting seat for the off-focus channel. Table-wide flow
    // (step/priority), the loss moments that already land on their own crest
    // (life/damage/elimination), and the session moments (issue #509) are
    // deliberately not seat activity — the ping means "this seat did something
    // over there", not "something happened". The session moments now emit real
    // intents, but each already lands its own cue on the acting seat's crest,
    // so crediting them too would double-pulse one crest in one transition.
    // Off-focus pings from *other* events in the same transition are unaffected
    // and still emit alongside a session moment.
    switch (event.type) {
      case 'spell_cast':
        activity.add(event.player);
        pushMotion(motions, 'cast', key, {
          entityId: event.card.id,
          from: entityRef(event.card.id),
          to: stackRef(event.card.id),
        });
        transient(transients, 'cast', seatRef(event.player), seatAccent(current, event.player));
        break;
      case 'spell_resolved':
        activity.add(event.player);
        pushMotion(motions, 'resolve', key, { entityId: event.card.id });
        transient(
          transients,
          'resolution',
          current.battlefield.some((permanent) => permanent.id === event.card.id)
            ? entityRef(event.card.id)
            : seatRef(event.player),
          SCENE_HUES.gold.value,
        );
        break;
      case 'spell_countered':
      case 'spell_fizzled': {
        const category = event.type === 'spell_countered' ? 'counter' : 'fizzle';
        activity.add(event.player);
        pushMotion(motions, category, key, { entityId: event.card.id });
        // The §6.3 fizzle rule, and it is normative: a countered or fizzled
        // spell's terminal lands on the **stack object**, never on its target.
        // The object has already left the stack in this view, so the anchor
        // resolves through `freezeDepartedEffectAnchors` onto the rect it
        // occupied — which is exactly where the player was looking.
        transient(transients, 'counter', stackRef(event.card.id), SCENE_HUES.red.value);
        break;
      }
      case 'attackers_declared':
        activity.add(event.player);
        for (const attacker of event.attackers) {
          pushMotion(motions, 'attack', `${key}:${attacker.id}`, { entityId: attacker.id });
        }
        break;
      case 'blockers_declared':
        activity.add(event.player);
        for (const { blocker, attacker } of event.blocks) {
          pushMotion(motions, 'block', `${key}:${blocker.id}`, {
            entityId: blocker.id,
            from: entityRef(blocker.id),
            to: entityRef(attacker.id),
          });
        }
        break;
      case 'life_changed': {
        const category = event.amount > 0 ? 'heal' : 'damage';
        pushMotion(motions, category, key, {
          to: seatRef(event.player),
          magnitude: event.amount,
        });
        transient(
          transients,
          event.amount > 0 ? 'healing' : 'damage',
          seatRef(event.player),
          event.amount > 0 ? SCENE_HUES.green.value : SCENE_HUES.red.value,
          event.amount,
        );
        break;
      }
      case 'damage_dealt': {
        const target =
          event.target.kind === 'player'
            ? seatRef(event.target.player)
            : entityRef(event.target.permanent.id);
        pushMotion(motions, 'damage', key, { to: target, magnitude: event.amount });
        transient(transients, 'damage', target, SCENE_HUES.red.value, event.amount);
        break;
      }
      case 'cards_drawn':
        activity.add(event.player);
        pushMotion(motions, 'draw', key, {
          from: pileRef(event.player),
          to: handRef(event.player),
          magnitude: event.count,
        });
        transient(
          transients,
          'draw',
          seatRef(event.player),
          seatAccent(current, event.player),
          event.count,
        );
        break;
      case 'permanent_died': {
        const seat = previousPermanents.get(event.permanent.id)?.controller;
        if (seat !== undefined) activity.add(seat);
        pushMotion(motions, 'death', key, {
          entityId: event.permanent.id,
          from: entityRef(event.permanent.id),
          ...(seat === undefined ? {} : { to: pileRef(seat) }),
        });
        transient(transients, 'death', entityRef(event.permanent.id), SCENE_HUES.red.value);
        break;
      }
      case 'step_changed': {
        const turnChanged = event.turn !== previous.turn;
        pushMotion(motions, turnChanged ? 'turn' : 'phase', key, {
          to: seatRef(event.active_player),
        });
        transient(transients, 'flow', seatRef(event.active_player), SCENE_HUES.gold.value);
        break;
      }
      case 'player_eliminated':
        transient(transients, 'death', seatRef(event.player), SCENE_HUES.red.value);
        break;
      case 'commander_returned_to_command_zone':
        activity.add(event.player);
        pushMotion(motions, 'zone-travel', key, {
          entityId: event.card.id,
          to: pileRef(event.player),
        });
        break;
      // ── Session moments (visual-system §8, issue #509) ───────────────────
      case 'mulligan':
        // "hand sweeps back to library, redraw deals". The redraw arrives as the
        // next view's own hand; this intent carries the sweep, hand → library.
        pushMotion(motions, 'mulligan', key, {
          from: handRef(event.player),
          to: pileRef(event.player),
        });
        transient(transients, 'flow', seatRef(event.player), momentAccent('mulligan'));
        break;
      case 'hand_kept':
        // Keeping resolves the London mulligan: the kept hand settles and the
        // bottomed cards travel to the library. The library is not a visible
        // zone and the wire never names which cards were bottomed, so this
        // aggregate travel — the same shape `cards_drawn` uses — is the honest
        // carrier. Nothing is guessed about the hand's contents.
        pushMotion(motions, 'hand-kept', key, {
          from: handRef(event.player),
          to: pileRef(event.player),
        });
        transient(transients, 'draw', seatRef(event.player), seatAccent(current, event.player));
        break;
      case 'game_over': {
        // The verdict, staged from the receiving seat exactly as the verdict
        // panel phrases it. Restraint is the requirement: victory is the
        // disciplined gold bloom, a loss wears the §2 loss-moment family, and a
        // draw stays neutral. The client formats a decided result — it never
        // decides terminality or a winner.
        const moment = verdictMoment(event.result, current.you);
        const anchor = current.you !== '' ? current.you : event.result.winner;
        pushMotion(motions, 'verdict', key, {
          durationMs: momentBudgetMs(moment),
          ...(anchor === undefined ? {} : { to: seatRef(anchor) }),
        });
        if (anchor !== undefined) {
          transient(
            transients,
            moment === 'victory' ? 'resolution' : moment === 'defeat' ? 'death' : 'flow',
            seatRef(anchor),
            momentAccent(moment),
          );
        }
        break;
      }
    }
  }
}

/**
 * The permanents of one view transition that appear as a **swarm**: two or more
 * that entered the battlefield with no prior visible location and that a player
 * cannot tell apart (same controller, same identity, same type line).
 *
 * This is a presentation read of the view diff, never a rules derivation — the
 * wire carries no token bit, and nothing here decides what a permanent *is*.
 * Identical multiples arriving at once are exactly what the batch-staging
 * budget calls a token swarm, and exactly what the ×N fold then piles up, so
 * they stage as one batch instead of N unrelated arrivals. A lone appearance
 * (a reanimation, a morph flip, an unknown effect) stays a generic
 * `battlefield-entry`.
 */
function swarmEntries(
  before: ReadonlyMap<EntityId, ZoneLocation>,
  current: GameView,
): Set<EntityId> {
  const byIdentity = new Map<string, EntityId[]>();
  for (const permanent of current.battlefield) {
    if (before.has(permanent.id)) continue;
    const identity = permanent.card.functional_id ?? permanent.card.name;
    const key = `${permanent.controller}|${identity}|${permanent.card.type_line}`;
    const group = byIdentity.get(key);
    if (group) group.push(permanent.id);
    else byIdentity.set(key, [permanent.id]);
  }
  const swarm = new Set<EntityId>();
  for (const group of byIdentity.values()) {
    if (group.length > 1) for (const id of group) swarm.add(id);
  }
  return swarm;
}

function addDiffIntents(
  previous: GameView,
  current: GameView,
  motions: GameViewMotionIntent[],
  transients: TransientInvocation[],
  activity: SeatActivity,
): void {
  const before = visibleLocations(previous);
  const after = visibleLocations(current);
  const swarm = swarmEntries(before, current);
  for (const [id, destination] of after) {
    const source = before.get(id);
    if (source?.zone === destination.zone && source.seat === destination.seat) continue;
    // A zone change is its seat's activity at both ends (a card leaving one
    // seat's zone for another's is visible movement on both boards).
    activity.add(destination.seat);
    if (source !== undefined) activity.add(source.seat);
    let category: GameViewMotionCategory = 'zone-travel';
    if (destination.zone === 'hand' && source === undefined) category = 'draw';
    else if (source?.zone === 'hand' && destination.zone === 'stack') category = 'cast';
    else if (source?.zone === 'hand' && destination.zone === 'battlefield') category = 'play';
    else if (destination.zone === 'battlefield' && source === undefined)
      category = swarm.has(id) ? 'token-batch' : 'battlefield-entry';
    pushMotion(motions, category, id, {
      entityId: id,
      ...(source === undefined ? {} : { from: locationAnchor(source, id) }),
      to: locationAnchor(destination, id),
    });
  }

  const previousPermanents = new Map(
    previous.battlefield.map((permanent) => [permanent.id, permanent]),
  );
  for (const permanent of current.battlefield) {
    const prior = previousPermanents.get(permanent.id);
    if (!prior) {
      activity.add(permanent.controller);
      transient(
        transients,
        'battlefield-entry',
        entityRef(permanent.id),
        seatAccent(current, permanent.controller),
      );
      continue;
    }
    if (Boolean(prior.tapped) !== Boolean(permanent.tapped)) {
      activity.add(permanent.controller);
      pushMotion(motions, permanent.tapped ? 'tap' : 'untap', permanent.id, {
        entityId: permanent.id,
      });
    }
    const counterDelta = counterTotal(permanent) - counterTotal(prior);
    const countersChanged =
      JSON.stringify(permanent.counters ?? []) !== JSON.stringify(prior.counters ?? []);
    const powerChanged =
      permanent.card.power !== prior.card.power ||
      permanent.card.toughness !== prior.card.toughness;
    if (countersChanged || powerChanged) {
      activity.add(permanent.controller);
      pushMotion(motions, 'counter-change', permanent.id, {
        entityId: permanent.id,
        magnitude: counterDelta,
      });
      transient(
        transients,
        'counter-change',
        entityRef(permanent.id),
        counterDelta < 0 ? SCENE_HUES.red.value : SCENE_HUES.green.value,
        counterDelta || 1,
      );
    }
    if (!prior.attacking && permanent.attacking) {
      activity.add(permanent.controller);
      pushMotion(motions, 'attack', permanent.id, { entityId: permanent.id });
    }
    if (prior.blocking !== permanent.blocking && permanent.blocking !== undefined) {
      activity.add(permanent.controller);
      pushMotion(motions, 'block', permanent.id, {
        entityId: permanent.id,
        from: entityRef(permanent.id),
        to: entityRef(permanent.blocking),
      });
    }
  }

  if (
    previous.priority_player !== current.priority_player &&
    current.priority_player !== undefined
  ) {
    pushMotion(motions, 'priority', current.priority_player, {
      to: seatRef(current.priority_player),
    });
    transient(transients, 'flow', seatRef(current.priority_player), SCENE_HUES.gold.value);
  }
  if (previous.phase !== current.phase) {
    pushMotion(motions, 'phase', current.phase, {
      to: seatRef(current.active_player),
    });
  }
  if (previous.turn !== current.turn || previous.active_player !== current.active_player) {
    pushMotion(motions, 'turn', String(current.turn), {
      to: seatRef(current.active_player),
    });
  }
}

function applyBatchDelays(motions: GameViewMotionIntent[], staging: PresentationStaging): void {
  const batchable = new Set<GameViewMotionCategory>([
    'untap',
    'battlefield-entry',
    'death',
    'counter-change',
    'token-batch',
    // A multiplayer pre-game resolves several seats at once; the sweeps and
    // settles stage as one batch inside the same 80/800 ms window rather than
    // firing on top of each other.
    'mulligan',
    'hand-kept',
  ]);
  const counters = new Map<GameViewMotionCategory, number>();
  for (const motion of motions) {
    if (!batchable.has(motion.category)) continue;
    if (staging.reducedMotion || staging.quality === 'lite') {
      motion.delayMs = 0;
      continue;
    }
    const index = counters.get(motion.category) ?? 0;
    counters.set(motion.category, index + 1);
    motion.delayMs = Math.min(
      index * SCENE_BATCH.staggerMs,
      Math.max(0, SCENE_BATCH.windowMs - motion.durationMs),
    );
  }
}

/**
 * Classify a relationship's destination so the right §5 endpoint treatment is
 * chosen — a 90° crest arc for a player (D8), an inset reticle for a stack
 * object (§5.5), an open reticle for a permanent (§5.2).
 *
 * The protocol does not type target references (gap **G6**), so this is a
 * **membership test over the view's own server-supplied lists** and nothing
 * more: it never parses text and never infers a kind from a name. Zone
 * destinations (§5.4 / R3) have no protocol representation at all (gap G7) and
 * are therefore specified but dormant — no client-side zone inference exists.
 */
function endpointKindOf(view: GameView, id: EntityId | PlayerId, seats: PlayerId[]): EndpointKind {
  if (seats.includes(id as PlayerId)) return 'player';
  if (view.stack.some((item) => item.id === id)) return 'stack';
  return 'card';
}

function persistentEffects(
  view: GameView,
  targetingPaths: readonly TargetingPresentationPath[],
): PersistentEffect[] {
  const effects: PersistentEffect[] = [];
  const seats =
    view.seat_order.length > 0
      ? view.seat_order
      : [view.you, ...view.opponents.map((o) => o.player_id)];
  for (const permanent of view.battlefield) {
    const duelDefender =
      permanent.attacking && permanent.attacking_player === undefined && seats.length === 2
        ? seats.find((seat) => seat !== permanent.controller)
        : undefined;
    const defender = permanent.attacking_player ?? duelDefender;
    if (permanent.attacking && defender !== undefined) {
      // R7 — an attack is a server-stated fact, so the path is CONFIRMED:
      // solid, static, and therefore free of per-frame cost (§8.4 / IN1).
      effects.push({
        id: `attack:${permanent.id}`,
        category: 'attack-path',
        from: { ref: entityRef(permanent.id) },
        to: { ref: seatRef(defender) },
        accent: SCENE_HUES.orange.value,
        state: 'confirmed',
        endpoint: 'player',
      });
    }
    if (permanent.blocking !== undefined) {
      // R8 — a block is a BIND, not a directed effect: doubled parallel stroke,
      // no lift, and deliberately no arrowhead (decision D7).
      effects.push({
        id: `block:${permanent.id}`,
        category: 'blocker-link',
        from: { ref: entityRef(permanent.id) },
        to: { ref: entityRef(permanent.blocking) },
        accent: SCENE_HUES.orange.value,
        state: 'confirmed',
      });
    }
    if (permanent.attached_to !== undefined) {
      // R9 — attachment is an elbow bracket with symmetric square terminals in
      // neutral line work, never an arc and never a relationship hue (D6). It
      // says "belongs to"; a target path says "acts on".
      effects.push({
        id: `attach:${permanent.id}`,
        category: 'attachment-bracket',
        from: { ref: entityRef(permanent.id) },
        to: { ref: entityRef(permanent.attached_to) },
        accent: SCENE_NEUTRALS.text,
        state: 'confirmed',
      });
    }
  }
  for (const item of view.stack) {
    // R9 — an ability plate's tether back to its source permanent. `source` is
    // the only spell/ability discriminator the wire carries today.
    if (item.source === undefined) continue;
    effects.push({
      id: `tether:${item.id}`,
      category: 'source-tether',
      from: { ref: stackRef(item.id) },
      to: { ref: entityRef(item.source) },
      accent: SCENE_NEUTRALS.text,
      state: 'confirmed',
    });
  }
  for (const path of targetingPaths) {
    const kind = endpointKindOf(view, path.to, seats);
    effects.push({
      id: `target:${path.id}`,
      category: 'targeting-path',
      from: { ref: entityRef(path.from) },
      to: { ref: kind === 'player' ? seatRef(path.to as PlayerId) : entityRef(path.to) },
      accent: SCENE_HUES.orange.value,
      // §4.4 — the slot being answered crawls; an already-answered slot holds
      // its dashes still. Both stay dashed: dashed vs solid is what separates
      // "intended" from "stated", and it survives reduced motion (§7.2).
      state: path.pending === true ? 'pending' : 'provisional',
      endpoint: kind,
      ...(path.numeral === undefined ? {} : { numeral: path.numeral }),
    });
  }
  return effects;
}

/**
 * Derive passive presentation work from two complete authoritative views.
 * `previous === undefined` is reconnect/first mount: render complete with no
 * catch-up animation, while current combat/target paths still reconstruct.
 */
export function deriveGameViewPresentation(
  previous: GameView | undefined,
  current: GameView,
  staging: PresentationStaging = {},
): GameViewPresentation {
  const motions: GameViewMotionIntent[] = [];
  const transients: TransientInvocation[] = [];
  const events = previous === undefined ? [] : newLogEntries(previous, current);
  if (previous !== undefined) {
    const activity: SeatActivity = new Set();
    addLogIntents(events, previous, current, motions, transients, activity);
    addDiffIntents(previous, current, motions, transients, activity);
    if (staging.previousFocusSeat !== staging.focusSeat && staging.focusSeat !== undefined) {
      pushMotion(motions, 'focus', staging.focusSeat, { to: seatRef(staging.focusSeat) });
    }
    // The off-focus channel leads the batch: "never silent" is a guarantee, so
    // a crest ping is never the invocation a dense moment pushes past the
    // quality level's transient cap.
    transients.unshift(
      ...offFocusPings(current, activity, staging.focusSeat, (seat) => seatAccent(current, seat)),
    );
    applyBatchDelays(motions, staging);
  }
  return {
    motions,
    transients,
    persistent: applyRelationshipEmphasis(
      persistentEffects(current, staging.targetingPaths ?? []),
      staging.isolatedId ?? null,
    ),
    events,
  };
}

/**
 * Freeze an effect aimed at a departed object onto its previous visual rect.
 * Current anchors remain live for everything still present.
 */
export function freezeDepartedEffectAnchors(
  invocations: readonly TransientInvocation[],
  previous: ReadonlyMap<string, Rect>,
  current: ReadonlyMap<string, Rect>,
): TransientInvocation[] {
  return invocations.map((invocation) => {
    if ('rect' in invocation.target || current.has(invocation.target.ref)) return invocation;
    const rect = previous.get(invocation.target.ref);
    return rect === undefined ? invocation : { ...invocation, target: { rect } };
  });
}
