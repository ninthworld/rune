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
  rowGap: 10,
  bandGap: 18,
  handGap: 28,
} as const;

/**
 * Default logical width the layout wraps within when the caller passes none.
 * Bands wrap to as many rows as needed to stay inside this budget, so a large
 * board grows downward rather than off the right edge (no horizontal page scroll,
 * ui-requirements §11 / brief "Dynamic Card Sizing"). Callers that know the real
 * viewport width (e.g. a resize-aware `Table`) pass it through for responsiveness.
 */
export const DEFAULT_VIEWPORT_WIDTH = 1280;

/**
 * The receiver's own seat id, taken straight from `view.you`. An older server
 * may send it empty; treat that as "unknown" (`undefined`) so band ordering and
 * `isLocal` degrade the same way they did before the field existed.
 */
function localPlayerIdOf(view: GameView): PlayerId | undefined {
  return view.you || undefined;
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

/**
 * How many same-width cards fit in one row inside `availWidth` (always ≥ 1, so a
 * single card never vanishes even in an absurdly narrow viewport). Pure integer
 * math over the shared gap/margin tokens — the unit under test for wrapping.
 */
function cardsPerRow(cardW: number, availWidth: number): number {
  const usable = availWidth - LAYOUT.margin * 2;
  return Math.max(1, Math.floor((usable + LAYOUT.cardGap) / (cardW + LAYOUT.cardGap)));
}

/**
 * Lay a band of same-tier cards into as many rows as fit within `availWidth`,
 * wrapping left-to-right then top-to-bottom. Returns the placed cards, the widest
 * row's width, and the total height the band occupies. An empty band still
 * reserves one card-height row so its (possibly local) slot stays visible.
 *
 * This is the pure wrapping math the whole feature turns on: bounding row width to
 * the viewport is what keeps a 100-permanent board from growing a horizontal
 * scrollbar — it grows downward instead.
 */
function layBand(
  cards: Omit<RenderedCard, 'rect'>[],
  top: number,
  cardW: number,
  cardH: number,
  availWidth: number,
): { placed: RenderedCard[]; width: number; height: number } {
  if (cards.length === 0) return { placed: [], width: 0, height: cardH };
  const perRow = cardsPerRow(cardW, availWidth);
  const placed = cards.map((card, i) => {
    const col = i % perRow;
    const row = Math.floor(i / perRow);
    return {
      ...card,
      rect: {
        x: LAYOUT.margin + col * (cardW + LAYOUT.cardGap),
        y: top + row * (cardH + LAYOUT.rowGap),
        w: cardW,
        h: cardH,
      },
    };
  });
  const rows = Math.ceil(cards.length / perRow);
  const cols = Math.min(perRow, cards.length);
  const width = LAYOUT.margin * 2 + cols * cardW + (cols - 1) * LAYOUT.cardGap;
  const height = rows * cardH + (rows - 1) * LAYOUT.rowGap;
  return { placed, width, height };
}

/**
 * Build the full scene from a view. `selectedId` marks the currently selected
 * entity so its card draws a selection ring; it never changes what is offered.
 * `viewportWidth` is the logical width budget bands wrap within (defaults to
 * {@link DEFAULT_VIEWPORT_WIDTH}); the returned `width` never exceeds it beyond a
 * single card, so the board never needs a horizontal page scrollbar.
 */
export function buildTableScene(
  view: GameView,
  selectedId?: EntityId,
  viewportWidth: number = DEFAULT_VIEWPORT_WIDTH,
): TableScene {
  const localPlayerId = localPlayerIdOf(view);
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
  const fieldT = TIER.field;

  for (const playerId of ordered) {
    const perms = byController.get(playerId) ?? [];
    const { placed, width, height } = layBand(
      perms.map(toRenderable),
      top,
      fieldT.w,
      fieldT.h,
      viewportWidth,
    );
    bands.push({ playerId, isLocal: playerId === localPlayerId, cards: placed });
    maxWidth = Math.max(maxWidth, width);
    top += height + LAYOUT.bandGap;
  }

  // Hand along the bottom, in the larger hand tier — wrapping the same way so a
  // big hand also grows downward instead of off the right edge.
  top += LAYOUT.handGap - LAYOUT.bandGap;
  const handT = TIER.hand;
  const {
    placed: hand,
    width: handWidth,
    height: handHeight,
  } = layBand(
    view.my_hand.map((card) => ({
      entityId: card.id,
      zone: 'hand' as const,
      tier: 'hand' as const,
      name: card.name,
      data: toDisplayData(card, { selected: card.id === selectedId }),
      actions: actionsFor(card.id, subjectActions),
    })),
    top,
    handT.w,
    handT.h,
    viewportWidth,
  );
  maxWidth = Math.max(maxWidth, handWidth);
  const height = top + handHeight + LAYOUT.margin;

  return { width: maxWidth, height, bands, hand, localPlayerId };
}
