import type { EntityId, PlayerId, ValidAction } from '../../protocol';
import type { CardDisplayData, RenderTier } from '../../card/cardFactory';
import type { ColorIdentity } from '../../tokens';

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
   * absent) in a non-commander game (`command` omitted).
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
