/**
 * GameView → table scene mapping.
 *
 * A **pure** function that turns the store's latest {@link GameView} into the set
 * of rendered entities (battlefield bands per controller + the local hand), each
 * carrying the {@link CardDisplayData} the Pixi factory draws, a layout `rect`,
 * and the `valid_actions` that belong to it (ADR 0004 subject routing). Keeping
 * this pure and headless makes the whole GameView→scene mapping unit-testable
 * without a WebGL context — the React/Pixi layers only position what it returns.
 *
 * No game logic lives here: characteristics (P/T, counters, tapped) are passed
 * through exactly as the server computed them, and interactivity is derived
 * solely from `valid_actions[]`.
 */
import type {
  CardView,
  Counter,
  EntityId,
  GameView,
  Permanent,
  PlayerId,
  ValidAction,
} from '../protocol';
import type { CardDisplayData, CardTier } from '../card/cardFactory';
import { TIER } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';

/** Absolute placement of a card within the scene's logical coordinate space. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One card the scene renders, with its factory data, position, and actions. */
export interface RenderedCard {
  /** Entity id (permanent or hand card). */
  entityId: EntityId;
  /** Which zone it lives in — drives its tier and layout row. */
  zone: 'battlefield' | 'hand';
  /** Size tier passed to the card factory. */
  tier: CardTier;
  /** Convenience copy of the display name for labels/aria. */
  name: string;
  /** Everything the Pixi factory needs to draw the card. */
  data: CardDisplayData;
  /** Logical position of the card. */
  rect: Rect;
  /** The subject-actions bound to this entity (empty ⇒ not interactive). */
  actions: ValidAction[];
}

/** A per-controller battlefield row. */
export interface Band {
  /** Controller of the permanents in this band. */
  playerId: PlayerId;
  /** Whether this is the local player's band (rendered nearest the hand). */
  isLocal: boolean;
  /** The rendered permanents, in server order. */
  cards: RenderedCard[];
}

/** The full scene: opponents' bands (top), the local band, and the hand. */
export interface TableScene {
  /** Logical width the canvas + DOM overlay share. */
  width: number;
  /** Logical height the canvas + DOM overlay share. */
  height: number;
  /** Battlefield bands, opponents first and the local player last. */
  bands: Band[];
  /** The local player's hand. */
  hand: RenderedCard[];
  /** The resolved local player id, if it could be identified. */
  localPlayerId?: PlayerId;
}

/** Layout geometry (logical px). Card sizes come from the shared TIER tokens. */
const LAYOUT = {
  margin: 16,
  cardGap: 12,
  bandGap: 18,
  handGap: 28,
} as const;

/**
 * Identify which player is "me". The GameView never names the receiver, but the
 * hidden-zone `opponents[]` list does name everyone else, so the local player is
 * whichever id appears in a public zone yet is not an opponent. Pure display
 * inference — it selects a band to render nearest the hand, nothing more.
 */
export function deriveLocalPlayerId(view: GameView): PlayerId | undefined {
  const opponents = new Set(view.opponents.map((o) => o.player_id));
  const candidates: PlayerId[] = [];
  for (const perm of view.battlefield) candidates.push(perm.controller, perm.owner);
  for (const pile of view.graveyards) candidates.push(pile.player_id);
  for (const pile of view.exile) candidates.push(pile.player_id);
  if (view.priority_player) candidates.push(view.priority_player);
  return candidates.find((id) => !opponents.has(id));
}

/** Map a server card + permanent state onto the factory's display data. */
function toDisplayData(
  card: CardView,
  opts: { tapped?: boolean; counters?: Counter[]; selected: boolean },
): CardDisplayData {
  return {
    name: card.name,
    typeLine: card.type_line,
    colorIdentity: deriveColorIdentity(card),
    manaCost: card.mana_cost,
    // P/T and counters are rendered verbatim — the server sends effective values.
    power: card.power,
    toughness: card.toughness,
    counters: opts.counters,
    tapped: opts.tapped,
    selected: opts.selected,
  };
}

/** The subject-actions from `valid_actions[]` that name a given entity. */
function actionsFor(entityId: EntityId, actions: ValidAction[]): ValidAction[] {
  return actions.filter((a) => a.subject?.includes(entityId));
}

/** Lay a row of same-tier cards out left-to-right, returning them + row width. */
function layRow(
  cards: Omit<RenderedCard, 'rect'>[],
  top: number,
  cardW: number,
): { placed: RenderedCard[]; width: number } {
  const placed = cards.map((card, i) => ({
    ...card,
    rect: {
      x: LAYOUT.margin + i * (cardW + LAYOUT.cardGap),
      y: top,
      w: cardW,
      h: TIER[card.tier].h,
    },
  }));
  const width =
    cards.length === 0
      ? 0
      : LAYOUT.margin * 2 + cards.length * cardW + (cards.length - 1) * LAYOUT.cardGap;
  return { placed, width };
}

/**
 * Build the full scene from a view. `selectedId` marks the currently selected
 * entity so its card draws a selection ring; it never changes what is offered.
 */
export function buildTableScene(
  view: GameView,
  localPlayerId?: PlayerId,
  selectedId?: EntityId,
): TableScene {
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);

  // Group battlefield permanents by controller (zone placement follows the
  // controller, not the owner — Control-Magic-safe, per ui-requirements §2).
  const byController = new Map<PlayerId, Permanent[]>();
  for (const perm of view.battlefield) {
    const list = byController.get(perm.controller) ?? [];
    list.push(perm);
    byController.set(perm.controller, list);
  }

  // Band order: each opponent (top), any other controller, then the local band.
  const opponentIds = view.opponents.map((o) => o.player_id);
  const ordered: PlayerId[] = [...opponentIds];
  for (const controller of byController.keys()) {
    if (!ordered.includes(controller) && controller !== localPlayerId) ordered.push(controller);
  }
  if (localPlayerId !== undefined) ordered.push(localPlayerId);

  const toRenderable = (perm: Permanent): Omit<RenderedCard, 'rect'> => ({
    entityId: perm.id,
    zone: 'battlefield',
    tier: 'field',
    name: perm.card.name,
    data: toDisplayData(perm.card, {
      tapped: perm.tapped,
      counters: perm.counters,
      selected: perm.id === selectedId,
    }),
    actions: actionsFor(perm.id, subjectActions),
  });

  const bands: Band[] = [];
  let top = LAYOUT.margin;
  let maxWidth = LAYOUT.margin * 2;
  const fieldW = TIER.field.w;

  for (const playerId of ordered) {
    const perms = byController.get(playerId) ?? [];
    const { placed, width } = layRow(perms.map(toRenderable), top, fieldW);
    bands.push({ playerId, isLocal: playerId === localPlayerId, cards: placed });
    maxWidth = Math.max(maxWidth, width);
    top += TIER.field.h + LAYOUT.bandGap;
  }

  // Hand row along the bottom, in the larger hand tier.
  top += LAYOUT.handGap - LAYOUT.bandGap;
  const handW = TIER.hand.w;
  const { placed: hand, width: handWidth } = layRow(
    view.my_hand.map((card) => ({
      entityId: card.id,
      zone: 'hand' as const,
      tier: 'hand' as const,
      name: card.name,
      data: toDisplayData(card, { selected: card.id === selectedId }),
      actions: actionsFor(card.id, subjectActions),
    })),
    top,
    handW,
  );
  maxWidth = Math.max(maxWidth, handWidth);
  const height = top + TIER.hand.h + LAYOUT.margin;

  return { width: maxWidth, height, bands, hand, localPlayerId };
}
