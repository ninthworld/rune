/**
 * The Rune frame's measured model (issue #529), asserted against the binding
 * specification `docs/design/card-representation.md` — itself a transcription of
 * the approved baselines in `docs/ui-concepts/`.
 *
 * These are contract tests over pure functions and tokens, not layout tests:
 * jsdom performs no layout and applies no CSS module, so what is asserted here
 * is the **declared** geometry every stylesheet then consumes — the boxes, the
 * band stack, the floor rule, and the token values. Whether the painted frame
 * looks like the baseline is browser verification and belongs to the maintainer.
 */
import { describe, expect, it } from 'vitest';
import {
  ART,
  CARD_BACK,
  FRAME,
  INDICATORS,
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
import {
  BATTLEFIELD_TIERS,
  CARD_BOX,
  SCREEN_TIERS,
  authoredTypeSize,
  faceFootprint,
  faceMetrics,
  splayLayers,
  splayShadow,
  surfaceKindFor,
  type CardFaceTier,
  type CardSurfaceKind,
} from './theme';

const ALL_TIERS: CardFaceTier[] = ['chip', 'mini', 'support', 'field', 'hand', 'stack', 'inspect'];

describe('the frame family: two silhouettes, one anatomy (card-representation §2, §3.1)', () => {
  it('draws every battlefield permanent as a 1.00 square plaque', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      const m = faceMetrics(tier, 'permanent');
      expect(m.w / m.h, tier).toBe(RUNE_FRAME.aspectPermanent);
    }
  });

  it('draws every screen-space card at the 0.715 portrait aspect', () => {
    for (const tier of SCREEN_TIERS) {
      const m = faceMetrics(tier, 'card');
      // Within 1% of the authored ratio — the tier table is whole px.
      expect(
        Math.abs(m.w / m.h - RUNE_FRAME.aspectFull) / RUNE_FRAME.aspectFull,
        tier,
      ).toBeLessThan(0.01);
    }
  });

  it('draws every battlefield land as the 1.45 resource tile', () => {
    for (const tier of BATTLEFIELD_TIERS.filter((t) => t !== 'chip')) {
      const m = faceMetrics(tier, 'land');
      expect(
        Math.abs(m.w / m.h - RUNE_FRAME.aspectLandTile) / RUNE_FRAME.aspectLandTile,
        tier,
      ).toBeLessThan(0.02);
      // A land tile is WIDER than tall, so it can never be confused with the
      // square plaque of a nonland permanent at the same tier.
      expect(m.h, tier).toBeLessThan(faceMetrics(tier, 'permanent').h);
    }
  });

  it('resolves the silhouette from the staging row, never from the card itself', () => {
    expect(surfaceKindFor('field')).toBe('permanent');
    expect(surfaceKindFor('field', true)).toBe('land');
    // In hand, on the stack and in inspect a land is an ordinary portrait card
    // (§4, §11: the tile grows INTO the full card on focus).
    for (const tier of SCREEN_TIERS) {
      expect(surfaceKindFor(tier, true), tier).toBe('card');
      expect(surfaceKindFor(tier, false), tier).toBe('card');
    }
  });
});

describe('the canonical tier table (card-representation §8.1)', () => {
  /** The 4-player, 1280 × 720 dimensions the specification publishes. */
  const CANONICAL: [CardFaceTier, CardSurfaceKind, number, number][] = [
    ['field', 'permanent', 96, 96],
    ['field', 'land', 96, 66],
    ['support', 'permanent', 78, 78],
    ['support', 'land', 78, 54],
    ['mini', 'permanent', 62, 62],
    ['chip', 'permanent', 48, 48],
    ['hand', 'card', 116, 162],
    ['stack', 'card', 104, 145],
    ['inspect', 'card', 260, 364],
  ];

  it.each(CANONICAL)('%s / %s is %i × %i', (tier, kind, w, h) => {
    const m = faceMetrics(tier, kind);
    expect([m.w, m.h]).toEqual([w, h]);
  });

  it('replaces the inherited 84 × 118 field and 104 × 146 hand assumptions', () => {
    expect([TIER.field.w, TIER.field.h]).not.toEqual([84, 118]);
    expect([TIER.hand.w, TIER.hand.h]).not.toEqual([104, 146]);
    // The battlefield permanent grew in width, which is what the square
    // silhouette buys (§15.8): art dominance at a smaller drawn height.
    expect(TIER.field.w).toBeGreaterThan(84);
  });

  it('publishes one box table for the plane to stage from, matching the faces', () => {
    for (const tier of ALL_TIERS) {
      for (const kind of ['permanent', 'land', 'card'] as CardSurfaceKind[]) {
        const m = faceMetrics(tier, kind);
        expect(CARD_BOX[tier][kind], `${tier}/${kind}`).toEqual({ w: m.w, h: m.h });
      }
    }
  });

  it('reserves the swept box for a tapped card at every tier and silhouette', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      for (const kind of ['permanent', 'land'] as CardSurfaceKind[]) {
        const rest = faceFootprint(tier, false, kind);
        const tapped = faceFootprint(tier, true, kind);
        expect(rest, `${tier}/${kind}`).toEqual(CARD_BOX[tier][kind]);
        expect(tapped.w, `${tier}/${kind}`).toBeGreaterThan(rest.w);
        expect(tapped.h, `${tier}/${kind}`).toBeGreaterThan(rest.h);
        const c = Math.cos(TAP.angle);
        const s = Math.sin(TAP.angle);
        expect(tapped.w).toBe(Math.round(rest.w * c + rest.h * s));
        expect(tapped.h).toBe(Math.round(rest.w * s + rest.h * c));
      }
    }
  });

  it('keeps every battlefield tier above the 44 px interactive floor in width', () => {
    // presentation-budgets §Accessibility: the drawn card need not be 44 px in
    // both axes (the plane grows the hotspot), but the width never falls short.
    for (const tier of BATTLEFIELD_TIERS) {
      expect(faceMetrics(tier, 'permanent').w, tier).toBeGreaterThanOrEqual(44);
    }
  });
});

describe('the band stack (card-representation §3.2, §3.3)', () => {
  it('gives the battlefield permanent title + art + status, and NOTHING else', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      const { bands } = faceMetrics(tier, 'permanent');
      // The permanent face carries no mana cost and no type bar — a normative
      // rule of the family (§3.3), not a truncation.
      expect(bands.type.h, tier).toBe(0);
      expect(bands.rules.h, tier).toBe(0);
      expect(bands.art.h, tier).toBeGreaterThan(0);
      expect(bands.status.h, tier).toBeGreaterThan(0);
    }
  });

  it('gives the full card title + art + type + rules, and no status band', () => {
    for (const tier of SCREEN_TIERS) {
      const { bands } = faceMetrics(tier, 'card');
      expect(bands.status.h, tier).toBe(0);
      for (const band of [bands.title, bands.art, bands.type, bands.rules]) {
        expect(band.h, tier).toBeGreaterThan(0);
      }
    }
  });

  it('gives the land resource tile an art window and no title bar band', () => {
    const { bands } = faceMetrics('field', 'land');
    expect([bands.type.h, bands.rules.h, bands.status.h]).toEqual([0, 0, 0]);
    // The art window runs to the bottom edge; §15.9's name strip overlays it.
    expect(bands.art.top + bands.art.h).toBeCloseTo(
      faceMetrics('field', 'land').h - faceMetrics('field', 'land').edgeBottom,
      6,
    );
  });

  it('stacks the bands in order with the gold rules and edges accounted for', () => {
    for (const tier of ALL_TIERS) {
      const m = faceMetrics(tier, tier === 'chip' ? 'permanent' : surfaceKindFor(tier));
      const { bands } = m;
      const drawn = [bands.title, bands.art, bands.type, bands.rules, bands.status].filter(
        (b) => b.h > 0,
      );
      // Bands never overlap and the last one closes onto the bottom edge.
      const ordered = [...drawn].sort((a, b) => a.top - b.top);
      for (let i = 1; i < ordered.length; i += 1) {
        expect(ordered[i]!.top, `${tier} band ${i}`).toBeGreaterThanOrEqual(
          ordered[i - 1]!.top + ordered[i - 1]!.h,
        );
      }
      expect(ordered[0]!.top, tier).toBeCloseTo(m.edge + m.rule, 6);
      const last = ordered.at(-1)!;
      expect(last.top + last.h, tier).toBeCloseTo(m.h - m.edgeBottom, 6);
    }
  });

  it('keeps the art window the single largest band at every battlefield tier', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      const m = faceMetrics(tier, 'permanent');
      expect(m.bands.art.h, tier).toBeGreaterThan(m.bands.title.h);
      expect(m.bands.art.h, tier).toBeGreaterThan(m.bands.status.h);
    }
  });

  it('makes art DOMINANT at the baseline tier, where the issue requires it', () => {
    // Issue #529: "artwork is the dominant content at battlefield and hand
    // sizes". At `field` and on the full card the art window outweighs every
    // other band put together. It cannot at `mini` (62 px): an 11 px name plus
    // a 12 px P/T with the §8.4 line boxes already consume 31 px of 62, which
    // is the floor rule's stated consequence — the plates never shrink.
    const field = faceMetrics('field', 'permanent');
    expect(field.bands.art.h).toBeGreaterThan(field.bands.title.h + field.bands.status.h);
    const hand = faceMetrics('hand', 'card');
    expect(hand.bands.art.h).toBeGreaterThan(hand.bands.title.h + hand.bands.type.h);
  });

  it('makes the bottom edge thicker than the other three — the paper read', () => {
    const m = faceMetrics('field', 'permanent');
    expect(m.edgeBottom).toBeGreaterThan(m.edge);
    expect(m.edgeBottom / m.w).toBeCloseTo(RUNE_FRAME.edgeBottom, 6);
    expect(m.edge / m.w).toBeCloseTo(RUNE_FRAME.edge, 6);
  });
});

describe('the floor rule (card-representation §8.4)', () => {
  it('clamps every drawn name to 11 px and every critical value to 12 px', () => {
    for (const tier of ALL_TIERS) {
      const m = faceMetrics(tier, surfaceKindFor(tier));
      expect(m.pt, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorValue);
      expect(m.tab, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorValue);
      expect(m.badge, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorValue);
      if (m.name > 0) expect(m.name, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorName);
      if (m.typeLine > 0) expect(m.typeLine, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorName);
      if (m.rules > 0) expect(m.rules, tier).toBeGreaterThanOrEqual(RUNE_TYPE.floorName);
    }
  });

  it('is a real clamp: the authored ratio at battlefield widths is below the floor', () => {
    // The specification's worked example: at `field` (W = 96) the authored name
    // size is 7.1 px, so the floor bites and the title bar grows around it.
    expect(authoredTypeSize(RUNE_TYPE.name, TIER.field.w)).toBeCloseTo(7.104, 3);
    expect(faceMetrics('field', 'permanent').name).toBe(RUNE_TYPE.floorName);
  });

  it('grows the holding band around a clamped size instead of clipping it', () => {
    const m = faceMetrics('field', 'permanent');
    // The authored title band would be 0.100 · H = 9.6 px, too short for an
    // 11 px name; it grows to the line box instead (§8.4's 9.6 → 15 px).
    expect(RUNE_BANDS_PERM.title * m.h).toBeLessThan(m.name * 1.35);
    expect(m.bands.title.h).toBeCloseTo(m.name * 1.35, 6);
    expect(m.bands.status.h).toBeCloseTo(m.pt * 1.35, 6);
  });

  it('makes the art window absorb the difference, never the critical values', () => {
    const m = faceMetrics('field', 'permanent');
    const authoredArt = RUNE_BANDS_PERM.art * m.h;
    expect(m.bands.art.h).toBeLessThan(authoredArt);
    // …and the P/T plate keeps its full clamped size regardless.
    expect(m.pt).toBeGreaterThanOrEqual(RUNE_TYPE.floorValue);
  });

  it('drops the chip title bar entirely rather than draw an illegible name', () => {
    // At W = 48 the name cannot reach the 11 px floor inside the band at all,
    // so identity moves to the glyph + inspect path (§8.4).
    expect(faceMetrics('chip', 'permanent').name).toBe(0);
    expect(faceMetrics('chip', 'permanent').bands.title.h).toBe(0);
    // The P/T plate survives the chip rung — it is the authoritative surface.
    expect(faceMetrics('chip', 'permanent').bands.status.h).toBeGreaterThan(0);
  });
});

describe('the ×N pile splay (card-representation §6.1 panel 7, §15.3)', () => {
  it('caps the drawn depth so a 240-fold looks like a 4-fold', () => {
    expect(splayLayers(1)).toBe(0);
    expect(splayLayers(undefined)).toBe(0);
    expect(splayLayers(2)).toBe(1);
    expect(splayLayers(4)).toBe(SPLAY.maxLayers);
    expect(splayLayers(240)).toBe(SPLAY.maxLayers);
  });

  it('splays DOWN-AND-LEFT, keeping the right edge clear for the P/T plate', () => {
    const m = faceMetrics('field', 'permanent');
    const shadow = splayShadow(SPLAY.maxLayers, m.w, m.h);
    const offsets = [...shadow.matchAll(/(-?[\d.]+)px (-?[\d.]+)px/g)].map(([, x, y]) => ({
      x: Number(x),
      y: Number(y),
    }));
    expect(offsets).toHaveLength(SPLAY.maxLayers * 2);
    for (const o of offsets) {
      expect(o.x).toBeLessThan(0);
      expect(o.y).toBeGreaterThan(0);
    }
    // Monotonically deeper, at the authored fraction of the card box.
    expect(offsets[0]!.x).toBeCloseTo(-SPLAY.stepX * m.w, 1);
    expect(offsets[0]!.y).toBeCloseTo(SPLAY.stepY * m.h, 1);
    expect(Math.abs(offsets.at(-1)!.x)).toBeGreaterThan(Math.abs(offsets[0]!.x));
  });

  it('scales the pile with the card box, so every tier splays proportionally', () => {
    const field = faceMetrics('field', 'permanent');
    const mini = faceMetrics('mini', 'permanent');
    const first = (css: string) => Number(/(-?[\d.]+)px/.exec(css)![1]);
    expect(Math.abs(first(splayShadow(1, field.w, field.h)))).toBeGreaterThan(
      Math.abs(first(splayShadow(1, mini.w, mini.h))),
    );
  });

  it('paints only through the face tokens — no literal ever reaches a layer', () => {
    const css = splayShadow(3, 96, 96);
    expect(css).not.toMatch(/#[0-9a-f]{3,8}|rgba?\(/i);
    expect(css).toContain('var(--face-body)');
    expect(css).toContain('var(--face-accent)');
  });
});

describe('the token additions (card-representation §5)', () => {
  it('adds exactly the named frame, band, and type blocks', () => {
    expect(RUNE_FRAME.aspectFull).toBe(0.715);
    expect(RUNE_FRAME.aspectPermanent).toBe(1.0);
    expect(RUNE_FRAME.aspectLandTile).toBe(1.45);
    expect(RUNE_FRAME.radius).toBe(0.07);
    expect(RUNE_FRAME.plateRadius).toBe(0.035);
    expect(RUNE_FRAME.edge).toBe(0.037);
    expect(RUNE_FRAME.edgeBottom).toBe(0.057);
    expect(RUNE_FRAME.rule).toBe(0.01);
    expect(RUNE_FRAME.ruleInset).toBe(0.063);
    expect(RUNE_FRAME.archRise).toBe(0.09);
    expect(RUNE_BANDS_FULL).toMatchObject({ title: 0.077, art: 0.482, type: 0.077, rules: 0.27 });
    expect(RUNE_BANDS_PERM).toMatchObject({ title: 0.1, art: 0.647, status: 0.137 });
    expect(RUNE_TYPE).toMatchObject({
      name: 0.074,
      typeLine: 0.056,
      rules: 0.056,
      pt: 0.115,
      cost: 0.085,
      tab: 0.075,
      badge: 0.07,
      floorName: 11,
      floorValue: 12,
    });
    expect(SPLAY).toMatchObject({ stepX: 0.055, stepY: 0.03, maxLayers: 3 });
  });

  it('adds the slate/parchment/gold material and the card back field', () => {
    expect(SURFACES.frameEdge).toBe('#2E343A');
    expect(SURFACES.frameEdgeShade).toBe('#1B2024');
    expect(SURFACES.plate).toBe('#DED8CB');
    expect(SURFACES.plateInk).toBe('#191C20');
    expect(SURFACES.statusBand).toBe('#2F3438');
    expect(SURFACES.costDisc).toBe('#20262B');
    expect(SURFACES.tokenTab).toBe('#20262B');
    expect(RUNE_GOLD).toMatchObject({
      rule: '#C7A46A',
      ruleShade: '#8A7042',
      plateRim: '#B9955E',
    });
    expect(CARD_BACK).toMatchObject({
      field: '#2B3340',
      emblem: '#C7A46A',
      rivet: '#C7A46A',
    });
  });

  it('keeps selection BLUE and targeting ORANGE (§15.1 maintainer ruling)', () => {
    expect(INDICATORS.selectRing).toBe(SURFACES.selection);
    expect(INDICATORS.selectGlow).toBe(SURFACES.selection);
    expect(INDICATORS.targetPath).toBe(SURFACES.targeting);
    expect(INDICATORS.targetReticle).toBe(SURFACES.targeting);
    expect(SURFACES.selection).toBe('#7FB2E5');
    expect(SURFACES.targeting).toBe('#E0784A');
    // The approved sheets draw a violet selection ring and a blue targeting
    // path; the ruling supersedes both, and the violet must not come back.
    expect(INDICATORS.selectRing).not.toBe(INDICATORS.abilityMarker);
    for (const value of Object.values(INDICATORS)) expect(value).not.toBe('#8B5CF6');
    // Selection and targeting stay separated by SHAPE as well as hue: the ring
    // weights differ, so a colorblind player still tells them apart.
    expect(RUNE_FRAME.selectRing).toBeGreaterThan(RUNE_FRAME.targetRing);
  });

  it('takes the counter and damage badge colors off the measured sheet', () => {
    expect(INDICATORS.counterBg).toBe('#2A5436');
    expect(INDICATORS.counterText).toBe('#EAF3E9');
    expect(INDICATORS.damageBg).toBe('#8E3A2A');
    expect(INDICATORS.damageText).toBe('#F6E7E4');
  });

  it('drops the summoning-sickness alpha — §6.2 makes it a glyph plate', () => {
    expect(FRAME).not.toHaveProperty('sickAlpha');
    expect(FRAME.tappedAlpha).toBe(0.8);
    expect(FRAME.dimmedAlpha).toBe(0.32);
  });

  it('keeps the art window on every framed tier but never on the chip', () => {
    // The window is ONE node either way (illustration or procedural field), so
    // widening it costs no budget; a digest chip stays procedural (§12).
    expect(ART.tiers).not.toContain('chip');
    for (const tier of ['mini', 'support', 'field', 'hand']) {
      expect(ART.tiers, tier).toContain(tier);
    }
  });
});
