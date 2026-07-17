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
import { cardVisualSignature, type CardDisplayData, type RenderTier } from '../card/cardFactory';
import type { GlyphName } from '../chrome/glyphs';
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
  /**
   * Entity id (permanent or hand card). For a collapsed `×N` stack (issue #318) this
   * is the representative — the first member in server order — kept stable so the
   * reconciler reuses one display object across frames; {@link RenderedCard.memberIds}
   * carries the full set.
   */
  entityId: EntityId;
  /** Which zone it lives in — drives its tier and layout row. */
  zone: 'battlefield' | 'hand';
  /** Size tier passed to the card factory (`'chip'` for a land at the back). */
  tier: RenderTier;
  /** Convenience copy of the display name for labels/aria. */
  name: string;
  /** Everything the Pixi factory needs to draw the card. */
  data: CardDisplayData;
  /** Logical position of the card — the visible footprint (rotated, if tapped). */
  rect: Rect;
  /**
   * Every permanent this render stands for (issue #318). Length 1 for a normal card;
   * `> 1` for an `×N` stack of identical-state permanents. A prompt that must address
   * an individual permanent expands stacks (targeting mode never collapses), so each
   * member stays choosable.
   */
  memberIds: EntityId[];
  /** How many permanents this render collapses (`memberIds.length`); `1` if not a stack. */
  stackCount: number;
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
 * A type-grouped row within a band (issue #318). The rows are a **sorting
 * convention, not zones** (see `docs/design/ui-design-notes.md` §Battlefield bands):
 * they order a board so it reads at a glance and never carry rule-implying labels —
 * only the lands row earns the honest `"Lands"` label. Row membership is derived
 * from the server-computed type line alone; the client knows no rules.
 */
export type BandRowKind = 'creatures' | 'support' | 'lands';

/** One type-grouped row's placement + optional (lands-only) label. */
export interface BandRow {
  /** Which type group this row holds. */
  kind: BandRowKind;
  /** The tier its cards render at (creatures→field, support→support, lands→chip). */
  tier: RenderTier;
  /** The row's bounding region in scene coordinates (spans the band width). */
  rect: Rect;
  /** The row label — set only for the lands row (`"Lands"`); rows are not zones. */
  label?: string;
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
  /**
   * The type-grouped rows in this band (issue #318), ordered toward the center line:
   * for the local band, creatures are nearest the center (first) and lands at the
   * back; an opponent's band mirrors this so their creatures sit nearest the center
   * too. Empty rows are omitted. The DOM geography layer labels only the lands row.
   */
  rows: BandRow[];
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
  opts: {
    tapped?: boolean;
    counters?: Counter[];
    selected: boolean;
    actionable: boolean;
    landGlyph?: GlyphName;
  },
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
    // A basic-land chip draws this glyph instead of a name (issue #318); absent for
    // any other render. Derived from the server type line, not a rules lookup.
    landGlyph: opts.landGlyph,
  };
}

/** The subject-actions from `valid_actions[]` that name a given entity. */
function actionsFor(entityId: EntityId, actions: ValidAction[]): ValidAction[] {
  return actions.filter((a) => a.subject?.includes(entityId));
}

/**
 * Which type-grouped row a permanent belongs to (issue #318), derived from the
 * **server-computed type line** alone — the client knows no rules. A permanent that
 * is any kind of creature/planeswalker/battle goes to the front row (so an animated
 * land or crewed Vehicle migrates up when its types change); a land goes to the back
 * chip row; everything else (artifacts, enchantments/auras) is support. The creature
 * test comes first so an "Artifact Creature" or "Land Creature" reads as a creature.
 */
export function rowKindForType(typeLine: string): BandRowKind {
  if (/\b(Creature|Planeswalker|Battle)\b/.test(typeLine)) return 'creatures';
  if (/\bLand\b/.test(typeLine)) return 'lands';
  return 'support';
}

/** The glyph for a basic land's chip, or `undefined` for a nonbasic land / non-land. */
export function basicLandGlyph(typeLine: string): GlyphName | undefined {
  if (!/\bBasic\b/.test(typeLine)) return undefined;
  if (/\bPlains\b/.test(typeLine)) return 'land-plains';
  if (/\bIsland\b/.test(typeLine)) return 'land-island';
  if (/\bSwamp\b/.test(typeLine)) return 'land-swamp';
  if (/\bMountain\b/.test(typeLine)) return 'land-mountain';
  if (/\bForest\b/.test(typeLine)) return 'land-forest';
  return undefined;
}

/** The render tier each type-grouped row uses. */
const TIER_FOR_ROW: Record<BandRowKind, RenderTier> = {
  creatures: 'field',
  support: 'support',
  lands: 'chip',
};

/**
 * A permanent's on-board footprint at its tier. A tapped **field/support** card
 * rotates 90°, so its reserved cell is `h × w` (swapped) — this is exactly what
 * keeps a tapped card from overlapping its neighbors (issue #318). A **chip** never
 * rotates (dim + corner tap glyph instead), so its footprint is constant.
 */
function cellSize(tier: RenderTier, tapped: boolean): { w: number; h: number } {
  if (tier === 'chip') return { w: TIER.chip.w, h: TIER.chip.h };
  const t = TIER[tier];
  return tapped ? { w: t.h, h: t.w } : { w: t.w, h: t.h };
}

/**
 * Collapse identical-state permanents in one row into `×N` stacks (issue #318). The
 * grouping key is the card's **full visual signature** (tap state, counters, and all
 * interactive flags included), so a stack never hides a differing card — "four
 * Plains, one tapped" reads as an untapped ×3 beside a tapped single. A card that
 * carries any individual affordance (an offered action, target candidacy, a
 * multi-select pick, or the current selection) is never folded in, so every
 * individually-addressable permanent stays its own render for prompts and clicks.
 */
function groupStacks(
  cards: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'>[],
): Omit<RenderedCard, 'rect'>[] {
  const result: Omit<RenderedCard, 'rect'>[] = [];
  const stackAt = new Map<string, number>();
  for (const card of cards) {
    const individual =
      card.actions.length > 0 || card.targetable || card.chosen || card.data.selected === true;
    if (individual) {
      result.push({ ...card, stackCount: 1, memberIds: [card.entityId] });
      continue;
    }
    const key = cardVisualSignature(card.data, card.tier);
    const at = stackAt.get(key);
    if (at === undefined) {
      stackAt.set(key, result.length);
      result.push({ ...card, stackCount: 1, memberIds: [card.entityId] });
    } else {
      const group = result[at]!;
      group.memberIds.push(card.entityId);
      group.stackCount += 1;
      group.data = { ...group.data, stackCount: group.stackCount };
    }
  }
  return result;
}

/**
 * Flow a row of (possibly mixed-footprint) cards left-to-right, wrapping to a new
 * line when the next card would cross `availWidth`. Returns the placed cards, the
 * widest extent reached, and the total height the row occupies. Each card's `rect`
 * is its **visible footprint** (rotated dimensions for a tapped field/support card),
 * so both the reconciler's placement and the DOM hotspot cover the drawn card.
 *
 * This is the pure wrapping math the feature turns on: bounding line width to the
 * viewport keeps a 100-permanent board growing downward, never off the right edge
 * (ui-requirements §11 / brief "Dynamic Card Sizing").
 */
function flowRow(
  cards: Omit<RenderedCard, 'rect'>[],
  top: number,
  availWidth: number,
): { placed: RenderedCard[]; width: number; height: number } {
  if (cards.length === 0) return { placed: [], width: 0, height: 0 };
  const limit = availWidth - LAYOUT.margin;
  const placed: RenderedCard[] = [];
  let x: number = LAYOUT.margin;
  let y = top;
  let lineHeight = 0;
  let maxRight: number = LAYOUT.margin;
  for (const card of cards) {
    const size = cellSize(card.tier, card.data.tapped ?? false);
    // Wrap when this card would cross the right edge — but never wrap the first card
    // of a line, so an over-wide card still gets its own line (≥ 1 per line).
    if (x !== LAYOUT.margin && x + size.w > limit) {
      x = LAYOUT.margin;
      y += lineHeight + LAYOUT.rowGap;
      lineHeight = 0;
    }
    placed.push({ ...card, rect: { x, y, w: size.w, h: size.h } });
    maxRight = Math.max(maxRight, x + size.w);
    x += size.w + LAYOUT.cardGap;
    lineHeight = Math.max(lineHeight, size.h);
  }
  return { placed, width: maxRight + LAYOUT.margin, height: y - top + lineHeight };
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
    card: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds' | 'targetable' | 'chosen'>,
  ): Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'> => {
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

  const toRenderable = (
    perm: Permanent,
  ): Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'> => {
    const actions = actionsFor(perm.id, subjectActions);
    const rowKind = rowKindForType(perm.card.type_line);
    const landGlyph = rowKind === 'lands' ? basicLandGlyph(perm.card.type_line) : undefined;
    return withTargeting({
      entityId: perm.id,
      zone: 'battlefield',
      tier: TIER_FOR_ROW[rowKind],
      name: perm.card.name,
      data: toDisplayData(perm.card, {
        tapped: perm.tapped,
        counters: perm.counters,
        selected: perm.id === selectedId,
        actionable: actions.length > 0,
        landGlyph,
      }),
      actions,
    });
  };

  // Lay each band as type-grouped rows (issue #318), reserving a header strip at its
  // top for the DOM geography layer's label + zone piles (issue #278). Band regions
  // are finalized after the full width is known so every lane spans edge-to-edge.
  interface BandMeta {
    playerId: PlayerId;
    isLocal: boolean;
    cards: RenderedCard[];
    rows: BandRow[];
    isEmpty: boolean;
    top: number;
    height: number;
  }
  const bandMetas: BandMeta[] = [];
  let top = LAYOUT.margin;
  let maxWidth = LAYOUT.margin * 2;

  for (const playerId of ordered) {
    const perms = byController.get(playerId) ?? [];
    const isLocal = playerId === localPlayerId;
    const bandTop = top;

    // Split into type-grouped rows in server order, then collapse identical-state
    // permanents in each row into ×N stacks.
    const inRow = (kind: BandRowKind) =>
      groupStacks(perms.filter((p) => rowKindForType(p.card.type_line) === kind).map(toRenderable));
    const grouped: Record<BandRowKind, Omit<RenderedCard, 'rect'>[]> = {
      creatures: inRow('creatures'),
      support: inRow('support'),
      lands: inRow('lands'),
    };

    // Vertical order toward the center line: the local band leads with creatures
    // (nearest center) and lands at the back; an opponent mirrors it so their
    // creatures also sit nearest the center. Empty rows are omitted.
    const order: BandRowKind[] = isLocal
      ? ['creatures', 'support', 'lands']
      : ['lands', 'support', 'creatures'];

    const rows: BandRow[] = [];
    const placedByKind: Record<BandRowKind, RenderedCard[]> = {
      creatures: [],
      support: [],
      lands: [],
    };
    let rowTop = bandTop + LAYOUT.bandHeader;
    for (const kind of order) {
      const cards = grouped[kind];
      if (cards.length === 0) continue;
      const { placed, width, height } = flowRow(cards, rowTop, viewportWidth);
      placedByKind[kind] = placed;
      rows.push({
        kind,
        tier: TIER_FOR_ROW[kind],
        rect: { x: 0, y: rowTop, w: 0, h: height },
        // Only the lands row is labeled — rows are a sorting convention, not zones.
        label: kind === 'lands' ? 'Lands' : undefined,
      });
      maxWidth = Math.max(maxWidth, width);
      rowTop += height + LAYOUT.rowGap;
    }

    // Band cards in a stable order (creatures, support, lands) regardless of the
    // mirrored vertical layout, so the reconciler's draw order stays deterministic.
    const cards = [...placedByKind.creatures, ...placedByKind.support, ...placedByKind.lands];
    const contentHeight = rows.length > 0 ? rowTop - LAYOUT.rowGap - bandTop : LAYOUT.bandHeader;
    // An empty band still reserves a card-height slot so its "invite play" hint fits.
    const bandHeight = rows.length > 0 ? contentHeight : LAYOUT.bandHeader + TIER.field.h;

    bandMetas.push({
      playerId,
      isLocal,
      cards,
      rows,
      isEmpty: perms.length === 0,
      top: bandTop,
      height: bandHeight,
    });
    top = bandTop + bandHeight + LAYOUT.bandGap;
  }

  // Hand along the bottom, in the larger hand tier — wrapping the same way so a big
  // hand also grows downward instead of off the right edge. The hand is never
  // stacked: every card stays individually playable. Its own header strip separates
  // "my hand" from "my battlefield" (issue #278).
  top += LAYOUT.handGap - LAYOUT.bandGap;
  const handTop = top;
  const handCards: Omit<RenderedCard, 'rect'>[] = view.my_hand.map((card) => {
    const actions = actionsFor(card.id, subjectActions);
    const base = withTargeting({
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
    return { ...base, stackCount: 1, memberIds: [card.id] };
  });
  const {
    placed: hand,
    width: handWidth,
    height: handHeight,
  } = flowRow(handCards, handTop + LAYOUT.handHeader, viewportWidth);
  maxWidth = Math.max(maxWidth, handWidth);
  const handRegionHeight = LAYOUT.handHeader + handHeight;
  const height = handTop + handRegionHeight + LAYOUT.margin;

  // Finalize band regions now that the board width is known: every lane (and each of
  // its rows) spans the full width, is labeled by its controller, and carries that
  // controller's pile counts (all straight from the view — no layout state persists).
  const bands: Band[] = bandMetas.map((meta) => ({
    playerId: meta.playerId,
    isLocal: meta.isLocal,
    cards: meta.cards,
    rows: meta.rows.map((row) => ({ ...row, rect: { ...row.rect, w: maxWidth } })),
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
