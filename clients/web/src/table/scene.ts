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
import { TIER, type ColorIdentity } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';
import { playerName } from '../playerNames';

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
  /**
   * The host this render is attached to (issue #333), a passthrough of the server's
   * `attached_to`. Set only when the host is a visible permanent in the same band, so
   * the attachment is laid out adjacent to (clustered with) its host; absent when the
   * permanent is unattached or its host is not in this band (graceful degradation — it
   * renders in its own type row). An attached render never folds into an ×N stack.
   */
  attachedTo?: EntityId;
  /**
   * The attachments clustered under this render (issue #333): the ids of the
   * permanents whose `attached_to` names this one and that lay out adjacent to it,
   * host first. Empty for a permanent with none. A host with attachments never folds
   * into an ×N stack, so its cluster stays coherent and individually addressable.
   */
  attachments?: EntityId[];
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
  /**
   * The graveyard's top card, when the pile is non-empty. Graveyard contents are
   * public in the view (`GameView.graveyards`), so the pile can show what died last
   * in place — filling the `faceUp` slot the pile layout reserved (§Zone piles)
   * without any protocol change. Presentation-only projection of view data.
   */
  graveyardTop?: { name: string; colorIdentity: ColorIdentity };
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
   * The reserved column where this band's zone piles park — a consistent corner of
   * every player's region (§Zone piles), on the table itself rather than in the
   * band's header chrome. The card rows wrap short of this column, so piles and
   * cards never collide; the DOM geography layer renders the pile stack here.
   */
  pileRect: Rect;
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

/**
 * One declared blocker→attacker relationship (issue #332), both as entity ids the
 * scene also renders as cards. Carried on the scene so a focus overlay can draw the
 * link between the two objects (or isolate one object's links on a crowded board)
 * without the client deriving any combat legality — the pair is exactly the server's
 * `blocking` reference. Several links may name the same attacker (multi-block).
 */
export interface CombatLink {
  /** The blocking permanent's entity id. */
  blocker: EntityId;
  /** The attacker it was declared to block. */
  attacker: EntityId;
}

/**
 * One declared attacker→defending-player relationship (issue #341/#347): which player
 * an attacker is attacking, both straight from the view (`Permanent.attacking_player`).
 * Carried on the scene so a renderer can point the attacker's treatment toward the
 * attacked player's area/HUD tile and so any player — including a bystander not being
 * attacked — can read who attacks whom. Reconstructed from the view alone, so a client
 * that mounts mid-combat shows the same assignments as one that watched them declared.
 * Empty in a two-player game (the sole opponent is the only defender) and outside
 * combat.
 */
export interface AttackTarget {
  /** The attacking permanent's entity id. */
  attacker: EntityId;
  /** The defending player it is attacking (their `p{N}` id). */
  defender: PlayerId;
}

/** The full scene: opponents' bands (top), the local band, and the hand. */
export interface TableScene {
  /** Logical width the canvas + DOM overlay share. */
  width: number;
  /** Logical height the canvas + DOM overlay share. */
  height: number;
  /**
   * The uniform card scale this scene was laid out at (issue: spend the screen —
   * ui-design-notes §Tabletop shell "large screens are spent"). Card rects already
   * carry the scaled footprints; the Pixi reconciler applies the same factor to each
   * card display object so drawn pixels match the rects. `1` at the baseline
   * geometry, larger on large viewports — never below 1. Optional so hand-built
   * scene fixtures default to the baseline.
   */
  scale?: number;
  /** Battlefield bands, opponents first and the local player last. */
  bands: Band[];
  /** The local player's hand. */
  hand: RenderedCard[];
  /** The hand row's labeled region (issue #278). */
  handRegion: HandRegion;
  /** The resolved local player id, if it could be identified. */
  localPlayerId?: PlayerId;
  /**
   * The declared blocker→attacker relationships this combat (issue #332), in server
   * order. Empty outside combat. Reconstructed from the view alone, so a client that
   * mounts mid-combat shows the same links as one that watched them being declared.
   */
  combatLinks: CombatLink[];
  /**
   * The declared attacker→defending-player assignments this combat (issue #341/#347),
   * in battlefield order. Empty in a two-player game and outside combat. Reconstructed
   * from the view alone (`Permanent.attacking_player`), so who-attacks-whom is readable
   * on a fresh mount.
   */
  attackTargets: AttackTarget[];
}

/** Layout geometry (logical px, at scale 1). Card sizes come from the TIER tokens. */
const LAYOUT = {
  margin: 16,
  cardGap: 12,
  rowGap: 10,
  bandGap: 18,
  handGap: 28,
  /** Header strip reserved at the top of each band for its DOM nameplate. */
  bandHeader: 40,
  /** Header strip reserved above the hand row for its label. */
  handHeader: 24,
} as const;

/**
 * The reserved zone-pile column (§Zone piles): a fixed-width strip on the right
 * edge of every band where the library/graveyard/exile pile stack parks. Card rows
 * wrap short of it. The DOM piles render at a fixed size (their look is chrome, in
 * CSS), so the reservation is deliberately un-scaled — a bigger table gives cards
 * more room without inflating the pile furniture.
 */
const PILE_COL = {
  /** Full reserved width, including its inner padding. */
  width: 84,
  /** Inner padding between the column edge and the pile stack. */
  pad: 8,
  /** Minimum column height that fits the three-pile stack comfortably. */
  minHeight: 176,
} as const;

/** Per-scene layout metrics: the LAYOUT constants at the scene's card scale. */
interface Metrics {
  margin: number;
  cardGap: number;
  rowGap: number;
  bandGap: number;
  handGap: number;
  bandHeader: number;
  handHeader: number;
}

function metricsAt(scale: number): Metrics {
  return {
    margin: Math.round(LAYOUT.margin * scale),
    cardGap: Math.round(LAYOUT.cardGap * scale),
    rowGap: Math.round(LAYOUT.rowGap * scale),
    bandGap: Math.round(LAYOUT.bandGap * scale),
    handGap: Math.round(LAYOUT.handGap * scale),
    // Header strips hold fixed-size DOM chrome (nameplates), so they stay un-scaled.
    bandHeader: LAYOUT.bandHeader,
    handHeader: LAYOUT.handHeader,
  };
}

/**
 * Options for {@link buildTableScene} beyond the wrap budget: the card scale and
 * the minimum scene height (both derived from the measured shell geometry by
 * `layout()` — the scene itself stays pure).
 */
export interface SceneOptions {
  /**
   * Uniform card/geometry scale (≥ 1). Card footprints, gaps, and margins multiply
   * by it, so large viewports get a proportionally bigger table instead of a small
   * board in a corner.
   */
  scale?: number;
  /**
   * Stretch the scene to at least this height by distributing the slack into the
   * gaps between bands (and before the hand), so the table fills its region — the
   * local band stays anchored near the hand — rather than pooling empty space at
   * the bottom. A scene naturally taller than this is unchanged (it scrolls).
   */
  minHeight?: number;
}

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
 * placement follows control, ui-requirements §2) by their **display name** (issue
 * #294 — players are people, never seat ids, §Identity), falling back to the raw
 * id only when the server sent no name. The local band is marked "(you)" so a
 * newcomer can tell their area from the opponent's.
 */
function bandLabel(view: GameView, playerId: PlayerId, isLocal: boolean): string {
  const name = playerName(view, playerId);
  return isLocal ? `${name} (you)` : name;
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
  const graveyardCards = view.graveyards.find((g) => g.player_id === playerId)?.cards ?? [];
  const exile = view.exile.find((e) => e.player_id === playerId)?.cards.length ?? 0;
  // The graveyard is ordered and public; its last card is the top of the pile, shown
  // face-up in place (§Zone piles — a pile is a place where a card can be shown).
  const topCard = graveyardCards.length > 0 ? graveyardCards[graveyardCards.length - 1] : undefined;
  return {
    library,
    graveyard: graveyardCards.length,
    exile,
    graveyardTop: topCard
      ? { name: topCard.name, colorIdentity: deriveColorIdentity(topCard) }
      : undefined,
  };
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
    attacking?: boolean;
    attackingPlayer?: PlayerId;
    blocking?: boolean;
    blockedBy?: number;
    markedDamage?: number;
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
    // Combat-declaration state and marked damage (issue #332), all straight from the
    // view — the client renders exactly what the server declared and predicts nothing.
    attacking: opts.attacking,
    // Whom this attacker is attacking (issue #341/#347): the card face can point its
    // treatment toward that player. Absent in a two-player game and for non-attackers.
    attackingPlayer: opts.attackingPlayer,
    blocking: opts.blocking,
    blockedBy: opts.blockedBy,
    markedDamage: opts.markedDamage,
    // Purely presentational: the card has ≥1 offered subject-action. No legality
    // is computed here (the server already decided what is offered, issue #277).
    actionable: opts.actionable,
    // A basic-land chip draws this glyph instead of a name (issue #318); absent for
    // any other render. Derived from the server type line, not a rules lookup.
    landGlyph: opts.landGlyph,
    // Card-face information budget (issue #320): keyword glyphs from the server's
    // keyword list, and a latent activated-ability marker read off the printed text.
    keywords: card.keywords,
    hasActivatedAbility: hasActivatedAbilityText(card.rules_text),
  };
}

/**
 * Whether a card's printed rules text describes a **latent activated ability** (issue
 * #320). This is a display heuristic over the server-generated rules text (ADR 0018),
 * **not** rules computation: an activated ability is printed as `"cost: effect"`, so a
 * cost/effect colon marks one — independently of whether the ability is payable right
 * now (that "live" state is the gold edge bar's job, driven by `valid_actions`). If a
 * dedicated view field for this ever ships, this heuristic is the swap point.
 */
function hasActivatedAbilityText(rulesText?: string): boolean {
  return rulesText !== undefined && /:\s/.test(rulesText);
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
function cellSize(tier: RenderTier, tapped: boolean, scale: number): { w: number; h: number } {
  if (tier === 'chip') {
    return { w: Math.round(TIER.chip.w * scale), h: Math.round(TIER.chip.h * scale) };
  }
  const t = TIER[tier];
  const w = Math.round(t.w * scale);
  const h = Math.round(t.h * scale);
  return tapped ? { w: h, h: w } : { w, h };
}

/**
 * A fingerprint of a card's offered subject-actions, used only as part of the ×N
 * grouping key. Two permanents whose full visual state AND offered action shapes
 * (type + label, in server order) are identical are interchangeable from the
 * player's point of view, so they may fold into one stack — activating the stack
 * submits the representative's action id, and the server's next view splits the
 * stack exactly where states diverge. Entity-bound ids are deliberately excluded
 * (they always differ); this is a presentation key, never legality.
 */
function actionFingerprint(actions: ValidAction[]): string {
  return actions.map((a) => `${a.type} ${a.label}`).join('');
}

/**
 * Collapse identical-state permanents in one row into `×N` stacks (issue #318). The
 * grouping key is the card's **full visual signature** (tap state, counters, and all
 * interactive flags included) plus its offered-action fingerprint, so a stack never
 * hides a differing card — "four Plains, one tapped" reads as an untapped ×3 beside
 * a tapped single. Ordinary actionability does NOT force a card to render alone:
 * four untapped Plains each offering the same tap-for-mana action fold into one
 * activatable ×4 stack (activation fires the representative's action). A card that
 * carries a *pick-specific* affordance — target candidacy, a multi-select pick, the
 * current selection, combat participation, or an attachment relationship — is never
 * folded in, so every individually-addressable permanent stays its own render for
 * prompts and clicks (ui-requirements §Table and zones).
 */
function groupStacks(
  cards: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'>[],
): Omit<RenderedCard, 'rect'>[] {
  const result: Omit<RenderedCard, 'rect'>[] = [];
  const stackAt = new Map<string, number>();
  for (const card of cards) {
    const individual =
      card.targetable ||
      card.chosen ||
      card.data.selected === true ||
      // A combat participant (attacker or blocker) stays its own render so its
      // treatment and its blocker→attacker link stay attached to one object, and
      // several blockers on one attacker remain distinguishable (issue #332).
      card.data.attacking === true ||
      card.data.blocking === true ||
      // An attachment, and any host that carries one, stay their own render so the
      // cluster stays coherent and every attached object remains addressable (#333).
      card.attachedTo !== undefined ||
      (card.attachments?.length ?? 0) > 0;
    if (individual) {
      result.push({ ...card, stackCount: 1, memberIds: [card.entityId] });
      continue;
    }
    const key = `${cardVisualSignature(card.data, card.tier)}|${actionFingerprint(card.actions)}`;
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
 * Flow a row of (possibly mixed-footprint) cards, wrapping to a new line when the
 * next card would cross `availWidth`, then **centering each line** within the
 * available span — so a sparse row reads as a balanced table row rather than cards
 * huddled against the left edge (§Tabletop shell: the table stays centered).
 * Returns the placed cards, the widest extent reached, and the total height the
 * row occupies. Each card's `rect` is its **visible footprint** (rotated dimensions
 * for a tapped field/support card), so both the reconciler's placement and the DOM
 * hotspot cover the drawn card.
 *
 * This is the pure wrapping math the feature turns on: bounding line width to the
 * viewport keeps a 100-permanent board growing downward, never off the right edge
 * (ui-requirements §11 / brief "Dynamic Card Sizing").
 */
function flowRow(
  cards: Omit<RenderedCard, 'rect'>[],
  top: number,
  availWidth: number,
  m: Metrics,
  scale: number,
): { placed: RenderedCard[]; width: number; height: number } {
  if (cards.length === 0) return { placed: [], width: 0, height: 0 };
  const limit = availWidth - m.margin;
  const placed: RenderedCard[] = [];
  let x: number = m.margin;
  let y = top;
  let lineHeight = 0;
  let maxRight: number = m.margin;
  let lineStart = 0;

  // Center a completed line [from, placed.length) within [margin, limit].
  const centerLine = (from: number): void => {
    if (from >= placed.length) return;
    const last = placed[placed.length - 1]!;
    const lineRight = last.rect.x + last.rect.w;
    const slack = Math.max(0, limit - lineRight);
    const shift = Math.floor(slack / 2);
    if (shift <= 0) return;
    for (let i = from; i < placed.length; i += 1) placed[i]!.rect.x += shift;
    maxRight = Math.max(maxRight, lineRight + shift);
  };

  for (const card of cards) {
    const size = cellSize(card.tier, card.data.tapped ?? false, scale);
    // Wrap when this card would cross the right edge — but never wrap the first card
    // of a line, so an over-wide card still gets its own line (≥ 1 per line).
    if (x !== m.margin && x + size.w > limit) {
      centerLine(lineStart);
      lineStart = placed.length;
      x = m.margin;
      y += lineHeight + m.rowGap;
      lineHeight = 0;
    }
    placed.push({ ...card, rect: { x, y, w: size.w, h: size.h } });
    maxRight = Math.max(maxRight, x + size.w);
    x += size.w + m.cardGap;
    lineHeight = Math.max(lineHeight, size.h);
  }
  centerLine(lineStart);
  return { placed, width: maxRight + m.margin, height: y - top + lineHeight };
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
 *
 * `opts` carries the shell-derived presentation geometry: the card `scale` (large
 * viewports spend their space on bigger cards) and `minHeight` (slack distributes
 * between bands so the table fills its region). Both default to the pre-existing
 * behavior (scale 1, natural height).
 */
export function buildTableScene(
  view: GameView,
  selectedId?: EntityId,
  viewportWidth: number = DEFAULT_VIEWPORT_WIDTH,
  targeting?: TargetingScene,
  opts?: SceneOptions,
): TableScene {
  const scale = Math.max(1, opts?.scale ?? 1);
  const m = metricsAt(scale);
  const localPlayerId = localPlayerIdOf(view);
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);
  const candidateSet = targeting ? new Set(targeting.candidates) : null;
  const chosenSet = targeting ? new Set(targeting.selected ?? []) : null;

  // Combat relationships (issue #332), all read straight from the view: how many
  // blockers each attacker faces (for the attacker's "blocked ×N" readout) and the
  // flat list of blocker→attacker links. Reconstructed from the view alone, so a
  // client mounting mid-combat derives the same state as one that watched declaration.
  const blockerCountByAttacker = new Map<EntityId, number>();
  const combatLinks: CombatLink[] = [];
  const attackTargets: AttackTarget[] = [];
  for (const perm of view.battlefield) {
    if (perm.blocking !== undefined) {
      blockerCountByAttacker.set(
        perm.blocking,
        (blockerCountByAttacker.get(perm.blocking) ?? 0) + 1,
      );
      combatLinks.push({ blocker: perm.id, attacker: perm.blocking });
    }
    // Whom each attacker attacks (issue #341/#347), straight from the view. Two-player
    // views omit `attacking_player` (the sole opponent is implied), so this stays empty.
    if (perm.attacking_player !== undefined) {
      attackTargets.push({ attacker: perm.id, defender: perm.attacking_player });
    }
  }

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

  // Band order: opponents first (stacked toward the top), then any other
  // controller, then the local band anchored at the bottom. Opponent areas are
  // stacked in the table's **seat order** (`view.seat_order`, issue #345) so their
  // relative positions stay stable across view updates — a bystander who mounts
  // mid-game reads the same arrangement as one who watched it fill (issue #348).
  // `seat_order` is the explicit contract for the arrangement; an older or partial
  // view that omits it falls back to the opponent-projection order (which the server
  // already happens to emit in seat order). Every opponent gets a band even with no
  // permanents, so a three-opponent table always shows three opponent areas.
  const opponentIds = view.opponents.map((o) => o.player_id);
  const opponentSet = new Set(opponentIds);
  const seatOrderOpponents = view.seat_order.filter(
    (id) => id !== localPlayerId && opponentSet.has(id),
  );
  const orderedOpponents =
    seatOrderOpponents.length > 0
      ? // Seat-order first, then any opponent the seat order somehow omitted (defensive).
        [...seatOrderOpponents, ...opponentIds.filter((id) => !seatOrderOpponents.includes(id))]
      : opponentIds;
  const ordered: PlayerId[] = [...orderedOpponents];
  for (const controller of byController.keys()) {
    if (!ordered.includes(controller) && controller !== localPlayerId) ordered.push(controller);
  }
  if (localPlayerId !== undefined) ordered.push(localPlayerId);

  const toRenderable = (
    perm: Permanent,
    cluster?: { attachedTo?: EntityId; attachments?: EntityId[] },
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
        attacking: perm.attacking,
        attackingPlayer: perm.attacking_player,
        blocking: perm.blocking !== undefined,
        blockedBy: blockerCountByAttacker.get(perm.id),
        markedDamage: perm.damage,
      }),
      actions,
      attachedTo: cluster?.attachedTo,
      attachments: cluster?.attachments,
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
  let top = m.margin;
  let maxWidth = m.margin * 2;
  // Card rows wrap short of the reserved zone-pile column on the band's right edge,
  // so piles and cards never collide (§Zone piles: piles are table furniture).
  const rowBudget = Math.max(TIER.field.w * scale + m.margin * 2, viewportWidth - PILE_COL.width);

  for (const playerId of ordered) {
    const perms = byController.get(playerId) ?? [];
    const isLocal = playerId === localPlayerId;
    const bandTop = top;

    // Attachment clustering (issue #333): an attachment whose host is a visible
    // permanent in this same band rides adjacent to that host (in the host's row,
    // host first) instead of flowing in its own type row. A host that is itself
    // attached does not adopt clusters — its own attachments degrade to their normal
    // rows rather than nesting — and an attachment whose host is not in this band
    // (e.g. an aura on an opponent-controlled creature) also degrades to its own row.
    const bandPermById = new Map<EntityId, Permanent>(perms.map((p) => [p.id, p]));
    const clustersUnderHost = (p: Permanent): boolean => {
      if (p.attached_to === undefined) return false;
      const host = bandPermById.get(p.attached_to);
      return host !== undefined && host.attached_to === undefined;
    };
    const attachmentsByHost = new Map<EntityId, Permanent[]>();
    for (const p of perms) {
      if (!clustersUnderHost(p)) continue;
      const list = attachmentsByHost.get(p.attached_to!) ?? [];
      list.push(p);
      attachmentsByHost.set(p.attached_to!, list);
    }

    // Split into type-grouped rows in server order, clustering each host with its
    // attachments, then collapse identical-state permanents in each row into ×N
    // stacks (hosts and attachments never fold, so a cluster stays coherent).
    const inRow = (kind: BandRowKind) => {
      const renderables: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'>[] = [];
      for (const p of perms) {
        if (rowKindForType(p.card.type_line) !== kind) continue;
        // A clustered attachment rides with its host (below), not in its own row.
        if (clustersUnderHost(p)) continue;
        const attachments = attachmentsByHost.get(p.id);
        renderables.push(
          toRenderable(p, attachments ? { attachments: attachments.map((a) => a.id) } : undefined),
        );
        for (const att of attachments ?? []) {
          renderables.push(toRenderable(att, { attachedTo: p.id }));
        }
      }
      return groupStacks(renderables);
    };
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
    let rowTop = bandTop + m.bandHeader;
    for (const kind of order) {
      const cards = grouped[kind];
      if (cards.length === 0) continue;
      const { placed, width, height } = flowRow(cards, rowTop, rowBudget, m, scale);
      placedByKind[kind] = placed;
      rows.push({
        kind,
        tier: TIER_FOR_ROW[kind],
        rect: { x: 0, y: rowTop, w: 0, h: height },
        // Only the lands row is labeled — rows are a sorting convention, not zones.
        label: kind === 'lands' ? 'Lands' : undefined,
      });
      maxWidth = Math.max(maxWidth, width);
      rowTop += height + m.rowGap;
    }

    // Band cards in a stable order (creatures, support, lands) regardless of the
    // mirrored vertical layout, so the reconciler's draw order stays deterministic.
    const cards = [...placedByKind.creatures, ...placedByKind.support, ...placedByKind.lands];
    const contentHeight = rows.length > 0 ? rowTop - m.rowGap - bandTop : m.bandHeader;
    // An empty band still reserves a card-height slot so its "invite play" hint fits;
    // every band is at least tall enough to park its three-pile zone column.
    const naturalHeight =
      rows.length > 0 ? contentHeight : m.bandHeader + Math.round(TIER.field.h * scale);
    const bandHeight = Math.max(naturalHeight, m.bandHeader + PILE_COL.minHeight);

    bandMetas.push({
      playerId,
      isLocal,
      cards,
      rows,
      isEmpty: perms.length === 0,
      top: bandTop,
      height: bandHeight,
    });
    top = bandTop + bandHeight + m.bandGap;
  }

  // Hand along the bottom, in the larger hand tier — wrapping the same way so a big
  // hand also grows downward instead of off the right edge. The hand is never
  // stacked: every card stays individually playable. Its own header strip separates
  // "my hand" from "my battlefield" (issue #278).
  top += m.handGap - m.bandGap;
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
  } = flowRow(handCards, handTop + m.handHeader, viewportWidth, m, scale);
  maxWidth = Math.max(maxWidth, handWidth);
  const handRegionHeight = m.handHeader + handHeight;
  const naturalHeight = handTop + handRegionHeight + m.margin;

  // The scene spans the full wrap budget (bands and centered rows read against the
  // whole table width), growing wider only if a single over-wide card forced it.
  const width = Math.max(viewportWidth, maxWidth);

  // Spend the vertical space (§Tabletop shell: adaptation runs in both directions):
  // when the natural layout is shorter than the region, the slack distributes into
  // the gaps between bands (and before the hand) — band i shifts by i/n of the
  // slack, the hand by all of it — so the table fills its region top to bottom with
  // the local band kept adjacent to the hand. A taller-than-region scene is
  // unchanged (it scrolls). Deterministic: same inputs, same shifts.
  const slack = Math.max(0, Math.floor((opts?.minHeight ?? 0) - naturalHeight));
  const gaps = bandMetas.length; // one gap after each band; the last sits before the hand
  const bandShift = (index: number): number =>
    gaps === 0 ? 0 : Math.floor((slack * index) / gaps);
  const shiftCards = (cards: RenderedCard[], dy: number): RenderedCard[] =>
    dy === 0 ? cards : cards.map((card) => ({ ...card, rect: { ...card.rect, y: card.rect.y + dy } }));

  // Finalize band regions now that the board width is known: every lane (and each of
  // its rows) spans the full width, is labeled by its controller, and carries that
  // controller's pile counts (all straight from the view — no layout state persists).
  const bands: Band[] = bandMetas.map((meta, index) => {
    const dy = bandShift(index);
    return {
      playerId: meta.playerId,
      isLocal: meta.isLocal,
      cards: shiftCards(meta.cards, dy),
      rows: meta.rows.map((row) => ({
        ...row,
        rect: { ...row.rect, y: row.rect.y + dy, w: width },
      })),
      isEmpty: meta.isEmpty,
      label: bandLabel(view, meta.playerId, meta.isLocal),
      zones: zoneCountsOf(view, meta.playerId, meta.isLocal),
      rect: { x: 0, y: meta.top + dy, w: width, h: meta.height },
      // The zone piles park in the reserved right-edge column, below the nameplate
      // strip — the same corner of every player's region (§Zone piles).
      pileRect: {
        x: width - PILE_COL.width + PILE_COL.pad,
        y: meta.top + dy + m.bandHeader,
        w: PILE_COL.width - PILE_COL.pad * 2,
        h: Math.max(0, meta.height - m.bandHeader),
      },
    };
  });

  const handRegion: HandRegion = {
    rect: { x: 0, y: handTop + slack, w: width, h: handRegionHeight },
    label: 'Your hand',
  };

  return {
    width,
    height: Math.max(naturalHeight, opts?.minHeight ?? 0),
    scale,
    bands,
    hand: shiftCards(hand, slack),
    handRegion,
    localPlayerId,
    combatLinks,
    attackTargets,
  };
}
