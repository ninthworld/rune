/**
 * GameView → table scene mapping (fixed-shell anatomy, ADR 0023).
 *
 * A **pure** function that turns the store's latest {@link GameView} into the set
 * of rendered entities — one band per player panel plus the local hand — each
 * carrying the {@link CardDisplayData} the Pixi factory draws, a layout `rect`,
 * and the `valid_actions` that belong to it (ADR 0004 subject routing). Keeping
 * this pure and headless makes the whole GameView→scene mapping unit-testable
 * without a WebGL context — the React/Pixi layers only position what it returns.
 *
 * The scene lays out into the **carved panel frames** the shell layout supplies
 * ({@link SceneGeometry}, from `layout.ts`): each player's cards live inside their
 * own bounded panel, and the hand lives inside the bottom shell's hand area.
 * Fixed zone homes are what make travel animations legible and drops
 * deterministic (`docs/design/ui-blueprint.md`).
 *
 * Density ladder (blueprint §Density ladder): per panel, engaged automatically —
 * full tier for the surface, then one card-tier step down, then aggressive ×N
 * folding (the stack grouping below), then vertical compression as the last
 * resort. Each panel picks its own rung: one hoarding opponent never shrinks the
 * others.
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
import { artKeyFor } from '../card/art/artStore';
import type { GlyphName } from '../chrome/glyphs';
import { TAP, TIER, type ColorIdentity } from '../tokens';
import { deriveColorIdentity } from './colorIdentity';
import { identityAccent } from './identityAccents';
import { layout } from './layout';
import { playerName } from '../playerNames';

/** Absolute placement of a card within the scene's logical coordinate space. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The full-face card tier a battlefield surface uses (blueprint §Card
 * vocabulary): the receiver's board is one step larger than the opponents', and
 * the density ladder may step a crowded panel down one rung. Lands always render
 * as chips; the hand always renders at the hand tier.
 */
export type SurfaceTier = 'field' | 'support' | 'mini';

/** One carved player panel: full rect, header strip, card content area, and the
 * piles column (zero-width when the panel parks no piles — the local panel's
 * piles live in the bottom shell). All canvas-local coordinates. */
export interface PanelFrame {
  rect: Rect;
  header: Rect;
  content: Rect;
  piles: Rect;
  /**
   * Whether this opponent frame is a **collapsed summary tile** (issue #400): the
   * phone-portrait multiplayer composition demotes every un-focused opponent to a
   * crest/name/counts tile with no card area (`content`/`piles` are zero-sized).
   * Undefined on the full/duel compositions and on the receiver's own frame — those
   * always render a full battlefield. Set by the layout; the scene builder skips
   * card-laying for a summary frame and the chrome layer renders the tile.
   */
  summary?: boolean;
}

/** The card-surface geometry the shell layout carves for the scene builder:
 * per-player panel frames, the hand area, and the tier assignments. */
export interface SceneGeometry {
  /** Canvas logical width. */
  width: number;
  /** Canvas logical height. */
  height: number;
  /** Opponent panel frames, in seat order. */
  opponents: PanelFrame[];
  /** The receiver's battlefield panel frame. */
  you: PanelFrame;
  /** The hand card area (bottom shell). */
  hand: Rect;
  /** Per-surface card tiers (the density ladder may step these down per panel). */
  tiers: { you: SurfaceTier; opp: SurfaceTier };
  /** Whether the hand overlaps into a fan when it outgrows its area (compact). */
  handFan: boolean;
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
  /** Logical position of the card — the visible footprint (the rotated bounding
   * box, if tapped: one ~25° tap treatment at every tier, blueprint §Card
   * vocabulary). */
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
   * The offered subject-less **combat declaration** (declare attackers/blockers)
   * that lists this entity among its candidates (ADR 0025), when the entity has
   * no subject-actions of its own. Makes the candidate directly interactive: a
   * click enters the declaration with this entity pre-toggled, instead of the
   * player hunting the dock button first. Pure projection of `valid_actions` —
   * the client still computes no legality. Suppressed during targeting mode.
   */
  declaration?: ValidAction;
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
  /** The tier its cards render at. */
  tier: RenderTier;
  /** The row's bounding region in scene coordinates (spans the panel content). */
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
   * Command-zone size (CR 903.6, issue #372) — how many of this player's cards sit
   * in the command zone (their commander, while it is there). Omitted (treated as
   * absent) in a non-commander game, so the pile only appears where one exists.
   */
  command?: number;
  /**
   * The graveyard's top card, when the pile is non-empty. Graveyard contents are
   * public in the view (`GameView.graveyards`), so the pile can show what died last
   * in place — filling the `faceUp` slot the pile layout reserved (§Zone piles)
   * without any protocol change. Presentation-only projection of view data.
   */
  graveyardTop?: { name: string; colorIdentity: ColorIdentity };
}

/** A per-player battlefield panel band. */
export interface Band {
  /** Controller of the permanents in this band. */
  playerId: PlayerId;
  /** Whether this is the local player's band. */
  isLocal: boolean;
  /** The rendered permanents, in server order. */
  cards: RenderedCard[];
  /** The band's full panel rect in scene coordinates (ADR 0023: a carved home). */
  rect: Rect;
  /** The panel's header strip rect (the DOM chrome layer renders the crest,
   * nameplate, and meta here). */
  headerRect: Rect;
  /**
   * The controller's display label. Names the *controller* of the band, not the
   * owner — zone placement follows control (ui-requirements §2).
   */
  label: string;
  /** Whether the band holds no permanents (drives the "invite play" placeholder). */
  isEmpty: boolean;
  /** The controller's library/graveyard/exile pile counts, straight from the view. */
  zones: ZoneCounts;
  /**
   * The controller's identity accent (§Identity): worn by the panel border and
   * nameplate — never by cards. Deterministic from the view's seat order, so every
   * client (and a fresh mount) derives the same color for the same player.
   */
  accent: string;
  /**
   * The panel's piles column, where library/graveyard/exile park (blueprint: a
   * consistent edge of every opponent panel). Zero-width for the local band on the
   * full composition — the receiver's piles live in the bottom shell.
   */
  pileRect: Rect;
  /** The type-grouped rows in this band (issue #318), top-to-bottom: creatures,
   * support, lands (the shared vocabulary of the blueprint mocks). Empty rows are
   * omitted. The DOM chrome layer labels only the lands row. */
  rows: BandRow[];
  /** The density-ladder rung this panel resolved (0 = full tier, 1 = stepped
   * down, 2 = stepped down + vertically compressed). Presentation metadata. */
  densityRung: number;
  /**
   * Whether this band is a **collapsed summary tile** (issue #400, phone-portrait
   * multiplayer): it carries the controller's identity + counts but no rendered
   * cards, and the chrome layer draws it as a tap-to-focus tile instead of a full
   * battlefield. False for the receiver, every full/duel-composition panel, and the
   * one opponent currently expanded in place.
   */
  summary: boolean;
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
 * attacked player's panel and so any player — including a bystander not being
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

/** The full scene: one band per player panel, plus the hand. */
export interface TableScene {
  /** Logical width the canvas + DOM overlay share. */
  width: number;
  /** Logical height the canvas + DOM overlay share. */
  height: number;
  /** Uniform display scale (legacy; the fixed shell always lays out at 1 — tiers,
   * not scaling, spend the screen). Kept optional for the reconciler contract. */
  scale?: number;
  /** Player bands, opponents (seat order) first and the local player last. */
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

/** Layout metrics (logical px). Card sizes come from the TIER tokens. */
const M = {
  cardGap: 10,
  rowGap: 8,
  handGap: 8,
  /** The hand fan never overlaps a card past this fraction of its width. */
  fanMaxOverlap: 0.62,
  /** A selected/lifted hand card raises by this much (the fan lift). */
  handLift: 12,
} as const;

/**
 * Default logical width used when tests build a scene without a measured shell
 * (see {@link defaultSceneGeometry}).
 */
export const DEFAULT_VIEWPORT_WIDTH = 1280;

/**
 * A geometry for callers with no measured shell (tests, fixtures): the full
 * composition carved by the real layout function at the default viewport.
 * Implemented via `layout()` so tests exercise the same carve as the live table.
 * (The import is cycle-safe: `layout.ts` imports only *types* from this module,
 * which are erased at compile time.)
 */
export function defaultSceneGeometry(
  playerCount = 2,
  viewport: { width: number; height: number } = { width: DEFAULT_VIEWPORT_WIDTH, height: 800 },
): SceneGeometry {
  return layout(viewport, playerCount).scene;
}

/**
 * The receiver's own seat id, taken straight from `view.you`. An older server
 * may send it empty; treat that as "unknown" (`undefined`) so band ordering and
 * `isLocal` degrade the same way they did before the field existed.
 */
function localPlayerIdOf(view: GameView): PlayerId | undefined {
  return view.you || undefined;
}

/**
 * The opponents in the table's stable **seat order** (`view.seat_order`, issue
 * #345), excluding the receiver — the exact order the scene lays opponent panels
 * and the shell carves opponent frames, so an index into this list addresses one
 * opponent frame. Falls back to `view.opponents` order where the server sent no
 * seat order, and appends any opponent the seat order omits so none is dropped.
 * Shared by the scene builder and the table's focus/expansion mapping (issue #400).
 */
export function orderedOpponentIds(view: GameView): PlayerId[] {
  const localPlayerId = localPlayerIdOf(view);
  const opponentIds = view.opponents.map((o) => o.player_id);
  const opponentSet = new Set(opponentIds);
  const seatOrderOpponents = view.seat_order.filter(
    (id) => id !== localPlayerId && opponentSet.has(id),
  );
  return seatOrderOpponents.length > 0
    ? [...seatOrderOpponents, ...opponentIds.filter((id) => !seatOrderOpponents.includes(id))]
    : opponentIds;
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
export function zoneCountsOf(view: GameView, playerId: PlayerId, isLocal: boolean): ZoneCounts {
  const library = isLocal
    ? view.me.library_size
    : (view.opponents.find((o) => o.player_id === playerId)?.library_size ?? 0);
  const graveyardCards = view.graveyards.find((g) => g.player_id === playerId)?.cards ?? [];
  const exile = view.exile.find((e) => e.player_id === playerId)?.cards.length ?? 0;
  // The command zone (issue #372) is public, like graveyard/exile; count this
  // player's entry, defaulting to 0 in a non-commander game (`command` omitted).
  const command = (view.command ?? []).find((c) => c.player_id === playerId)?.cards.length ?? 0;
  // The graveyard is ordered and public; its last card is the top of the pile, shown
  // face-up in place (§Zone piles — a pile is a place where a card can be shown).
  const topCard = graveyardCards.length > 0 ? graveyardCards[graveyardCards.length - 1] : undefined;
  return {
    library,
    graveyard: graveyardCards.length,
    exile,
    command,
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
    // The card's currently-published illustration key (ADR 0024), looked up by the
    // stable `functional_id` the protocol reserved for client-local presentation
    // enrichment. Undefined under the procedural default or until art loads, so the
    // face — and every existing test — is byte-identical without art.
    artKey: artKeyFor(card.functional_id),
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

/** The combat-declaration kinds whose candidates get direct entity entry (ADR 0025). */
const DECLARATION_KINDS = new Set(['declare_attackers', 'declare_blockers']);

/**
 * The single offered subject-less combat declaration listing `entityId` among
 * its requirement candidates (ADR 0025), or `undefined` when none. Only the two
 * combat declarations participate — reversible toggle-and-confirm flows where
 * "click the creature" is unmistakably the player's intent; other multi-select
 * actions (mulligan bottoming, zone selections) keep their explicit entry.
 */
function declarationFor(entityId: EntityId, actions: ValidAction[]): ValidAction | undefined {
  const matches = actions.filter(
    (a) =>
      DECLARATION_KINDS.has(a.type) &&
      (a.subject === undefined || a.subject.length === 0) &&
      (a.requirements ?? []).some((r) => (r.candidates ?? []).includes(entityId)),
  );
  return matches.length === 1 ? matches[0] : undefined;
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

/** The per-row tiers at a surface tier (creatures lead one step over support). */
function tiersForSurface(surface: SurfaceTier): Record<BandRowKind, RenderTier> {
  if (surface === 'field') return { creatures: 'field', support: 'support', lands: 'chip' };
  if (surface === 'support') return { creatures: 'support', support: 'mini', lands: 'chip' };
  return { creatures: 'mini', support: 'mini', lands: 'chip' };
}

/** One step down the tier ladder (blueprint §Density ladder rung 2). */
function stepDown(surface: SurfaceTier): SurfaceTier {
  return surface === 'field' ? 'support' : 'mini';
}

/**
 * A card's on-board footprint at its tier: the **rotated bounding box** when
 * tapped. Tap is ONE treatment at every tier — a ~{@link TAP.angle} rotation plus
 * a slight dim (blueprint §Card vocabulary) — so the reserved cell is the box the
 * rotated card sweeps; the row gap absorbs the swept corners.
 */
export function cellSize(tier: RenderTier, tapped: boolean): { w: number; h: number } {
  const t = TIER[tier];
  if (!tapped) return { w: t.w, h: t.h };
  return tappedFootprint(t.w, t.h);
}

/** The axis-aligned bounding box of a `w×h` card rotated by the tap angle. */
export function tappedFootprint(w: number, h: number): { w: number; h: number } {
  const c = Math.cos(TAP.angle);
  const s = Math.sin(TAP.angle);
  return { w: Math.round(w * c + h * s), h: Math.round(w * s + h * c) };
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
  return actions.map((a) => `${a.type} ${a.label}`).join('');
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
 * Flow a row of (possibly mixed-footprint) cards inside a content area, wrapping
 * to a new line when the next card would cross the right edge, then **centering
 * each line** within the span — the blueprint mocks' centered row lines. Returns
 * the placed cards and the total height. Each card's `rect` is its **visible
 * footprint** (the rotated bounding box for a tapped card), so both the
 * reconciler's placement and the DOM hotspot cover the drawn card.
 */
function flowRow(
  cards: Omit<RenderedCard, 'rect'>[],
  left: number,
  top: number,
  availWidth: number,
): { placed: RenderedCard[]; height: number } {
  if (cards.length === 0) return { placed: [], height: 0 };
  const limit = left + availWidth;
  const placed: RenderedCard[] = [];
  let x = left;
  let y = top;
  let lineHeight = 0;
  let lineStart = 0;

  // Center a completed line [from, placed.length) within [left, limit], and
  // bottom-align its cards so mixed footprints share a baseline.
  const closeLine = (from: number): void => {
    if (from >= placed.length) return;
    const last = placed[placed.length - 1]!;
    const lineRight = last.rect.x + last.rect.w;
    const shift = Math.floor(Math.max(0, limit - lineRight) / 2);
    for (let i = from; i < placed.length; i += 1) {
      const rect = placed[i]!.rect;
      rect.x += shift;
      rect.y += lineHeight - rect.h;
    }
  };

  for (const card of cards) {
    const size = cellSize(card.tier, card.data.tapped ?? false);
    // Wrap when this card would cross the right edge — but never wrap the first card
    // of a line, so an over-wide card still gets its own line (≥ 1 per line).
    if (x !== left && x + size.w > limit) {
      closeLine(lineStart);
      lineStart = placed.length;
      x = left;
      y += lineHeight + M.rowGap;
      lineHeight = 0;
    }
    placed.push({ ...card, rect: { x, y, w: size.w, h: size.h } });
    x += size.w + M.cardGap;
    lineHeight = Math.max(lineHeight, size.h);
  }
  closeLine(lineStart);
  return { placed, height: y - top + lineHeight };
}

/** Per-panel layout result before finalization. */
interface PanelLayout {
  cards: RenderedCard[];
  rows: BandRow[];
  densityRung: number;
}

/**
 * Lay one player's permanents into their panel content area, engaging the density
 * ladder: full surface tier → one tier step down → vertical compression. Rows run
 * creatures / support / lands top-to-bottom (the shared mock vocabulary) and the
 * row block centers vertically in the content area.
 */
function layPanel(
  renderables: Record<BandRowKind, Omit<RenderedCard, 'rect'>[]>,
  content: Rect,
  surface: SurfaceTier,
): PanelLayout {
  const attempt = (
    tier: SurfaceTier,
  ): { cards: RenderedCard[]; rows: BandRow[]; height: number } => {
    const tiers = tiersForSurface(tier);
    const rows: BandRow[] = [];
    const cards: RenderedCard[] = [];
    let top = 0;
    for (const kind of ['creatures', 'support', 'lands'] as BandRowKind[]) {
      const rowCards = renderables[kind].map((card) => ({ ...card, tier: tiers[kind] }));
      if (rowCards.length === 0) continue;
      const { placed, height } = flowRow(rowCards, content.x, content.y + top, content.w);
      cards.push(...placed);
      rows.push({
        kind,
        tier: tiers[kind],
        rect: { x: content.x, y: content.y + top, w: content.w, h: height },
        // Only the lands row is labeled — rows are a sorting convention, not zones.
        label: kind === 'lands' ? 'Lands' : undefined,
      });
      top += height + M.rowGap;
    }
    return { cards, rows, height: rows.length > 0 ? top - M.rowGap : 0 };
  };

  // Rung 1: the surface's full tier. Rung 2: one step down (per panel — one
  // hoarding opponent never shrinks the others).
  let rung = 0;
  let laid = attempt(surface);
  if (laid.height > content.h && surface !== 'mini') {
    rung = 1;
    laid = attempt(stepDown(surface));
  }

  // Center the row block vertically; if it still overflows, compress the vertical
  // offsets (cards overlap upward like a crowded physical table) so the panel
  // never spills into its neighbors — nothing can clip by construction.
  let shift = Math.max(0, Math.floor((content.h - laid.height) / 2));
  let squeeze = 1;
  if (laid.height > content.h && laid.height > 0) {
    rung = 2;
    shift = 0;
    const lastRow = laid.rows[laid.rows.length - 1]!;
    const lastRowH = lastRow.rect.h;
    const travel = laid.height - lastRowH;
    if (travel > 0) squeeze = Math.max(0.35, (content.h - lastRowH) / travel);
  }
  const remap = (y: number): number => content.y + Math.round((y - content.y) * squeeze) + shift;
  for (const card of laid.cards) card.rect.y = remap(card.rect.y);
  for (const row of laid.rows) row.rect.y = remap(row.rect.y);

  return { cards: laid.cards, rows: laid.rows, densityRung: rung };
}

/**
 * Build the full scene from a view and the shell's carved geometry. `selectedId`
 * marks the currently selected entity so its card draws a selection ring (and a
 * selected hand card lifts); it never changes what is offered.
 *
 * When `targeting` is supplied the scene enters targeting mode: only the listed
 * candidate cards are targetable (highlighted with the targeting ring), every
 * other card is dimmed and non-interactive, and normal subject-actions are
 * suppressed so the sole interaction is picking a target. The candidates come
 * straight from the server; the scene derives no legality (ADR 0009 §Client).
 */
export function buildTableScene(
  view: GameView,
  selectedId: EntityId | undefined,
  geometry: SceneGeometry,
  targeting?: TargetingScene,
): TableScene {
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
      // Entity entry is likewise suppressed mid-pick: the flow is already open.
      declaration: undefined,
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

  // Panel order: opponents in the table's **seat order** (`view.seat_order`, issue
  // #345) so their relative positions stay stable across view updates — a bystander
  // who mounts mid-game reads the same arrangement as one who watched it fill
  // (issue #348) — then the local player. Every opponent gets a panel even with no
  // permanents. Any controller the seat order somehow omits is appended
  // defensively so its permanents always render.
  const orderedOpponents = orderedOpponentIds(view);
  const ordered: PlayerId[] = [...orderedOpponents];
  for (const controller of byController.keys()) {
    if (!ordered.includes(controller) && controller !== localPlayerId) ordered.push(controller);
  }

  const toRenderable = (
    perm: Permanent,
    cluster?: { attachedTo?: EntityId; attachments?: EntityId[] },
  ): Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'> => {
    const actions = actionsFor(perm.id, subjectActions);
    // A creature with no subject-actions may still be a candidate of an offered
    // combat declaration (ADR 0025): it becomes directly interactive — and wears
    // the playable affordance — as the entry point into that declaration.
    const declaration =
      actions.length === 0 ? declarationFor(perm.id, view.valid_actions) : undefined;
    const rowKind = rowKindForType(perm.card.type_line);
    const landGlyph = rowKind === 'lands' ? basicLandGlyph(perm.card.type_line) : undefined;
    return withTargeting({
      entityId: perm.id,
      zone: 'battlefield',
      // The tier is provisional; `layPanel` assigns the panel's resolved tier.
      tier: 'support',
      name: perm.card.name,
      data: toDisplayData(perm.card, {
        tapped: perm.tapped,
        counters: perm.counters,
        selected: perm.id === selectedId,
        actionable: actions.length > 0 || declaration !== undefined,
        landGlyph,
        attacking: perm.attacking,
        attackingPlayer: perm.attacking_player,
        blocking: perm.blocking !== undefined,
        blockedBy: blockerCountByAttacker.get(perm.id),
        markedDamage: perm.damage,
      }),
      actions,
      declaration,
      attachedTo: cluster?.attachedTo,
      attachments: cluster?.attachments,
    });
  };

  /** Build a controller's row renderables (clustered, stacked) in server order. */
  const renderablesFor = (
    perms: Permanent[],
  ): Record<BandRowKind, Omit<RenderedCard, 'rect'>[]> => {
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
    const inRow = (kind: BandRowKind): Omit<RenderedCard, 'rect'>[] => {
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
    return { creatures: inRow('creatures'), support: inRow('support'), lands: inRow('lands') };
  };

  // Lay each panel: opponents into their carved frames (seat order), the receiver
  // into theirs. Each panel engages its own density-ladder rung. With no receiver
  // (a spectator's table) the `you` frame joins the pool, so every seat gets a
  // panel and no carved area sits empty.
  const frames =
    localPlayerId === undefined ? [...geometry.opponents, geometry.you] : geometry.opponents;
  const bands: Band[] = [];
  ordered.forEach((playerId, index) => {
    const frame = frames[Math.min(index, frames.length - 1)] ?? geometry.you;
    const perms = byController.get(playerId) ?? [];
    // A collapsed summary tile (issue #400): no cards are laid — the chrome layer
    // draws identity + counts and a tap-to-focus affordance. Its permanents still
    // exist in the view (and remain reachable by expanding the tile), so nothing is
    // dropped from the reconstructable state; they simply are not drawn here.
    if (frame.summary) {
      bands.push({
        playerId,
        isLocal: false,
        cards: [],
        rows: [],
        isEmpty: perms.length === 0,
        label: bandLabel(view, playerId, false),
        zones: zoneCountsOf(view, playerId, false),
        accent: identityAccent(view, playerId),
        rect: frame.rect,
        headerRect: frame.header,
        pileRect: frame.piles,
        densityRung: 0,
        summary: true,
      });
      return;
    }
    const laid = layPanel(renderablesFor(perms), frame.content, geometry.tiers.opp);
    bands.push({
      playerId,
      isLocal: false,
      cards: laid.cards,
      rows: laid.rows,
      isEmpty: perms.length === 0,
      label: bandLabel(view, playerId, false),
      zones: zoneCountsOf(view, playerId, false),
      accent: identityAccent(view, playerId),
      rect: frame.rect,
      headerRect: frame.header,
      pileRect: frame.piles,
      densityRung: laid.densityRung,
      summary: false,
    });
  });
  if (localPlayerId !== undefined) {
    const perms = byController.get(localPlayerId) ?? [];
    const laid = layPanel(renderablesFor(perms), geometry.you.content, geometry.tiers.you);
    bands.push({
      playerId: localPlayerId,
      isLocal: true,
      cards: laid.cards,
      rows: laid.rows,
      isEmpty: perms.length === 0,
      label: bandLabel(view, localPlayerId, true),
      zones: zoneCountsOf(view, localPlayerId, true),
      accent: identityAccent(view, localPlayerId),
      rect: geometry.you.rect,
      headerRect: geometry.you.header,
      pileRect: geometry.you.piles,
      densityRung: laid.densityRung,
      summary: false,
    });
  }

  // The hand in the bottom shell's hand area: a single centered row at the hand
  // tier that never wraps — when it outgrows the area it overlaps into a fan
  // (blueprint: the hand becomes an overlapping fan; a selected card lifts). The
  // hand is never stacked: every card stays individually playable.
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
  const hand = layHand(handCards, geometry.hand, selectedId);

  const handRegion: HandRegion = { rect: geometry.hand, label: 'Your hand' };

  return {
    width: geometry.width,
    height: geometry.height,
    bands,
    hand,
    handRegion,
    localPlayerId,
    combatLinks,
    attackTargets,
  };
}

/**
 * Lay the hand as one centered row that compresses into an overlapping fan when
 * it outgrows its area (never wrapping — the hand row is a fixed shell home). A
 * selected card lifts by {@link M.handLift}; later cards draw over earlier ones,
 * matching the fan's physical stacking.
 */
function layHand(
  cards: Omit<RenderedCard, 'rect'>[],
  area: Rect,
  selectedId: EntityId | undefined,
): RenderedCard[] {
  if (cards.length === 0) return [];
  const t = TIER.hand;
  const w = t.w;
  const h = t.h;
  const n = cards.length;
  const natural = n * w + (n - 1) * M.handGap;
  // Step between card lefts: the natural pitch when it fits, else compressed down
  // to the fan's max overlap.
  const minStep = Math.ceil(w * (1 - M.fanMaxOverlap));
  const step =
    natural <= area.w || n === 1
      ? w + M.handGap
      : Math.max(minStep, Math.floor((area.w - w) / (n - 1)));
  const total = w + step * (n - 1);
  const left = area.x + Math.max(0, Math.floor((area.w - total) / 2));
  const top = area.y + Math.max(0, area.h - h);
  return cards.map((card, i) => {
    const lifted = selectedId !== undefined && card.entityId === selectedId;
    return {
      ...card,
      rect: { x: left + i * step, y: lifted ? Math.max(area.y, top - M.handLift) : top, w, h },
    };
  });
}
