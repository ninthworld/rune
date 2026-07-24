import type { CSSProperties } from 'react';
import {
  AFFORDANCE,
  ART,
  BADGE,
  FRAME,
  INDICATORS,
  PALETTE,
  PT_TEXT,
  SURFACES,
  TAP,
  TIER,
} from '../../tokens';
import type { CardDisplayData } from '../cardFactory';

/**
 * The size tiers the DOM card face renders. The five carried tiers come from the
 * shared `TIER` tokens; `inspect` is the fixed screen-space reading tier
 * (ui-design-notes §Card render — everything the server supplies, independent
 * of battlefield card size).
 */
export type CardFaceTier = 'chip' | 'mini' | 'support' | 'field' | 'hand' | 'inspect';

/** The tiers that render on the battlefield plane — the ≤ 12-node budget binds
 * on exactly these (presentation-budgets §Performance). */
export const BATTLEFIELD_TIERS: readonly CardFaceTier[] = ['chip', 'mini', 'support', 'field'];

/**
 * PROVISIONAL presentation seeds (issue #479) — the values issue #480 (the
 * visual-system token pass) owns. Collected here as the single swap point so
 * the component itself never carries a literal: when #480 lands its tokens,
 * this object is replaced by imports and nothing else moves. Values seed from
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
  /** The fixed screen-space inspect tier (independent of battlefield size). */
  inspect: { w: 220, name: 14, mono: 48, pip: 16, header: 40, type: 11, rules: 12 },
} as const;

/** The face metrics for a tier: card box and per-slot font/pip sizes. */
export interface FaceMetrics {
  w: number;
  h?: number;
  name: number;
  mono: number;
  pip: number;
  header: number;
  type: number;
}

/** Resolve a tier's face metrics (chip has no text slots beyond its name). */
export function faceMetrics(tier: CardFaceTier): FaceMetrics {
  if (tier === 'chip')
    return { w: TIER.chip.w, h: TIER.chip.h, name: 9, mono: 0, pip: 0, header: 0, type: 0 };
  if (tier === 'inspect') return PROVISIONAL.inspect;
  return TIER[tier];
}

/**
 * The reserved footprint of a face: the rotated bounding box when tapped (the
 * carried rule — rotation must reserve its swept box so drawn pixels always
 * match the reported rect; same math as the Pixi factory's tap transform and
 * the scene's `tappedFootprint`).
 */
export function faceFootprint(tier: CardFaceTier, tapped: boolean): { w: number; h: number } {
  const m = faceMetrics(tier);
  const h = m.h ?? Math.round((m.w * TIER.field.h) / TIER.field.w);
  if (!tapped) return { w: m.w, h };
  const c = Math.cos(TAP.angle);
  const s = Math.sin(TAP.angle);
  return { w: Math.round(m.w * c + h * s), h: Math.round(m.w * s + h * c) };
}

/**
 * The CSS custom properties one face renders through. Every color and size the
 * stylesheet uses flows through here from the shared tokens (`src/tokens.ts`) —
 * the ADR 0019 discipline: no hex literal ever lands in the component or its
 * stylesheet.
 */
export function cardFaceVars(
  data: CardDisplayData,
  tier: CardFaceTier,
  elevation: 'rest' | 'lifted' | 'held',
): CSSProperties {
  const m = faceMetrics(tier);
  const footprint = faceFootprint(tier, data.tapped ?? false);
  const accent = PALETTE[data.colorIdentity];
  return {
    '--face-w': `${m.w}px`,
    '--face-h': m.h !== undefined ? `${m.h}px` : 'auto',
    '--foot-w': `${footprint.w}px`,
    '--foot-h': m.h !== undefined ? `${footprint.h}px` : 'auto',
    '--face-radius': `${tier === 'chip' ? FRAME.chipRadius : FRAME.radius}px`,
    '--face-accent': accent,
    '--face-body': SURFACES.cardBody,
    '--face-name-text': SURFACES.nameText,
    '--face-type-text': SURFACES.typeText,
    '--face-pt-text': PT_TEXT[data.colorIdentity],
    '--face-border-w': `${FRAME.borderWidth}px`,
    '--face-header-h': `${m.header}px`,
    '--face-header-alpha': `${FRAME.headerTintAlpha * 100}%`,
    '--face-name-size': `${m.name}px`,
    '--face-mono-size': `${m.mono}px`,
    '--face-mono-alpha': FRAME.monogramAlpha,
    '--face-pip-size': `${m.pip}px`,
    '--face-type-size': `${m.type}px`,
    '--edge-h': `${AFFORDANCE.edgeHeight}px`,
    '--gold': AFFORDANCE.actionable,
    '--selection': SURFACES.selection,
    '--targeting': SURFACES.targeting,
    '--attacking': INDICATORS.attackingBar,
    '--blocking': INDICATORS.blockingBar,
    '--ability-marker': INDICATORS.abilityMarker,
    '--keyword-color': INDICATORS.keyword,
    '--badge-bg': BADGE.bg,
    '--badge-text': BADGE.text,
    '--badge-stroke': BADGE.stroke,
    '--counter-bg': BADGE.counterBg,
    '--counter-text': BADGE.counterText,
    '--damage-bg': INDICATORS.damageBg,
    '--damage-text': INDICATORS.damageText,
    '--ring-w': `${FRAME.selectionWidth}px`,
    '--art-inset': `${ART.inset}px`,
    '--art-top-gap': `${ART.topGap}px`,
    '--art-bottom-reserve': `${ART.bottomReserve}px`,
    '--art-radius': `${ART.radius}px`,
    '--art-scrim': `${ART.scrimAlpha * 100}%`,
    '--face-rules-size': `${tier === 'inspect' ? PROVISIONAL.inspect.rules : m.type}px`,
    '--tap-rot': data.tapped ? `${(TAP.angle * 180) / Math.PI}deg` : '0deg',
    '--face-alpha': faceAlpha(data),
    '--lift': `${PROVISIONAL.lift[elevation]}px`,
    '--elev-shadow': PROVISIONAL.shadow[elevation],
    '--motion-micro': `${PROVISIONAL.microMs}ms`,
  } as CSSProperties;
}

/**
 * The face's resting opacity, carried from the factory: tap dims slightly —
 * except for a declared attacker, which keeps full presence while tapped (it is
 * in combat, not inert) — summoning sickness dims a touch, and an ineligible
 * card during targeting recedes multiplicatively.
 */
export function faceAlpha(data: CardDisplayData): number {
  const base =
    data.tapped && !data.attacking ? FRAME.tappedAlpha : data.summoningSick ? FRAME.sickAlpha : 1;
  return data.dimmed ? base * FRAME.dimmedAlpha : base;
}
