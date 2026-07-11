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
import { Container, Graphics, Text } from 'pixi.js';
import {
  BADGE,
  FONT,
  FRAME,
  PALETTE,
  PIP,
  PT_TEXT,
  SURFACES,
  TIER,
  type ColorIdentity,
} from '../tokens';

/** Tiers that render a full card face (chips are a separate digest representation). */
export type CardTier = 'support' | 'field' | 'hand';

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
export function cardVisualSignature(data: CardDisplayData, tier: CardTier = 'field'): string {
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

function mkText(str: string, size: number, color: string): Text {
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

  // Selection ring (drawn inside `inner` so it rotates with a tapped card).
  if (data.selected) {
    const ring = new Graphics();
    ring.lineStyle({
      width: FRAME.selectionWidth,
      color: hexToNumber(SURFACES.selection),
      alignment: 0,
    });
    ring.drawRoundedRect(-2, -2, t.w + 4, t.h + 4, FRAME.radius + 2);
    inner.addChild(ring);
  }

  // Tapped: rotate 90° about the card center and dim.
  inner.pivot.set(t.w / 2, t.h / 2);
  if (data.tapped) {
    inner.rotation = Math.PI / 2;
    inner.alpha = FRAME.tappedAlpha;
    inner.position.set(t.h / 2, t.w / 2);
  } else {
    inner.alpha = data.summoningSick ? FRAME.sickAlpha : 1;
    inner.position.set(t.w / 2, t.h / 2);
  }

  return outer;
}
