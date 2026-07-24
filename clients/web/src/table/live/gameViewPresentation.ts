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
import { SCENE_BATCH, SCENE_HUES, SCENE_MOTION, SCENE_SEAT_ACCENTS } from '../../sceneTokens';
import type { EffectQuality, PersistentEffect, TransientInvocation } from '../effects';
import type { Rect } from '../scene';
import { offFocusPings, type SeatActivity } from './offFocusActivity';

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
  | 'focus';

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

function pushMotion(
  motions: GameViewMotionIntent[],
  category: GameViewMotionCategory,
  key: string,
  input: Omit<GameViewMotionIntent, 'id' | 'category' | 'durationMs' | 'delayMs'> = {},
): void {
  const id = `${category}:${key}`;
  if (motions.some((motion) => motion.id === id)) return;
  motions.push({ id, category, durationMs: motionDuration(category), delayMs: 0, ...input });
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
    // (life/damage/elimination), and the session moments with dedicated chrome
    // are deliberately not seat activity — the ping means "this seat did
    // something over there", not "something happened".
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
        transient(transients, 'counter', seatRef(event.player), SCENE_HUES.orange.value);
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
      case 'game_over':
      case 'mulligan':
      case 'hand_kept':
        // These session moments remain state-first; their dedicated chrome owns
        // the verdict/pregame treatments. No speculative scene motion is added.
        break;
    }
  }
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
      category = 'battlefield-entry';
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
      effects.push({
        id: `attack:${permanent.id}`,
        category: 'attack-path',
        from: { ref: entityRef(permanent.id) },
        to: { ref: seatRef(defender) },
        accent: SCENE_HUES.orange.value,
      });
    }
    if (permanent.blocking !== undefined) {
      effects.push({
        id: `block:${permanent.id}`,
        category: 'blocker-link',
        from: { ref: entityRef(permanent.id) },
        to: { ref: entityRef(permanent.blocking) },
        accent: SCENE_HUES.orange.value,
      });
    }
  }
  for (const path of targetingPaths) {
    const players =
      view.seat_order.length > 0
        ? view.seat_order
        : [view.you, ...view.opponents.map((opponent) => opponent.player_id)];
    effects.push({
      id: `target:${path.id}`,
      category: 'targeting-path',
      from: { ref: entityRef(path.from) },
      to: {
        ref: players.includes(path.to as PlayerId)
          ? seatRef(path.to as PlayerId)
          : entityRef(path.to),
      },
      accent: SCENE_HUES.orange.value,
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
  if (previous !== undefined) {
    const activity: SeatActivity = new Set();
    addLogIntents(
      newLogEntries(previous, current),
      previous,
      current,
      motions,
      transients,
      activity,
    );
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
    persistent: persistentEffects(current, staging.targetingPaths ?? []),
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
