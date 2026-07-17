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
  /**
   * The subject-actions bound to this entity (empty ⇒ not interactive). During
   * targeting mode this is forced empty: the only interaction is picking a target.
   */
  actions: ValidAction[];
  /**
   * Whether this card is a legal target for the active target slot — set only in
   * targeting mode, straight from the server's candidate list (ADR 0009 §Client).
   * The overlay makes exactly these cards pickable; everything else is dimmed.
   */
  targetable: boolean;
  /**
   * Whether this candidate is currently chosen in the active multi-select slot
   * (issue #143). Only ever true for a {@link RenderedCard.targetable} card while a
   * multi-select toggles a subset; drives the pressed/ringed affordance.
   */
  chosen: boolean;
}

/**
 * The active targeting step, as the table renders it: the legal candidate entity
 * ids for the one slot the player is currently filling. Supplied by the caller
 * (the server enumerated them); the scene highlights exactly these and dims the
 * rest, computing NO legality of its own.
 */
export interface TargetingScene {
  /** The entity ids that are legal targets for the active slot. */
  candidates: EntityId[];
  /**
   * The candidate ids already chosen in the active slot (multi-select only, issue
   * #143). Each such card renders as chosen (ringed/pressed) on top of its
   * targeting highlight; empty/absent for the single-target flow.
   */
  selected?: EntityId[];
}

/** The hidden/derived zone piles that sit in a player's table area (issue #278). */
export interface ZoneCounts {
  /** Library size — the deck pile, shown as a card back with this count. */
  library: number;
  /** Graveyard size — opens the graveyard browser (issue #262). */
  graveyard: number;
  /** Exile size — opens the exile browser (issue #262). */
  exile: number;
}

/** A per-controller battlefield row. */
export interface Band {
  /** Controller of the permanents in this band. */
  playerId: PlayerId;
  /** Whether this is the local player's band (rendered nearest the hand). */
  isLocal: boolean;
  /** The rendered permanents, in server order. */
  cards: RenderedCard[];
  /**
   * The band's bounding region in scene coordinates (issue #278). Spans the board
   * width and always has a height — even an empty band reserves its lane — so the
   * DOM geography layer can draw a labeled, bounded area a newcomer can point at.
   */
  rect: Rect;
  /**
   * The controller's display label ("p1 (you)" / "p2"). Names the *controller* of
   * the band, not the owner — zone placement follows control (ui-requirements §2).
   */
  label: string;
  /** Whether the band holds no permanents (drives the "invite play" placeholder). */
  isEmpty: boolean;
  /** The controller's library/graveyard/exile pile counts, straight from the view. */
  zones: ZoneCounts;
}

/** The local player's hand row as a labeled, bounded region (issue #278). */
export interface HandRegion {
  /** Bounding region of the hand row in scene coordinates. */
  rect: Rect;
  /** Static label for the local hand row. */
  label: string;
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
  /** The hand row's labeled region (issue #278). */
  handRegion: HandRegion;
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
  /** Header strip reserved at the top of each band for its DOM label + zone piles. */
  bandHeader: 48,
  /** Header strip reserved above the hand row for its label. */
  handHeader: 24,
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

/**
 * The band's display label. Names the *controller* of the permanents (zone
 * placement follows control, ui-requirements §2); the local band is marked
 * "(you)" so a newcomer can tell their area from the opponent's.
 */
function bandLabel(playerId: PlayerId, isLocal: boolean): string {
  return isLocal ? `${playerId} (you)` : playerId;
}

/**
 * A controller's library/graveyard/exile pile counts, read straight from the view
 * (the same fields the player tiles show). The local library comes from `me`;
 * an opponent's from its redacted `OpponentView`. Missing piles count as zero.
 */
function zoneCountsOf(view: GameView, playerId: PlayerId, isLocal: boolean): ZoneCounts {
  const library = isLocal
    ? view.me.library_size
    : (view.opponents.find((o) => o.player_id === playerId)?.library_size ?? 0);
  const graveyard = view.graveyards.find((g) => g.player_id === playerId)?.cards.length ?? 0;
  const exile = view.exile.find((e) => e.player_id === playerId)?.cards.length ?? 0;
  return { library, graveyard, exile };
}

/** Map a server card + permanent state onto the factory's display data. */
function toDisplayData(
  card: CardView,
  opts: { tapped?: boolean; counters?: Counter[]; selected: boolean; actionable: boolean },
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
    // Purely presentational: the card has ≥1 offered subject-action. No legality
    // is computed here (the server already decided what is offered, issue #277).
    actionable: opts.actionable,
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
 *
 * When `targeting` is supplied the scene enters targeting mode: only the listed
 * candidate cards are targetable (highlighted with the targeting ring), every
 * other card is dimmed and non-interactive, and normal subject-actions are
 * suppressed so the sole interaction is picking a target. The candidates come
 * straight from the server; the scene derives no legality (ADR 0009 §Client).
 */
export function buildTableScene(
  view: GameView,
  selectedId?: EntityId,
  viewportWidth: number = DEFAULT_VIEWPORT_WIDTH,
  targeting?: TargetingScene,
): TableScene {
  const localPlayerId = localPlayerIdOf(view);
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);
  const candidateSet = targeting ? new Set(targeting.candidates) : null;
  const chosenSet = targeting ? new Set(targeting.selected ?? []) : null;

  // Fold targeting state into a card's display data + interactivity. Outside
  // targeting mode this is a no-op; inside it, the server's candidate list is the
  // only thing deciding highlight (targetable) vs dim, and all subject-actions are
  // suppressed because the only move now is choosing a target. In a multi-select a
  // candidate already toggled into the answer is additionally marked `chosen`
  // (ringed), reusing the selection ring so the pick reads as committed.
  const withTargeting = (
    card: Omit<RenderedCard, 'rect' | 'targetable' | 'chosen'>,
  ): Omit<RenderedCard, 'rect'> => {
    if (candidateSet === null) return { ...card, targetable: false, chosen: false };
    const targetable = candidateSet.has(card.entityId);
    const chosen = targetable && (chosenSet?.has(card.entityId) ?? false);
    return {
      ...card,
      // A chosen multi-select candidate draws the selection ring; a not-yet-chosen
      // candidate shows only the targeting highlight. The play affordance is
      // suppressed in targeting mode — the sole interaction is picking a target,
      // so no card should advertise a subject-action (issue #277).
      data: {
        ...card.data,
        selected: chosen,
        targeting: targetable,
        dimmed: !targetable,
        actionable: false,
      },
      actions: [],
      targetable,
      chosen,
    };
  };

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

  const toRenderable = (perm: Permanent): Omit<RenderedCard, 'rect'> => {
    const actions = actionsFor(perm.id, subjectActions);
    return withTargeting({
      entityId: perm.id,
      zone: 'battlefield',
      tier: 'field',
      name: perm.card.name,
      data: toDisplayData(perm.card, {
        tapped: perm.tapped,
        counters: perm.counters,
        selected: perm.id === selectedId,
        actionable: actions.length > 0,
      }),
      actions,
    });
  };

  // Lay each band, reserving a header strip at its top for the DOM geography
  // layer's label + zone piles (issue #278). Band regions are finalized after the
  // full width is known so every lane spans the board edge-to-edge.
  interface BandMeta {
    playerId: PlayerId;
    isLocal: boolean;
    cards: RenderedCard[];
    isEmpty: boolean;
    top: number;
    height: number;
  }
  const bandMetas: BandMeta[] = [];
  let top = LAYOUT.margin;
  let maxWidth = LAYOUT.margin * 2;
  const fieldT = TIER.field;

  for (const playerId of ordered) {
    const perms = byController.get(playerId) ?? [];
    const bandTop = top;
    const { placed, width, height } = layBand(
      perms.map(toRenderable),
      bandTop + LAYOUT.bandHeader,
      fieldT.w,
      fieldT.h,
      viewportWidth,
    );
    const bandHeight = LAYOUT.bandHeader + height;
    bandMetas.push({
      playerId,
      isLocal: playerId === localPlayerId,
      cards: placed,
      isEmpty: perms.length === 0,
      top: bandTop,
      height: bandHeight,
    });
    maxWidth = Math.max(maxWidth, width);
    top = bandTop + bandHeight + LAYOUT.bandGap;
  }

  // Hand along the bottom, in the larger hand tier — wrapping the same way so a
  // big hand also grows downward instead of off the right edge. Its own header
  // strip separates "my hand" from "my battlefield" (issue #278).
  top += LAYOUT.handGap - LAYOUT.bandGap;
  const handTop = top;
  const handT = TIER.hand;
  const {
    placed: hand,
    width: handWidth,
    height: handHeight,
  } = layBand(
    view.my_hand.map((card) => {
      const actions = actionsFor(card.id, subjectActions);
      return withTargeting({
        entityId: card.id,
        zone: 'hand' as const,
        tier: 'hand' as const,
        name: card.name,
        data: toDisplayData(card, {
          selected: card.id === selectedId,
          actionable: actions.length > 0,
        }),
        actions,
      });
    }),
    handTop + LAYOUT.handHeader,
    handT.w,
    handT.h,
    viewportWidth,
  );
  maxWidth = Math.max(maxWidth, handWidth);
  const handRegionHeight = LAYOUT.handHeader + handHeight;
  const height = handTop + handRegionHeight + LAYOUT.margin;

  // Finalize band regions now that the board width is known: every lane spans the
  // full width, is labeled by its controller, and carries that controller's pile
  // counts (all straight from the view — no layout state persists across messages).
  const bands: Band[] = bandMetas.map((meta) => ({
    playerId: meta.playerId,
    isLocal: meta.isLocal,
    cards: meta.cards,
    isEmpty: meta.isEmpty,
    label: bandLabel(meta.playerId, meta.isLocal),
    zones: zoneCountsOf(view, meta.playerId, meta.isLocal),
    rect: { x: 0, y: meta.top, w: maxWidth, h: meta.height },
  }));

  const handRegion: HandRegion = {
    rect: { x: 0, y: handTop, w: maxWidth, h: handRegionHeight },
    label: 'Your hand',
  };

  return { width: maxWidth, height, bands, hand, handRegion, localPlayerId };
}
