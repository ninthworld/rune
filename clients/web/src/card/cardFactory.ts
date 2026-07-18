/**
 * Pixi card factory (ADR 0003: battlefield/hand/stack cards live in the canvas).
 *
 * Builds a Pixi display object for a single card from a plain data description —
 * the fields a `CardView` carries (name, type line, mana cost, power/toughness,
 * counters) plus per-permanent display state (tapped, summoning sick, selected).
 *
 * Design rules this module obeys:
 * - **No game logic.** Effective power/toughness and counter counts are rendered
 *   exactly as supplied; the factory never adds, subtracts, or otherwise computes
 *   characteristics. Color identity is supplied by the caller, not derived here.
 * - **All colors and sizes come from `../tokens`.** Nothing about the card's look
 *   is inlined; both card renderers read the same constants.
 * - **No images, official frames, or WotC branding.** The frame is clean 2D vector
 *   graphics plus text.
 *
 * Reimplemented from the reference in `prototypes/ui-battlefield-v3.html` (never
 * imported — that file is a visual reference only).
 *
 * Rendering note: the factory estimates text extents from `FONT.charWidthRatio`
 * rather than measuring glyphs, so it constructs a full scene graph without a
 * live canvas/GPU. That keeps it deterministic and headless-testable.
 */
import { BitmapFont, BitmapText, Container, Graphics, Text } from 'pixi.js';
import {
  AFFORDANCE,
  BADGE,
  FONT,
  FRAME,
  INDICATORS,
  PALETTE,
  PIP,
  PT_TEXT,
  SURFACES,
  TAP,
  TIER,
  type ColorIdentity,
} from '../tokens';
import { buildGlyphDisplay, keywordGlyphName, type GlyphName } from '../chrome/glyphs';

/** Tiers that render a full card face (chips are a separate digest representation).
 * `mini` is the stepped-down dense tier the density ladder engages (blueprint). */
export type CardTier = 'mini' | 'support' | 'field' | 'hand';

/**
 * The full set of size tiers a battlefield object can render at, including the
 * digest **chip** (issue #318) used for lands at the back of a band. A chip is not
 * a full face — it renders through {@link buildChipDisplay}, not {@link buildCardDisplay}.
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
   * derives this purely from the server-listed candidates — the factory computes
   * no legality.
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
   * `RenderedCard.actions.length > 0` upstream; the factory computes no legality.
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
   * The card's server-supplied keyword abilities as lowercase wire names (issue
   * #320) — e.g. `['flying', 'deathtouch']`. Rendered as a capped glyph strip at
   * support/field/hand tiers; a keyword with no glyph is dropped (the #317 coverage
   * test guarantees the shipped set is covered). Never derived here.
   */
  keywords?: string[];
  /**
   * Whether the permanent has a **latent activated ability** (issue #320) — drawn as
   * a quiet marker dot, distinct from the gold playable edge bar (which means "the
   * server is offering an action right now"). The dot says *latent*; both can appear
   * together. Supplied by the caller from view data; the factory computes no rules.
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
   * or playable permanent without relying on hue. An attacker also keeps full opacity
   * rather than dimming when tapped, so it stands out from inert tapped lands. Derived
   * purely from the view; the factory computes no combat.
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
   * a defended attacker reads at a glance and several blockers on one attacker stay
   * legible. `0`/absent for an unblocked or non-attacking permanent. A pure count of
   * server-supplied references — never a combat prediction.
   */
  blockedBy?: number;
}

/**
 * The explicit set of visual inputs {@link buildCardDisplay} reads, serialized
 * into a stable key. Two cards with equal signatures build byte-identical display
 * objects, so a reconciler may reuse one across frames instead of rebuilding its
 * ~10 Graphics/Text nodes. **Position is deliberately absent**: a position-only
 * change keeps the signature and only moves the existing container.
 *
 * Keep this in lockstep with the fields {@link buildCardDisplay} actually reads —
 * it is the single definition of "same-looking card" for the reconcile layer
 * (issue #58). It is a cache key only, never load-bearing game state.
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
    keywords: data.keywords ?? [],
    hasActivatedAbility: data.hasActivatedAbility ?? false,
    markedDamage: data.markedDamage ?? 0,
    attacking: data.attacking ?? false,
    blocking: data.blocking ?? false,
    blockedBy: data.blockedBy ?? 0,
    counters: (data.counters ?? []).map((c) => [c.kind, c.count]),
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

/** `'#RRGGBB'` token string to the numeric color Pixi expects. */
function hexToNumber(hex: string): number {
  return parseInt(hex.slice(1), 16);
}

/** Average glyph advance in px for a string at the given font size. */
function estTextWidth(text: string, fontSize: number): number {
  return text.length * fontSize * FONT.charWidthRatio;
}

/**
 * Parse a displayed mana cost such as `"{1}{G}{G}"` into pips. This is pure
 * display formatting of the server-provided string — not a mana computation.
 */
export function parseManaCost(manaCost: string): ManaPip[] {
  const symbols = manaCost.match(/\{([^}]+)\}/g) ?? [];
  return symbols.map((raw) => {
    const symbol = raw.slice(1, -1);
    const swatch = symbol in PIP ? PIP[symbol as keyof typeof PIP] : PIP.N;
    return { symbol, bg: swatch.bg, fg: swatch.fg };
  });
}

/**
 * A card text node. In a browser this is a {@link BitmapText} drawing from the
 * shared cached glyph atlas; where that atlas cannot be rasterized (headless
 * tests, SSR) it degrades to a plain {@link Text}. Both expose the `position`,
 * `anchor`, and `alpha` the factory sets.
 */
type CardText = Text | BitmapText;

/** The glyphs the card atlas must cover: printable ASCII, Latin-1 + Extended-A
 * (accented names, `×`), and the general-punctuation dashes/quotes/bullet/`…`. */
const CARD_FONT_CHARS: string[][] = [
  [' ', '~'],
  [' ', 'ɏ'],
  ['‐', '‧'],
];

/**
 * Memoized outcome of installing the shared card {@link BitmapFont}: the font
 * name once installed, `null` if rasterization is unavailable, `undefined` until
 * first attempted.
 */
let cardFontName: string | null | undefined;

/**
 * Lazily install (once) the shared white {@link BitmapFont} the factory draws all
 * card text from, memoizing the result. Building the glyph atlas needs a real 2D
 * canvas; where none exists (headless CI, SSR) `BitmapFont.from` throws and we
 * memoize `null` so {@link mkText} falls back to plain `Text`. The atlas is white
 * so each {@link BitmapText} is tinted to its token color at draw time — glyphs
 * are rasterized ONCE here rather than per card construction (the old per-build
 * `Text` at `resolution: 2` re-rasterized a fresh texture every time).
 */
function ensureCardFont(): string | null {
  if (cardFontName !== undefined) return cardFontName;
  try {
    BitmapFont.from(
      FONT.bitmapName,
      {
        fontFamily: FONT.family,
        fontSize: FONT.bitmapBaseSize,
        fontWeight: FONT.weight,
        fill: '#ffffff',
      },
      { chars: CARD_FONT_CHARS, resolution: 2 },
    );
    cardFontName = FONT.bitmapName;
  } catch {
    cardFontName = null;
  }
  return cardFontName;
}

/**
 * Build one text node. Prefers a cached {@link BitmapText} (reusing the shared
 * atlas' glyph textures, tinted to `color`) and falls back to a plain `Text` only
 * where no atlas could be rasterized. The two paths are visually equivalent; which
 * one is used depends solely on the environment, never on the card's data, so the
 * factory stays deterministic for a given environment.
 */
function mkText(str: string, size: number, color: string): CardText {
  const fontName = ensureCardFont();
  if (fontName) {
    const text = new BitmapText(str, { fontName, fontSize: size });
    text.tint = hexToNumber(color);
    return text;
  }
  const text = new Text(str, {
    fontFamily: FONT.family,
    fontSize: size,
    fill: color,
    fontWeight: FONT.weight,
  });
  text.resolution = 2;
  return text;
}

/** Truncate a name to what fits `maxWidth` px, appending an ellipsis. */
function fitName(name: string, maxWidth: number, fontSize: number): string {
  const maxChars = Math.max(2, Math.floor(maxWidth / (fontSize * FONT.charWidthRatio)));
  if (name.length <= maxChars) return name;
  return `${name.slice(0, Math.max(1, maxChars - 1))}…`;
}

/** A rounded label chip (P/T pill or counter/state badge). */
function makePill(
  label: string,
  fontSize: number,
  bg: string,
  fg: string,
  radius: number,
): Container {
  const wrap = new Container();
  const width = estTextWidth(label, fontSize) + 12;
  const height = fontSize + 3;
  const g = new Graphics();
  g.beginFill(hexToNumber(bg));
  g.drawRoundedRect(0, 0, width, height, radius);
  g.endFill();
  const tx = mkText(label, fontSize, fg);
  tx.position.set(6, 1.5);
  wrap.addChild(g, tx);
  return wrap;
}

/**
 * Build the display object for one card at the given tier. Returns a `Container`
 * whose child (`inner`) holds the frame and content; tapping rotates `inner` so
 * callers can position the outer container without recomputing layout.
 */
export function buildCardDisplay(data: CardDisplayData, tier: CardTier = 'field'): Container {
  const t = TIER[tier];
  const accent = PALETTE[data.colorIdentity];
  const accentNum = hexToNumber(accent);

  const outer = new Container();
  const inner = new Container();
  outer.addChild(inner);

  // Frame: bordered body with a tinted header band. Pure vector, no art.
  const frame = new Graphics();
  frame.lineStyle({ width: FRAME.borderWidth, color: accentNum, alpha: 1, alignment: 1 });
  frame.beginFill(hexToNumber(SURFACES.cardBody));
  frame.drawRoundedRect(0, 0, t.w, t.h, FRAME.radius);
  frame.endFill();
  frame.lineStyle(0);
  frame.beginFill(accentNum, FRAME.headerTintAlpha);
  frame.drawRoundedRect(3, 3, t.w - 6, t.header, FRAME.headerRadius);
  frame.endFill();
  inner.addChild(frame);

  // Name (top-left, truncated to fit the header width).
  const name = mkText(fitName(data.name, t.w - 14, t.name), t.name, SURFACES.nameText);
  name.position.set(7, 7);
  inner.addChild(name);

  // Mana cost pips beneath the name.
  if (data.manaCost) {
    parseManaCost(data.manaCost).forEach((mp, i) => {
      const pip = new Graphics();
      pip.beginFill(hexToNumber(mp.bg));
      pip.drawCircle(0, 0, t.pip / 2 + 1);
      pip.endFill();
      pip.position.set(7 + t.pip / 2 + i * (t.pip + 4), 7 + t.name + 12);
      const glyph = mkText(mp.symbol, 11, mp.fg);
      glyph.anchor.set(0.5);
      pip.addChild(glyph);
      inner.addChild(pip);
    });
  }

  // Center monogram (first letter of the name) as a faint watermark.
  const monogram = mkText(data.name.slice(0, 1), t.mono, accent);
  monogram.alpha = FRAME.monogramAlpha;
  monogram.anchor.set(0.5);
  monogram.position.set(t.w / 2, (t.h + t.header) / 2);
  inner.addChild(monogram);

  // Type line above the bottom edge.
  const typeLine = mkText(fitName(data.typeLine, t.w - 12, t.type), t.type, SURFACES.typeText);
  typeLine.position.set(6, t.h - t.type - 20);
  inner.addChild(typeLine);

  // Keyword-glyph strip (issue #320): a quiet row just above the type line, from the
  // server-supplied keywords only (no rules derivation). Capped to what fits at this
  // tier; the overflow degrades to a "+N" tag rather than shrinking glyphs below
  // recognition (ui-requirements §10 — an illegible strip is worse than none).
  const keywordGlyphs = (data.keywords ?? [])
    .map(keywordGlyphName)
    .filter((n): n is GlyphName => n !== null);
  if (keywordGlyphs.length > 0) {
    const gsize = t.pip;
    const gap = 3;
    const stripY = t.h - t.type - 20 - gsize - 3;
    const capacity = Math.max(1, Math.floor((t.w - 14) / (gsize + gap)));
    const overflow = keywordGlyphs.length > capacity;
    const shown = overflow ? capacity - 1 : keywordGlyphs.length;
    let gx = 7;
    for (let i = 0; i < shown; i++) {
      const g = buildGlyphDisplay(keywordGlyphs[i]!, { size: gsize, color: INDICATORS.keyword });
      g.position.set(gx, stripY);
      inner.addChild(g);
      gx += gsize + gap;
    }
    if (overflow) {
      const moreSize = Math.max(9, t.type);
      const more = mkText(`+${keywordGlyphs.length - shown}`, moreSize, INDICATORS.keyword);
      more.position.set(gx, stripY + (gsize - moreSize) / 2);
      inner.addChild(more);
    }
  }

  // Latent activated-ability marker (issue #320): a quiet dot, deliberately a
  // different *shape* from the gold playable edge bar — the dot says the permanent
  // *has* an activated ability, the bar says the server is offering one *right now*.
  // Both can appear together, so the dot sits above the bar, clear of the P/T pill.
  if (data.hasActivatedAbility) {
    const dot = new Graphics();
    dot.beginFill(hexToNumber(INDICATORS.abilityMarker));
    dot.drawCircle(9, t.h - 13, 3);
    dot.endFill();
    inner.addChild(dot);
  }

  // Power/toughness pill (bottom-right). Rendered exactly as provided.
  if (data.power !== undefined && data.toughness !== undefined) {
    const label = `${data.power}/${data.toughness}`;
    const width = estTextWidth(label, t.name) + 12;
    const height = t.name + 3;
    const pill = makePill(label, t.name, accent, PT_TEXT[data.colorIdentity], 5);
    pill.position.set(t.w - width - 5, t.h - height - 5);
    inner.addChild(pill);
  }

  // Corner badges (top-right): one per counter, then a summoning-sick marker.
  let badgeX = t.w - 8;
  const addBadge = (label: string, bg: string, fg: string) => {
    const width = estTextWidth(label, 11) + 12;
    const badge = makePill(label, 11, bg, fg, 8);
    badge.position.set(badgeX - width + 8, -8);
    badgeX -= width + 4;
    inner.addChild(badge);
  };
  (data.counters ?? []).forEach((counter) => {
    const label = counter.count === 1 ? counter.kind : `${counter.kind} ×${counter.count}`;
    addBadge(label, BADGE.counterBg, BADGE.counterText);
  });
  if (data.summoningSick) {
    addBadge('zz', BADGE.bg, BADGE.text);
  }
  // Marked-damage badge (issue #320/#332): rendered verbatim from view data.
  if ((data.markedDamage ?? 0) > 0) {
    addBadge(`${data.markedDamage} dmg`, INDICATORS.damageBg, INDICATORS.damageText);
  }
  // Blocked-by badge (issue #332): how many blockers this attacker faces, so a
  // defended attacker reads at a glance. A pure count of server references.
  if ((data.blockedBy ?? 0) > 0) {
    addBadge(`blocked ×${data.blockedBy}`, INDICATORS.blockingBar, BADGE.text);
  }
  // Stacking badge (issue #318): identical-state permanents collapse to one render
  // carrying an `×N`. The caller groups only on full state identity, so the badge
  // never hides a differing card.
  if ((data.stackCount ?? 1) > 1) {
    addBadge(`×${data.stackCount}`, BADGE.bg, BADGE.text);
  }

  // Combat-declaration bars (issue #332): an attacker wears a bar on the TOP edge,
  // a blocker one on the LEFT edge — each a different edge from the gold playable bar
  // (bottom) and from the full-perimeter rings, so combat state stays distinct by
  // shape alone (ui-requirements §Combat / §10). Inside `inner`, so they rotate with a
  // tapped attacker. Rendered straight from the view — the client declares no combat.
  if (data.attacking) {
    const bar = new Graphics();
    bar.beginFill(hexToNumber(INDICATORS.attackingBar));
    bar.drawRoundedRect(2, 0, t.w - 4, AFFORDANCE.edgeHeight, AFFORDANCE.edgeHeight / 2);
    bar.endFill();
    inner.addChild(bar);
  }
  if (data.blocking) {
    const bar = new Graphics();
    bar.beginFill(hexToNumber(INDICATORS.blockingBar));
    bar.drawRoundedRect(0, 2, AFFORDANCE.edgeHeight, t.h - 4, AFFORDANCE.edgeHeight / 2);
    bar.endFill();
    inner.addChild(bar);
  }

  // Playable affordance (issue #277): an always-on solid bar hugging the bottom
  // edge whenever the card carries an offered action. Deliberately a different
  // *shape* than the full-perimeter selection/targeting rings below (and drawn
  // under them, so selection still reads on top of a playable card), so it stays
  // distinguishable without color vision (ui-requirements §10). Inside `inner`,
  // so it rotates with a tapped-but-activatable permanent.
  if (data.actionable) {
    const edge = new Graphics();
    edge.beginFill(hexToNumber(AFFORDANCE.actionable));
    edge.drawRoundedRect(
      2,
      t.h - AFFORDANCE.edgeHeight,
      t.w - 4,
      AFFORDANCE.edgeHeight,
      AFFORDANCE.edgeHeight / 2,
    );
    edge.endFill();
    inner.addChild(edge);
  }

  // Selection / targeting ring (drawn inside `inner` so it rotates with a tapped
  // card). A legal target draws a ring in the shared targeting color; otherwise a
  // selected card draws the selection ring. The candidate set is server-supplied,
  // so this reflects legality without ever computing it.
  const ringColor = data.targeting ? SURFACES.targeting : data.selected ? SURFACES.selection : null;
  if (ringColor !== null) {
    const ring = new Graphics();
    ring.lineStyle({
      width: FRAME.selectionWidth,
      color: hexToNumber(ringColor),
      alignment: 0,
    });
    ring.drawRoundedRect(-2, -2, t.w + 4, t.h + 4, FRAME.radius + 2);
    inner.addChild(ring);
  }

  // Tapped: ONE treatment at every tier (blueprint §Card vocabulary) — a partial
  // ~25° rotation about the card center plus a slight dim, centered inside the
  // rotated bounding box the scene reserved. A dimmed card (ineligible during
  // targeting mode) recedes further, multiplying its base alpha. A declared
  // attacker keeps full opacity even while tapped (issue #332) — it is actively in
  // combat, not inert like a tapped land — so it never recedes below its neighbors.
  const baseAlpha =
    data.tapped && !data.attacking ? FRAME.tappedAlpha : data.summoningSick ? FRAME.sickAlpha : 1;
  inner.alpha = data.dimmed ? baseAlpha * FRAME.dimmedAlpha : baseAlpha;
  applyTapTransform(inner, t.w, t.h, data.tapped ?? false);

  return outer;
}

/**
 * Center a card's `inner` container in its reserved footprint, rotating it by the
 * shared {@link TAP.angle} when tapped. The footprint is the rotated bounding box
 * (`w·cosθ + h·sinθ` × `w·sinθ + h·cosθ`) the scene reserves for a tapped card, so
 * drawn pixels always match the reported rect at every tier — chips included.
 */
function applyTapTransform(inner: Container, w: number, h: number, tapped: boolean): void {
  inner.pivot.set(w / 2, h / 2);
  if (tapped) {
    const c = Math.cos(TAP.angle);
    const s = Math.sin(TAP.angle);
    inner.rotation = TAP.angle;
    inner.position.set((w * c + h * s) / 2, (w * s + h * c) / 2);
  } else {
    inner.position.set(w / 2, h / 2);
  }
}

/**
 * Build the digest **chip** for a land at the back of a band (issue #318). Chips are
 * the smallest tier (`TIER.chip`, 44×60): the information budget is frame color, a
 * name **or** a basic-land glyph, and tap state — nothing else (see
 * `docs/design/ui-design-notes.md` §Card render). Tap is the same partial-rotation
 * treatment as every other tier (blueprint §Card vocabulary); the caller reserves
 * the rotated bounding box accordingly.
 *
 * Basic lands draw their glyph ({@link CardDisplayData.landGlyph}); nonbasics draw a
 * truncated name. Selection/targeting rings, the playable edge bar, and the `×N`
 * stack badge all read the same as on a full face so a chip stays a first-class,
 * selectable object. No game logic: every input is server-supplied display state.
 */
export function buildChipDisplay(data: CardDisplayData): Container {
  const t = TIER.chip;
  const accent = PALETTE[data.colorIdentity];
  const accentNum = hexToNumber(accent);

  const outer = new Container();
  const inner = new Container();
  outer.addChild(inner);

  // Frame: bordered body with a faint accent header tint, mirroring the full face.
  const frame = new Graphics();
  frame.lineStyle({ width: FRAME.borderWidth, color: accentNum, alpha: 1, alignment: 1 });
  frame.beginFill(hexToNumber(SURFACES.cardBody));
  frame.drawRoundedRect(0, 0, t.w, t.h, FRAME.chipRadius);
  frame.endFill();
  inner.addChild(frame);

  if (data.landGlyph) {
    // Basic land: the glyph carries identity in place of a name.
    const glyph = buildGlyphDisplay(data.landGlyph, { size: 26, color: accent });
    glyph.position.set((t.w - 26) / 2, (t.h - 26) / 2);
    inner.addChild(glyph);
  } else {
    // Nonbasic land (or any named chip): a small truncated name, centered.
    const label = fitName(data.name, t.w - 8, 9);
    const name = mkText(label, 9, SURFACES.nameText);
    name.anchor.set(0.5);
    name.position.set(t.w / 2, t.h / 2);
    inner.addChild(name);
  }

  // Playable edge bar (issue #277) — same shape/meaning as the full face.
  if (data.actionable) {
    const edge = new Graphics();
    edge.beginFill(hexToNumber(AFFORDANCE.actionable));
    edge.drawRoundedRect(
      2,
      t.h - AFFORDANCE.edgeHeight,
      t.w - 4,
      AFFORDANCE.edgeHeight,
      AFFORDANCE.edgeHeight / 2,
    );
    edge.endFill();
    inner.addChild(edge);
  }

  // Stack badge (issue #318): identical tapped/untapped chips collapse to one ×N.
  if ((data.stackCount ?? 1) > 1) {
    const badge = makePill(`×${data.stackCount}`, 10, BADGE.bg, BADGE.text, 7);
    const width = estTextWidth(`×${data.stackCount}`, 10) + 12;
    badge.position.set(t.w - width - 1, -6);
    inner.addChild(badge);
  }

  // Selection / targeting ring, same channels as a full face.
  const ringColor = data.targeting ? SURFACES.targeting : data.selected ? SURFACES.selection : null;
  if (ringColor !== null) {
    const ring = new Graphics();
    ring.lineStyle({ width: FRAME.selectionWidth, color: hexToNumber(ringColor), alignment: 0 });
    ring.drawRoundedRect(-2, -2, t.w + 4, t.h + 4, FRAME.chipRadius + 2);
    inner.addChild(ring);
  }

  // Tapped: the same ~25° rotation + slight dim as every other tier (blueprint:
  // ONE tap treatment everywhere — partial rotation keeps even chips legible).
  const baseAlpha = data.tapped ? FRAME.tappedAlpha : 1;
  inner.alpha = data.dimmed ? baseAlpha * FRAME.dimmedAlpha : baseAlpha;
  applyTapTransform(inner, t.w, t.h, data.tapped ?? false);

  return outer;
}
