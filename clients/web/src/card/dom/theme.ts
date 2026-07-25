/**
 * Card-face geometry and paint, resolved once from `src/tokens.ts`.
 *
 * This module is the single implementation of the Rune frame's measured model
 * (`docs/design/card-representation.md`, issue #538 — the binding specification
 * for issue #529):
 *
 * - **Two silhouettes, one family** (§2, §3.1). A battlefield permanent is a
 *   1.00 **square plaque**; a card in hand / on the stack / in inspect is a
 *   0.715 portrait card; a land is a 1.45 **resource tile**. The square is the
 *   portrait frame with its rules area structurally removed, so both resolve
 *   from the same band ratios.
 * - **The battlefield face carries no mana cost and no type bar** (§3.3) — a
 *   normative rule of the family, transcribed from all three approved
 *   baselines, not a truncation.
 * - **The floor rule** (§8.4). Ratios are authored at `Wref = 190`; below that,
 *   type clamps to the `RUNE_TYPE` px floors, the band that holds it **grows**,
 *   and the art window absorbs the difference. Implemented here exactly once so
 *   no surface re-derives it. Where a floored name would claim the art window
 *   rather than absorb from it, the title bar gives way to the
 *   **colour-identity strip** ({@link drawsIdentityStrip}) — `mini` — and then
 *   to nothing at all — `chip`.
 * - **No literal.** Every color and length below comes from a token, and
 *   reaches the stylesheet as a CSS custom property (ADR 0019).
 *
 * {@link faceMetrics} is also the export surface the plane geometry consumes:
 * a card's reserved cell is `faceFootprint(tier, tapped, kind)` and nothing
 * else — never an image, never a tier's old hard-coded pair.
 */
import type { CSSProperties } from 'react';
import {
  AFFORDANCE,
  ART,
  BADGE,
  FRAME,
  INDICATORS,
  PALETTE,
  RUNE_BANDS_FULL,
  RUNE_BANDS_PERM,
  RUNE_FRAME,
  RUNE_GOLD,
  RUNE_TYPE,
  SPLAY,
  SURFACES,
  TAP,
  TIER,
} from '../../tokens';
import type { CardDisplayData } from '../cardFactory';

/**
 * The size tiers the DOM card face renders (card-representation §8.1). Four
 * battlefield tiers on the perspective plane, three screen-space tiers: the
 * hand fan, the stack rail, and the fixed reading surface.
 */
export type CardFaceTier = 'chip' | 'mini' | 'support' | 'field' | 'hand' | 'stack' | 'inspect';

/**
 * Which member of the frame family a face draws (card-representation §3.1/§4).
 * `permanent` is the 1.00 square plaque, `land` the 1.45 resource tile, `card`
 * the 0.715 portrait card.
 */
export type CardSurfaceKind = 'permanent' | 'land' | 'card';

/** The tiers that render on the battlefield plane — the ≤ 12-node budget binds
 * on exactly these (presentation-budgets §Performance). */
export const BATTLEFIELD_TIERS: readonly CardFaceTier[] = ['chip', 'mini', 'support', 'field'];

/** The screen-space tiers: budget-exempt reading surfaces at their own scale. */
export const SCREEN_TIERS: readonly CardFaceTier[] = ['hand', 'stack', 'inspect'];

/** Whether a tier stages on the perspective plane (and so wears the budget). */
export function isBattlefieldTier(tier: CardFaceTier): boolean {
  return BATTLEFIELD_TIERS.includes(tier);
}

/**
 * Which silhouette a tier draws. A land renders as the resource tile only on
 * the battlefield; in hand, on the stack, and in inspect it is an ordinary
 * portrait card (§4, §11 — the tile *grows into* the full card on focus).
 */
export function surfaceKindFor(tier: CardFaceTier, landTile = false): CardSurfaceKind {
  if (!isBattlefieldTier(tier)) return 'card';
  return landTile ? 'land' : 'permanent';
}

/**
 * Whether a battlefield permanent draws the **colour-identity strip** in place
 * of the parchment title plate (card-representation §8.4).
 *
 * `mini` does. At W = 62 the 11 px name floor grows the title bar to 14.9 px
 * and the 12 px P/T floor grows the status band to 16.2 px — together about
 * **half** the card's height, against an authored art ratio of 0.647. The tier
 * would be mostly type, which is the opposite of what it is for, so the `chip`
 * treatment applies one rung earlier: the band survives as a strip of the
 * card's identity accent at its **authored** height, carrying no text and
 * therefore taking no floor, and the art window keeps the difference. Identity
 * moves to the accent + glyph + inspect path, exactly as at `chip`.
 *
 * `chip` is the next rung down the same ladder: at W = 48 the band is dropped
 * altogether (its name resolves to `0`), and the art window's own 100% identity
 * rule (§3.4) plus the type/land glyph carry the accent. Every tier above
 * `mini` keeps the parchment name plate.
 */
export function drawsIdentityStrip(tier: CardFaceTier, kind: CardSurfaceKind): boolean {
  return kind === 'permanent' && tier === 'mini';
}

/**
 * PROVISIONAL presentation seeds (issue #479) — the values issue #480 (the
 * visual-system token pass) owns. Collected here as the single swap point so
 * the component itself never carries a literal. Values seed from
 * `docs/design/visual-system.md` §3 (elevation ladder) and the animation
 * budgets (`presentation-budgets.md` §Animation — micro feedback 80–150 ms).
 */
export const PROVISIONAL = {
  /** Elevation ladder lifts toward the camera, logical px (visual-system §3). */
  lift: { rest: 0, lifted: 24, held: 34 },
  /** Elevation shadows: contact → soft spread → widest (single implied key light). */
  shadow: {
    rest: '0 2px 4px rgba(0, 0, 0, 0.45)',
    lifted: '0 10px 18px rgba(0, 0, 0, 0.4)',
    held: '0 16px 28px rgba(0, 0, 0, 0.38)',
  },
  /** Micro-feedback transition duration (budget class 80–150 ms). */
  microMs: 120,
} as const;

/** Line box a clamped type size occupies inside its band (card-representation §8.4). */
const LINE = 1.35;

/** The px offsets of one band, top-anchored inside the card box. */
export interface FaceBand {
  /** Distance from the card's top edge to the band's top, px. */
  top: number;
  /** Band height, px. */
  h: number;
}

/**
 * A resolved face: the drawn box, the band stack, the frame lengths, and the
 * clamped type sizes. Everything the face and the plane need, in logical px.
 */
export interface FaceMetrics {
  /** The tier this was resolved for. */
  tier: CardFaceTier;
  /** Which silhouette of the frame family. */
  kind: CardSurfaceKind;
  /** Drawn card width, px. */
  w: number;
  /** Drawn card height, px. */
  h: number;
  /** Clamped card-name size, px (`0` where the tier draws no name). */
  name: number;
  /** Clamped type-line size, px (`0` on every battlefield tier — §3.3). */
  typeLine: number;
  /** Clamped rules size, px (`0` on every battlefield tier — §3.3). */
  rules: number;
  /** Clamped P/T size, px — a critical value, never below `floorValue`. */
  pt: number;
  /** Cost-disc numeral size, px (drawn on the full card only). */
  cost: number;
  /** Top-edge tab text size, px (`TOKEN`, `×N`). */
  tab: number;
  /** Badge text size, px. */
  badge: number;
  /** Procedural art-window monogram size, px. */
  monogram: number;
  /** Slate edge on left/right/top, px. */
  edge: number;
  /** The bottom paper-thickness edge, px. */
  edgeBottom: number;
  /** Gold hairline weight, px (never sub-pixel). */
  rule: number;
  /** Gold hairline inset from the card edge, px. */
  ruleInset: number;
  /** Outer corner radius, px. */
  radius: number;
  /** Plate / art-window corner radius, px. */
  plateRadius: number;
  /** The band stack, top to bottom. A zero-height band is not drawn. */
  bands: {
    /** Title bar — name, plus the cost disc on a full card. */
    title: FaceBand;
    /** Art window — the dominant band at every tier. */
    art: FaceBand;
    /** Type bar — full card only. */
    type: FaceBand;
    /** Rules area — full card only. */
    rules: FaceBand;
    /** Status band — battlefield permanent only: glyph plates + P/T plate. */
    status: FaceBand;
  };
}

/** The authored (unclamped) px size of a `RUNE_TYPE` ratio at width `W`. */
export function authoredTypeSize(ratio: number, w: number): number {
  return ratio * w;
}

/** Clamp an authored size to a floor; `0` means "this tier does not draw it". */
function clamp(size: number, floor: number): number {
  return size === 0 ? 0 : Math.max(floor, size);
}

const EMPTY_BAND: FaceBand = { top: 0, h: 0 };

/**
 * Resolve a tier + silhouette into its drawn box, band stack, and clamped type
 * sizes — the whole of card-representation §3.2, §3.3, §8.1 and §8.4 in one
 * pure function.
 */
export function faceMetrics(tier: CardFaceTier, kind?: CardSurfaceKind): FaceMetrics {
  const t = TIER[tier];
  const resolved = kind ?? surfaceKindFor(tier);
  const w = t.w;
  const h = resolved === 'land' ? t.landH : t.h;

  const edge = RUNE_FRAME.edge * w;
  const edgeBottom = RUNE_FRAME.edgeBottom * w;
  const rule = Math.max(1, RUNE_FRAME.rule * w);

  const name = clamp(t.name, RUNE_TYPE.floorName);
  const typeLine = clamp(t.type, RUNE_TYPE.floorName);
  const rules = clamp(t.rules, RUNE_TYPE.floorName);
  const pt = clamp(t.pt, RUNE_TYPE.floorValue);

  const base = {
    tier,
    kind: resolved,
    w,
    h,
    name,
    typeLine,
    rules,
    pt,
    cost: Math.max(RUNE_TYPE.floorValue, authoredTypeSize(RUNE_TYPE.cost, w)),
    tab: Math.max(RUNE_TYPE.floorValue, authoredTypeSize(RUNE_TYPE.tab, w)),
    badge: Math.max(RUNE_TYPE.floorValue, authoredTypeSize(RUNE_TYPE.badge, w)),
    monogram: RUNE_FRAME.monogram * w,
    edge,
    edgeBottom,
    rule,
    ruleInset: RUNE_FRAME.ruleInset * w,
    radius: RUNE_FRAME.radius * w,
    plateRadius: RUNE_FRAME.plateRadius * w,
  };

  if (resolved === 'card') {
    // Full card: title, art, type, rules — bottom-anchored from the rules area
    // so the art window is the band that absorbs every clamp (§8.4).
    const titleH = Math.max(RUNE_BANDS_FULL.title * h, name * LINE);
    const typeH = Math.max(RUNE_BANDS_FULL.type * h, typeLine * LINE);
    const rulesH = Math.max(RUNE_BANDS_FULL.rules * h, rules * LINE);
    const titleTop = edge + rule;
    const artTop = titleTop + titleH + rule;
    const rulesTop = h - edgeBottom - rulesH;
    const typeTop = rulesTop - rule - typeH;
    return {
      ...base,
      bands: {
        title: { top: titleTop, h: titleH },
        art: { top: artTop, h: Math.max(0, typeTop - rule - artTop) },
        type: { top: typeTop, h: typeH },
        rules: { top: rulesTop, h: rulesH },
        status: EMPTY_BAND,
      },
    };
  }

  if (resolved === 'land') {
    // Resource tile: art only, framed. A nonbasic or actionable land gets a
    // bottom name strip (§15.9); the face decides that and the strip reuses
    // the title band's box, anchored to the bottom.
    const stripH = Math.max(RUNE_BANDS_PERM.title * h, name * LINE);
    const artTop = edge + rule;
    return {
      ...base,
      bands: {
        title: { top: h - edgeBottom - stripH, h: stripH },
        art: { top: artTop, h: Math.max(0, h - edgeBottom - artTop) },
        type: EMPTY_BAND,
        rules: EMPTY_BAND,
        status: EMPTY_BAND,
      },
    };
  }

  // Battlefield permanent: title, art, status.
  //
  // The title band holds the parchment name plate only where the name clears
  // its px floor without eating the art window (§8.4). At `mini` it is the
  // **colour-identity strip** instead — the same band box at its authored
  // height, with no text and so no floor to grow it — and at `chip` the band is
  // dropped entirely, the name having resolved to `0`. On both rungs the face
  // draws no name at all, so the clamped size is published as `0`.
  const strip = drawsIdentityStrip(tier, resolved);
  const titleH = strip
    ? RUNE_BANDS_PERM.title * h
    : name === 0
      ? 0
      : Math.max(RUNE_BANDS_PERM.title * h, name * LINE);
  const statusH = Math.max(RUNE_BANDS_PERM.status * h, pt * LINE);
  const titleTop = edge + rule;
  const artTop = titleH === 0 ? titleTop : titleTop + titleH + rule;
  const statusTop = h - edgeBottom - statusH;
  return {
    ...base,
    name: strip ? 0 : name,
    bands: {
      title: { top: titleTop, h: titleH },
      art: { top: artTop, h: Math.max(0, statusTop - rule - artTop) },
      type: EMPTY_BAND,
      rules: EMPTY_BAND,
      status: { top: statusTop, h: statusH },
    },
  };
}

/**
 * The reserved footprint of a face: the rotated bounding box when tapped (the
 * carried rule — rotation must reserve its swept box so drawn pixels always
 * match the reported rect; same math as the scene's `tappedFootprint`).
 *
 * This is the function the plane geometry stages against. A square permanent
 * and a wide land tile at the same tier have DIFFERENT footprints, so the
 * silhouette has to be passed in — `cellSize` cannot key on the tier alone.
 */
export function faceFootprint(
  tier: CardFaceTier,
  tapped: boolean,
  kind?: CardSurfaceKind,
): { w: number; h: number } {
  const m = faceMetrics(tier, kind);
  if (!tapped) return { w: m.w, h: m.h };
  const c = Math.cos(TAP.angle);
  const s = Math.sin(TAP.angle);
  return { w: Math.round(m.w * c + m.h * s), h: Math.round(m.w * s + m.h * c) };
}

/**
 * Every drawn box the card layer publishes, tier × silhouette — the table the
 * plane's slot geometry (issue #531) reserves from. Derived from
 * {@link faceMetrics}, so it can never drift from what the face draws.
 */
export const CARD_BOX: Record<
  CardFaceTier,
  Record<CardSurfaceKind, { w: number; h: number }>
> = Object.fromEntries(
  (['chip', 'mini', 'support', 'field', 'hand', 'stack', 'inspect'] as CardFaceTier[]).map(
    (tier) => [
      tier,
      Object.fromEntries(
        (['permanent', 'land', 'card'] as CardSurfaceKind[]).map((kind) => {
          const m = faceMetrics(tier, kind);
          return [kind, { w: m.w, h: m.h }];
        }),
      ),
    ],
  ),
) as Record<CardFaceTier, Record<CardSurfaceKind, { w: number; h: number }>>;

/**
 * How many card edges the pile draws behind the top card for a fold of
 * `stackCount` members: one per hidden member, capped at {@link SPLAY.maxLayers}
 * so a 40-token fold keeps the same bounded silhouette as a 4-Plains fold (the
 * `×N` tab carries the exact count).
 */
export function splayLayers(stackCount: number | undefined): number {
  const hidden = (stackCount ?? 1) - 1;
  return hidden <= 0 ? 0 : Math.min(hidden, SPLAY.maxLayers);
}

/**
 * The layered box-shadow that draws a fold as a physical pile
 * (card-representation §6.1 panel 7, §15.3): each hidden card contributes a
 * body fill offset **down-and-left** by (`SPLAY.stepX` · W, `SPLAY.stepY` · H)
 * plus its accent edge one step further out, so depth reads as paper rather
 * than arithmetic and the card's right edge stays clear for the P/T plate.
 * Zero elements at any count — the whole pile is one `box-shadow`.
 */
export function splayShadow(layers: number, w: number, h: number): string {
  const parts: string[] = [];
  const dx = SPLAY.stepX * w;
  const dy = SPLAY.stepY * h;
  for (let i = 1; i <= layers; i += 1) {
    const x = round(i * dx);
    const y = round(i * dy);
    parts.push(`${-x}px ${y}px 0 0 var(--face-body)`);
    parts.push(`${-x}px ${y}px 0 ${SPLAY.edgePx}px var(--face-accent)`);
  }
  return parts.join(', ');
}

/** Round a resolved length to 2dp — enough for CSS, stable for snapshots. */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}

/** Publish a resolved length as a px custom-property value. */
function px(value: number): string {
  return `${round(value)}px`;
}

/**
 * The geometry a tier + silhouette publishes — identical for every card that
 * shares them, so it is resolved once and reused. Only the bands and type sizes
 * the silhouette actually draws are published: a battlefield permanent has no
 * type bar and no rules area (§3.3), so it emits neither, which keeps the
 * per-face style attribute (and the reconciler's morph over it) proportional to
 * what the frame really draws.
 */
const GEOMETRY = new Map<string, Record<string, string>>();

function faceGeometryVars(tier: CardFaceTier, kind: CardSurfaceKind): Record<string, string> {
  const key = `${tier}|${kind}`;
  const cached = GEOMETRY.get(key);
  if (cached !== undefined) return cached;
  const m = faceMetrics(tier, kind);
  const vars: Record<string, string> = {
    '--face-w': px(m.w),
    '--face-h': px(m.h),

    // ── Frame geometry (card-representation §3.1) ──────────────────────────
    '--face-radius': px(m.radius),
    '--plate-radius': px(m.plateRadius),
    '--frame-edge-w': px(m.edge),
    '--frame-edge-bottom': px(m.edgeBottom),
    '--rule-w': px(m.rule),
    '--rule-inset': px(m.ruleInset),

    // ── Band stack (§3.2 / §3.3, after the §8.4 floor rule) ───────────────
    '--band-title-top': px(m.bands.title.top),
    '--band-title-h': px(m.bands.title.h),
    '--band-art-top': px(m.bands.art.top),
    '--band-art-h': px(m.bands.art.h),

    // ── Type scale (§5, clamped by §8.4) ──────────────────────────────────
    '--face-name-size': px(m.name),
    '--face-pt-size': px(m.pt),
    '--face-badge-size': px(m.badge),
    '--face-tab-size': px(m.tab),
    '--face-mono-size': px(m.monogram),
    '--face-line': `${LINE}`,

    // ── Ring weights (§6.1 panels 3–4) ────────────────────────────────────
    '--ring-w': px(RUNE_FRAME.selectRing * m.w),
    '--ring-glow-w': px(RUNE_FRAME.selectGlow * m.w),
    '--target-ring-w': px(RUNE_FRAME.targetRing * m.w),
  };
  if (m.bands.status.h > 0) {
    vars['--band-status-top'] = px(m.bands.status.top);
    vars['--band-status-h'] = px(m.bands.status.h);
  }
  if (m.bands.type.h > 0) {
    vars['--band-type-top'] = px(m.bands.type.top);
    vars['--band-type-h'] = px(m.bands.type.h);
    vars['--face-type-size'] = px(m.typeLine);
  }
  if (m.bands.rules.h > 0) {
    vars['--band-rules-top'] = px(m.bands.rules.top);
    vars['--band-rules-h'] = px(m.bands.rules.h);
    vars['--face-rules-size'] = px(m.rules);
    vars['--face-cost-size'] = px(m.cost);
  }
  GEOMETRY.set(key, vars);
  return vars;
}

/**
 * The frame's material — slate, parchment, gold, and the shared motion class.
 * The same for every card at every identity, because color identity is an edge
 * accent and never a body fill (§3.4), so it is one frozen object.
 */
const MATERIAL: Record<string, string> = {
  '--face-body': SURFACES.frameEdge,
  '--face-edge-shade': SURFACES.frameEdgeShade,
  '--plate': SURFACES.plate,
  '--plate-ink': SURFACES.plateInk,
  '--plate-rim': RUNE_GOLD.plateRim,
  '--status-band': SURFACES.statusBand,
  '--cost-disc': SURFACES.costDisc,
  '--tab-fill': SURFACES.tokenTab,
  '--rune-gold': RUNE_GOLD.rule,
  '--rune-gold-shade': RUNE_GOLD.ruleShade,
  '--face-name-text': SURFACES.nameText,
  '--keyword-color': SURFACES.plateInk,
  '--face-mono-alpha': `${FRAME.monogramAlpha}`,
  '--art-scrim': `${ART.scrimAlpha * 100}%`,
  '--edge-h': `${AFFORDANCE.edgeHeight}px`,
  '--motion-micro': `${PROVISIONAL.microMs}ms`,
};

/**
 * The CSS custom properties one face renders through. Every color and size the
 * stylesheet uses flows through here from the shared tokens (`src/tokens.ts`) —
 * the ADR 0019 discipline: no hex literal ever lands in the component or its
 * stylesheet.
 *
 * State accents are published **only when their state is lit**. Every one of
 * them is used exclusively inside a `.state .selector` rule, so an unlit
 * channel's variable would never be read; leaving it out keeps the face's style
 * attribute — the thing the plane reconciler morphs on every view — as small as
 * the card's actual state.
 */
export function cardFaceVars(
  data: CardDisplayData,
  tier: CardFaceTier,
  elevation: 'rest' | 'lifted' | 'held',
): CSSProperties {
  const kind = surfaceKindFor(tier, data.landTile);
  const m = faceMetrics(tier, kind);
  const footprint = faceFootprint(tier, data.tapped ?? false, kind);
  const layers = splayLayers(data.stackCount);
  const badges = (data.counters?.length ?? 0) > 0;
  return {
    ...faceGeometryVars(tier, kind),
    ...MATERIAL,
    '--foot-w': `${footprint.w}px`,
    '--foot-h': `${footprint.h}px`,
    '--face-accent': PALETTE[data.colorIdentity],
    '--tap-rot': data.tapped ? `${(TAP.angle * 180) / Math.PI}deg` : '0deg',
    '--face-alpha': faceAlpha(data),
    '--lift': `${PROVISIONAL.lift[elevation]}px`,
    '--elev-shadow': PROVISIONAL.shadow[elevation],
    // Only a fold publishes the pile layers; an unfolded face leaves the
    // stylesheet's transparent default in place.
    ...(layers > 0 ? { '--splay-layers': splayShadow(layers, m.w, m.h) } : {}),

    // ── State channels (§6), each published only when it is lit ───────────
    ...(data.actionable ? { '--gold': AFFORDANCE.actionable } : {}),
    ...(data.selected
      ? { '--selection': INDICATORS.selectRing, '--selection-glow': INDICATORS.selectGlow }
      : {}),
    ...(data.targeting ? { '--targeting': INDICATORS.targetPath } : {}),
    ...(data.attacking ? { '--attacking': INDICATORS.attackingBar } : {}),
    ...(data.blocking || (data.blockedBy ?? 0) > 0 ? { '--blocking': INDICATORS.blockingBar } : {}),
    ...(data.hasActivatedAbility ? { '--ability-marker': INDICATORS.abilityMarker } : {}),
    ...(badges
      ? { '--counter-bg': INDICATORS.counterBg, '--counter-text': INDICATORS.counterText }
      : {}),
    ...((data.markedDamage ?? 0) > 0
      ? { '--damage-bg': INDICATORS.damageBg, '--damage-text': INDICATORS.damageText }
      : {}),
    ...(badges || (data.markedDamage ?? 0) > 0 || (data.blockedBy ?? 0) > 0
      ? {
          '--badge-bg': BADGE.bg,
          '--badge-text': BADGE.text,
          '--badge-stroke': BADGE.stroke,
        }
      : {}),
  } as CSSProperties;
}

/**
 * The custom properties the {@link CardArt} primitive owns (issue #527),
 * published on the image element itself so a card illustration is contained
 * even on a surface that publishes no frame variables at all (the inspect
 * panel). Frame-relative geometry — `--rule-inset`, `--band-art-top`,
 * `--band-art-h`, `--plate-radius`, `--face-radius` — still flows down from
 * {@link cardFaceVars}; these are the values that belong to the art window
 * itself: where a cover-fitted crop is anchored (an illustration on its focal
 * anchor, a whole printed card on the top-aligned §12 anchor), the declared
 * ratios the screen-space modes size by, and what an empty or failed image
 * reads as.
 * Every one comes from `src/tokens.ts`.
 */
export function cardArtVars(): CSSProperties {
  return {
    '--art-focus': `${ART.focusX * 100}% ${ART.focusY * 100}%`,
    '--art-full-focus': `${ART.fullFocusX * 100}% ${ART.fullFocusY * 100}%`,
    '--art-panel-aspect': `${ART.panelAspect}`,
    '--art-card-aspect': `${ART.cardAspect}`,
    '--art-empty': SURFACES.cardBody,
  } as CSSProperties;
}

/**
 * The custom properties the **reserved art slot** owns (issue #527) — the
 * screen-space rectangle the inspect surfaces keep for an illustration whether
 * or not one exists and whichever art mode is active. Published on the slot
 * itself for the same reason {@link cardArtVars} is published on the image: the
 * inspect popover is a chrome surface that emits no frame variables at all, so
 * the slot must carry its own geometry rather than inherit it.
 *
 * `--art-mono-color` is only the *fallback* for the empty-state monogram: on a
 * surface that does publish a frame (the inspect tier) the slot inherits
 * `--face-accent` and the mark keeps the card's color identity.
 */
export function cardArtSlotVars(): CSSProperties {
  return {
    '--art-slot-aspect': `${ART.slotAspect}`,
    '--art-radius': `${ART.radius}px`,
    '--art-empty': SURFACES.cardBody,
    '--art-mono-size': `${ART.slotMonogram}px`,
    '--art-mono-alpha': `${FRAME.monogramAlpha}`,
    '--art-mono-color': SURFACES.typeText,
  } as CSSProperties;
}

/**
 * The face's resting opacity: tap dims slightly — except for a declared
 * attacker, which keeps full presence while tapped (it is in combat, not
 * inert) — and an ineligible card during targeting recedes multiplicatively.
 *
 * Summoning sickness no longer dims: card-representation §6.2 replaces the
 * alpha with a dedicated glyph plate in the status band, so the state survives
 * at every tier and never competes with tap for the same channel.
 */
export function faceAlpha(data: CardDisplayData): number {
  const base = data.tapped && !data.attacking ? FRAME.tappedAlpha : 1;
  return data.dimmed ? base * FRAME.dimmedAlpha : base;
}
