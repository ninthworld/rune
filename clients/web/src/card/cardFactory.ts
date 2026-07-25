/**
 * Pure card **display-data model** (ADR 0030: the shipped match renders cards in
 * React DOM via {@link card/dom/CardFace}, with Pixi kept only for the passive
 * effects overlay).
 *
 * This module is now data-only: it defines the plain description a card face is
 * built from ({@link CardDisplayData}), the stable {@link cardVisualSignature}
 * that is the reconcile/fold key for "same-looking card", and {@link parseManaCost}
 * for turning a server mana-cost string into drawable pips. The former Pixi draw
 * path (`buildCardDisplay`/`buildChipDisplay`) was retired with the legacy scene
 * stack (issue #504); the DOM `CardFace` is the sole renderer.
 *
 * Design rules this module obeys:
 * - **No game logic.** Effective power/toughness and counter counts ride through
 *   exactly as supplied; nothing here adds, subtracts, or derives characteristics.
 *   Color identity is supplied by the caller, not derived here.
 * - **No images, official frames, or WotC branding.** These are plain data fields.
 */
import { PIP, type ColorIdentity } from '../tokens';
import type { GlyphName } from '../chrome/glyphs';
// The pure tokenizer directly, not the package root: this module is data-only
// and must not pull the React symbol component (or its stylesheet) in with it.
import { tokenizeNotation } from '../chrome/symbols/notation';

/** Tiers that render a full card face (chips are a separate digest representation).
 * `mini` is the stepped-down dense tier the density ladder engages (blueprint). */
export type CardTier = 'mini' | 'support' | 'field' | 'hand';

/**
 * The full set of size tiers a battlefield object can render at, including the
 * digest **chip** (issue #318) used for lands at the back of a band. A chip is not
 * a full face — the DOM renderer draws it in its compact form.
 */
export type RenderTier = CardTier | 'chip';

/** A named counter and its quantity, mirroring the protocol `Counter` shape. */
export interface CounterData {
  /** Counter name, e.g. `"+1/+1"` or `"loyalty"`. */
  kind: string;
  /** How many are present. Rendered verbatim — never summed into P/T. */
  count: number;
}

/**
 * The plain data a card display object is built from. This is a display
 * description, not a protocol type: `colorIdentity` is a token key the caller
 * chooses (deriving it from a `CardView` is a separate concern, see issue #36).
 */
export interface CardDisplayData {
  /** Display name, drawn in the header. */
  name: string;
  /** Full type line, e.g. `"Creature — Elf Warrior"`. */
  typeLine: string;
  /** Which palette entry frames the card. */
  colorIdentity: ColorIdentity;
  /** Displayed mana cost string, e.g. `"{1}{G}"`. Parsed into pips for display. */
  manaCost?: string;
  /** Displayed power, rendered exactly as provided (may be `"*"`). */
  power?: string;
  /** Displayed toughness, rendered exactly as provided. */
  toughness?: string;
  /** Counters, each rendered as its own chip. */
  counters?: CounterData[];
  /** Whether the permanent is tapped (rotated + dimmed). */
  tapped?: boolean;
  /** Whether the permanent has summoning sickness (chip + slight dim). */
  summoningSick?: boolean;
  /** Whether the card is currently selected (draws a selection ring). */
  selected?: boolean;
  /**
   * Whether the card is a legal target for the active target slot (ADR 0009
   * §Client). Draws a targeting ring in the shared targeting color. The caller
   * derives this purely from the server-listed candidates — no legality here.
   */
  targeting?: boolean;
  /**
   * Whether the card is dimmed and non-interactive because targeting mode is
   * active and it is NOT a legal target. Purely a display state driven by the
   * server's candidate list; reduces alpha so ineligible cards recede.
   */
  dimmed?: boolean;
  /**
   * Whether the card carries an offered action (issue #277) — a playable hand
   * card (`play_land`/`cast_spell`) or a permanent with an activatable ability.
   * Draws the always-on "playable" edge bar so an actionable card reads as
   * playable before any pointer interaction. Derived purely from
   * `RenderedCard.actions.length > 0` upstream; no legality here.
   */
  actionable?: boolean;
  /**
   * How many identical-state permanents this one render stands for (issue #318). A
   * value `> 1` draws an `×N` badge; the caller collapses only permanents whose full
   * display state is identical (tap state included), so the badge never hides a
   * differing card. Absent/`1` renders a single permanent with no badge.
   */
  stackCount?: number;
  /**
   * For a basic-land **chip** (issue #318), the basic-land glyph to draw in place of
   * a name (e.g. `'land-forest'`). Derived from the server type line by the caller;
   * absent for a nonbasic land (which shows its name) or any non-chip render.
   */
  landGlyph?: GlyphName;
  /**
   * Whether this permanent renders as the battlefield **land resource tile** —
   * the 1.45-aspect member of the frame family (card-representation §3.1, §4):
   * art only, no title bar, no cost, no type bar, with the mana glyph plate and
   * a name strip for a nonbasic or actionable land (§15.9).
   *
   * Supplied by the staging layer from the row it already sorted the permanent
   * into (`rowKindForType`), exactly like {@link landGlyph} — display glue over
   * the server's type line, never a rules computation, and never inferred by
   * the face itself. Absent everywhere but the battlefield: in hand, on the
   * stack, and in inspect a land is an ordinary portrait card.
   */
  landTile?: boolean;
  /**
   * The card's server-supplied keyword abilities as lowercase wire names (issue
   * #320) — e.g. `['flying', 'deathtouch']`. Rendered as a capped glyph strip at
   * support/field/hand tiers; a keyword with no glyph is dropped. Never derived here.
   */
  keywords?: string[];
  /**
   * Whether the permanent has a **latent activated ability** (issue #320) — drawn as
   * a quiet marker dot, distinct from the gold playable edge bar (which means "the
   * server is offering an action right now"). The dot says *latent*; both can appear
   * together. Supplied by the caller from view data; no rules here.
   */
  hasActivatedAbility?: boolean;
  /**
   * Marked combat damage on the permanent (issue #320/#332), drawn as a corner badge
   * when `> 0`. Rendered verbatim from the view's `damage` field — never computed or
   * predicted.
   */
  markedDamage?: number;
  /**
   * Whether this permanent is a **declared attacker** this combat (issue #332, CR
   * 508). Draws a bar on the *top* edge — deliberately a different edge from the gold
   * playable bar on the bottom, so an attacker reads as distinct from a merely tapped
   * or playable permanent without relying on hue. Derived purely from the view.
   */
  attacking?: boolean;
  /**
   * Whom this attacker is attacking (issue #341/#347): the defending player's `p{N}`
   * id, so a multi-opponent board can point the attacker's treatment toward that
   * player's area/HUD tile. Absent in a two-player game (the sole opponent is implied)
   * and for a non-attacker. Purely from the view (`Permanent.attacking_player`).
   */
  attackingPlayer?: string;
  /**
   * Whether this permanent is a **declared blocker** this combat (issue #332, CR 509).
   * Draws a bar on the *left* edge — a distinct edge again — marking it as defending.
   * Which attacker it blocks is carried by the scene's combat links, not the face.
   */
  blocking?: boolean;
  /**
   * How many blockers are assigned to this permanent as an attacker (issue #332): the
   * count of permanents whose `blocking` names this one. Draws a `blocked ×N` badge so
   * a defended attacker reads at a glance. `0`/absent for an unblocked or non-attacking
   * permanent. A pure count of server-supplied references — never a combat prediction.
   */
  blockedBy?: number;
  /**
   * Key of the card's currently-published illustration in the client-local art
   * store (ADR 0024), or absent for the procedural face. Opaque here: the renderer
   * only *looks up* the already-loaded texture — it never fetches, decodes, or
   * derives anything. The key changes when the art changes, so it rides the visual
   * signature and the reconciler rebuilds exactly on arrival.
   */
  artKey?: string;
}

/**
 * The explicit set of visual inputs the card face reads, serialized into a stable
 * key. Two cards with equal signatures render byte-identically, so the plane
 * reconciler may reuse (morph) one across frames instead of rebuilding it, and the
 * staging layer folds identical-looking permanents into one `×N` render.
 * **Position is deliberately absent**: a position-only change keeps the signature
 * and only moves the existing node.
 *
 * Keep this in lockstep with the fields the face actually reads — it is the single
 * definition of "same-looking card" for the reconcile/fold layer (issue #58). It is
 * a cache/fold key only, never load-bearing game state.
 */
export function cardVisualSignature(data: CardDisplayData, tier: RenderTier = 'field'): string {
  return JSON.stringify({
    tier,
    name: data.name,
    typeLine: data.typeLine,
    colorIdentity: data.colorIdentity,
    manaCost: data.manaCost ?? null,
    power: data.power ?? null,
    toughness: data.toughness ?? null,
    tapped: data.tapped ?? false,
    summoningSick: data.summoningSick ?? false,
    selected: data.selected ?? false,
    targeting: data.targeting ?? false,
    dimmed: data.dimmed ?? false,
    actionable: data.actionable ?? false,
    stackCount: data.stackCount ?? 1,
    landGlyph: data.landGlyph ?? null,
    landTile: data.landTile ?? false,
    keywords: data.keywords ?? [],
    hasActivatedAbility: data.hasActivatedAbility ?? false,
    markedDamage: data.markedDamage ?? 0,
    attacking: data.attacking ?? false,
    blocking: data.blocking ?? false,
    blockedBy: data.blockedBy ?? 0,
    counters: (data.counters ?? []).map((c) => [c.kind, c.count]),
    artKey: data.artKey ?? null,
  });
}

/** One parsed mana symbol ready to draw: the glyph plus its pip swatch. */
export interface ManaPip {
  /** The symbol as displayed inside the pip, e.g. `"1"` or `"G"`. */
  symbol: string;
  /** Pip disc fill color. */
  bg: string;
  /** Pip glyph color. */
  fg: string;
}

/**
 * Parse a displayed mana cost such as `"{1}{G}{G}"` into pips. This is pure
 * display formatting of the server-provided string — not a mana computation.
 *
 * The split and the swatch choice come from the shared symbol vocabulary
 * (issue #462, `chrome/symbols`), which is what keeps the cost row and the
 * inline symbols every DOM text surface draws from drifting apart. A code the
 * vocabulary does not know still gets a pip — on the neutral swatch, showing
 * the code verbatim — so nothing a server sends can vanish from a cost.
 */
export function parseManaCost(manaCost: string): ManaPip[] {
  return tokenizeNotation(manaCost).flatMap((token) => {
    if (token.kind === 'text') return [];
    if (token.kind === 'unknown') {
      return [{ symbol: token.code, bg: PIP.N.bg, fg: PIP.N.fg }];
    }
    return [{ symbol: token.caption, bg: token.swatch.bg, fg: token.swatch.fg }];
  });
}
