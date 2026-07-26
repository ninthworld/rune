/**
 * The **states sheet** — keyed panel by panel to `docs/ui-concepts/rune-card-states.jpg`
 * as transcribed in `docs/design/card-representation.md` §6.1, plus the states
 * the baselines do not show (§6.2), the art modes (§12), and the ADR 0030
 * transform/opacity-only motion contract. Issue #529.
 *
 * jsdom applies no CSS module and computes no stacking or layout, so the
 * channels are asserted where they are actually declared: the face's stable
 * `data-*` attributes and custom properties (the same hooks the consuming
 * surfaces use), and the stylesheet source for the zero-node channels
 * themselves. Whether the ring is visibly blue is browser verification.
 */
import { cleanup, render } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { AFFORDANCE, FRAME, INDICATORS, RUNE_FRAME, SURFACES, TAP } from '../../tokens';
import type { CardDisplayData } from '../cardFactory';
import { CardFace, type CardFaceProps } from './CardFace';
import { PROVISIONAL, faceAlpha, faceMetrics } from './theme';
import { glyphStripGeometry } from './glyphStrip';
import s from './card-face.module.css';

afterEach(cleanup);

const HERE = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(join(HERE, 'card-face.module.css'), 'utf8');

/** One CSS rule body from the frame stylesheet, or '' when it does not exist. */
function rule(selector: string): string {
  return new RegExp(`${selector}\\s*\\{[^}]*\\}`, 's').exec(css)?.[0] ?? '';
}

function bear(overrides: Partial<CardDisplayData> = {}): CardDisplayData {
  return {
    name: 'Runeclaw Bear',
    typeLine: 'Creature — Bear',
    colorIdentity: 'G',
    manaCost: '{1}{G}',
    power: '2',
    toughness: '2',
    ...overrides,
  };
}

function renderFace(data: CardDisplayData, extra: Partial<CardFaceProps> = {}) {
  const { container } = render(<CardFace data={data} {...extra} />);
  return container.firstElementChild as HTMLElement;
}

/** The face's rotating/lifting layer (the only transitioned element). */
function inner(root: HTMLElement): HTMLElement {
  return root.firstElementChild as HTMLElement;
}

describe('states sheet panels 1–9 (card-representation §6.1)', () => {
  it('1. normal — a resting face lights no state channel at all', () => {
    const root = renderFace(bear());
    for (const key of ['tapped', 'selected', 'targeting', 'actionable', 'attacking', 'blocking']) {
      expect(root.dataset[key], key).toBeUndefined();
    }
    expect(root.style.getPropertyValue('--face-alpha')).toBe('1');
    expect(root.dataset.elevation).toBe('rest');
  });

  it('2. tapped — one rotation + dim treatment, at every tier and every seat', () => {
    const degrees = Math.round((TAP.angle * 180) / Math.PI);
    expect(degrees).toBe(25);
    const tapped = renderFace(bear({ tapped: true }));
    expect(tapped.style.getPropertyValue('--tap-rot')).toBe(`${degrees}deg`);
    expect(tapped.style.getPropertyValue('--face-alpha')).toBe(`${FRAME.tappedAlpha}`);
    cleanup();
    expect(renderFace(bear()).style.getPropertyValue('--tap-rot')).toBe('0deg');
    // A declared attacker keeps full opacity while tapped — it is in combat.
    expect(faceAlpha(bear({ tapped: true, attacking: true }))).toBe(1);
  });

  it('3. selected — a BLUE ring plus a same-hue bloom, separated by spread', () => {
    const root = renderFace(bear({ selected: true }), { elevation: 'held' });
    expect(root.dataset.selected).toBe('true');
    expect(root.style.getPropertyValue('--selection')).toBe(INDICATORS.selectRing);
    expect(root.style.getPropertyValue('--selection-glow')).toBe(INDICATORS.selectGlow);
    // The maintainer's §15.1 ruling: selection is blue, not the sheet's violet.
    expect(INDICATORS.selectRing).toBe(SURFACES.selection);
    // Ring and bloom are one hue; the bloom reads by SPREAD.
    const body = rule('\\.selected \\.inner');
    expect(body).toContain('var(--ring-w) var(--selection)');
    expect(body).toContain('var(--ring-glow-w)');
    expect(RUNE_FRAME.selectGlow).toBeGreaterThan(RUNE_FRAME.selectRing);
    // Selection lifts and straightens; it never brightens the body (§3.11).
    expect(root.style.getPropertyValue('--lift')).toBe(`${PROVISIONAL.lift.held}px`);
    expect(root.style.getPropertyValue('--face-alpha')).toBe('1');
  });

  it('4. targeted — the ORANGE targeting family, a thinner ring than selection', () => {
    const root = renderFace(bear({ targeting: true }));
    expect(root.dataset.targeting).toBe('true');
    expect(root.style.getPropertyValue('--targeting')).toBe(INDICATORS.targetPath);
    expect(INDICATORS.targetPath).toBe(SURFACES.targeting);
    // Distinct from selection by weight as well as hue (§6.2).
    expect(RUNE_FRAME.targetRing).toBeLessThan(RUNE_FRAME.selectRing);
    expect(rule('\\.targeting \\.inner')).toContain('var(--target-ring-w) var(--targeting)');
    // The drawn path and the chosen-target reticle live on the SCENE layer,
    // above every face — this face draws neither.
    expect(css).not.toContain('reticle');
  });

  it('5. counters + damage — shaped badges that never hide the P/T plate', () => {
    const root = renderFace(bear({ counters: [{ kind: '+1/+1', count: 2 }], markedDamage: 3 }), {
      tier: 'hand',
    });
    expect(root.textContent).toContain('+1/+1 ×2');
    expect(root.textContent).toContain('3 dmg');
    // §15.2: the badges seat inside the ART window, so the status band, the
    // glyph plates and the P/T plate all stay visible.
    expect(root.textContent).toContain('2/2');
    const badge = rule('\\.badge');
    expect(badge).toContain('var(--band-art-top)');
    expect(badge).toContain('var(--band-art-h)');
    // Counters take the lower-LEFT channel, damage the lower-RIGHT (§7.1)…
    expect(badge).toContain('left: var(--rule-inset)');
    expect(rule('\\.badgeDamage')).toContain('right: var(--rule-inset)');
    // …and damage keeps a torn, deliberately non-rectangular silhouette (§7.3).
    expect(rule('\\.badgeDamage')).toContain('clip-path');
  });

  it('5b. damage is never merged into the P/T plate (CR 120.3, §7.3)', () => {
    const root = renderFace(bear({ markedDamage: 2 }));
    // The plate shows the server's current toughness; damage is a marked value.
    expect(root.textContent).toContain('2/2');
    expect(root.textContent).toContain('2 dmg');
  });

  it('7. identical stack — a capped splayed pile plus the exact count on a tab', () => {
    const root = renderFace(bear({ stackCount: 4 }));
    expect(root.dataset.stack).toBe('4');
    expect(root.dataset.splay).toBe('3');
    expect(root.textContent).toContain('×4');
    // The pile costs zero elements: it is one layered box-shadow.
    expect(rule('\\.stacked \\.inner')).toContain('--splay-shadow');
  });

  it('9. spell on stack — the full 0.715 card with all four bands', () => {
    const root = renderFace(bear(), { tier: 'stack', rulesText: 'Deal 3 damage.' });
    expect(root.dataset.kind).toBe('card');
    expect(root.textContent).toContain('Runeclaw Bear');
    expect(root.textContent).toContain('Creature — Bear');
    expect(root.textContent).toContain('Deal 3 damage.');
    // The controller accent and order index ride the SLOT, never the card
    // (§4, §6.1 panel 9) — no seat color reaches this face.
    expect(root.style.getPropertyValue('--seat-accent')).toBe('');
  });
});

describe('states the baselines do not show (card-representation §6.2)', () => {
  it('hover / focus is elevation only — no ring, no tint', () => {
    for (const elevation of ['rest', 'lifted', 'held'] as const) {
      const root = renderFace(bear(), { elevation });
      expect(root.dataset.elevation).toBe(elevation);
      expect(root.style.getPropertyValue('--lift')).toBe(`${PROVISIONAL.lift[elevation]}px`);
      expect(root.style.getPropertyValue('--elev-shadow')).toBe(PROVISIONAL.shadow[elevation]);
      // Elevation never lights the selection channel.
      expect(root.dataset.selected).toBeUndefined();
      cleanup();
    }
  });

  it('actionable stays the GOLD bottom edge bar — the cyan rim is not adopted', () => {
    const root = renderFace(bear({ actionable: true }));
    expect(root.dataset.actionable).toBe('true');
    expect(root.style.getPropertyValue('--gold')).toBe(AFFORDANCE.actionable);
    const bar = rule('\\.inner::after');
    expect(bar).toContain('bottom: 0');
    expect(bar).toContain('height: var(--edge-h)');
    expect(bar).toContain('background: var(--gold)');
    // …and it stacks above a full-card illustration (ADR 0024 §12).
    expect(bar).toContain('z-index: 1');
  });

  it('attacking is a TOP edge bar and blocking a LEFT one — shape, not hue', () => {
    const root = renderFace(bear({ attacking: true, blocking: true }));
    expect(root.dataset.attacking).toBe('true');
    expect(root.dataset.blocking).toBe('true');
    expect(rule('\\.attacking \\.title::before')).toContain('height: var(--edge-h)');
    expect(rule('\\.blocking \\.status::before')).toContain('width: var(--edge-h)');
  });

  it('unavailable / ineligible recedes multiplicatively, and only then', () => {
    expect(faceAlpha(bear({ dimmed: true }))).toBe(FRAME.dimmedAlpha);
    expect(faceAlpha(bear({ tapped: true, dimmed: true }))).toBeCloseTo(
      FRAME.tappedAlpha * FRAME.dimmedAlpha,
    );
  });

  it('summoning sickness is a status-band plate, replacing the old alpha dim', () => {
    const root = renderFace(bear({ summoningSick: true }));
    expect(root.dataset.sick).toBe('true');
    // The state survives at every tier because it is a plate, and it no longer
    // competes with tap for the opacity channel.
    expect(faceAlpha(bear({ summoningSick: true }))).toBe(1);
    expect(faceAlpha(bear({ summoningSick: true, tapped: true }))).toBe(FRAME.tappedAlpha);
    expect(
      root.querySelector('[data-plate-extra]')!.getAttribute('data-plate-extra'),
    ).not.toBeNull();
  });

  it('the latent activated ability is a marker dot on the title bar, not the bar', () => {
    const root = renderFace(bear({ hasActivatedAbility: true, actionable: true }));
    expect(root.dataset.ability).toBe('true');
    expect(root.style.getPropertyValue('--ability-marker')).toBe(INDICATORS.abilityMarker);
    // Latent (dot) is always distinguishable from live (gold bar) — §7.5.
    expect(INDICATORS.abilityMarker).not.toBe(AFFORDANCE.actionable);
    expect(rule('\\.hasAbility \\.title::after')).toContain('border-radius: 50%');
  });

  it('carries the title band’s channels onto the colour-identity strip (§8.4)', () => {
    // At `mini` the title band IS the strip, so the attacking top bar and the
    // latent-ability dot are its pseudo-elements too — neither server-computed
    // channel is lost along with the name.
    expect(css).toContain('.attacking .identityStrip::before');
    expect(rule('\\.hasAbility \\.identityStrip::after')).toContain('border-radius: 50%');
    const strip = rule('\\.identityStrip');
    // It reserves the title band's box and paints the identity accent — an edge
    // band on the slate, never a parchment plate and never a body fill (§3.4).
    expect(strip).toContain('top: var(--band-title-top)');
    expect(strip).toContain('height: var(--band-title-h)');
    // …tinted THROUGH the identity material plate (§3.12), so the accent reads
    // as a surface with a light source rather than as a flat colour block. The
    // accent is the bottom layer, so it is still what paints when no plate does.
    expect(strip).toContain('background: var(--plate-identity), var(--face-accent)');
    expect(strip).not.toContain('var(--plate)');
    expect(strip).toContain('background-blend-mode: overlay');
    const root = renderFace(bear({ attacking: true, hasActivatedAbility: true }), { tier: 'mini' });
    expect(root.dataset.attacking).toBe('true');
    expect(root.dataset.ability).toBe('true');
  });

  it('gives every state a non-color channel (budgets §Accessibility)', () => {
    // Each channel names a distinct shape/position/transform, so a player who
    // cannot separate the hues still separates the states.
    const shapes: [string, string][] = [
      ['actionable', rule('\\.inner::after')],
      ['attacking', rule('\\.attacking \\.title::before')],
      ['blocking', rule('\\.blocking \\.status::before')],
      ['selected', rule('\\.selected \\.inner')],
      ['targeting', rule('\\.targeting \\.inner')],
    ];
    for (const [name, body] of shapes) expect(body, name).not.toBe('');
    // Tap is a transform; dim and elevation are opacity/shadow.
    expect(rule('\\.inner')).toContain('rotate(var(--tap-rot))');
    expect(rule('\\.inner')).toContain('opacity: var(--face-alpha)');
  });
});

describe('art modes (ADR 0024 / card-representation §12)', () => {
  it('renders procedurally by default — the art store may always be empty', () => {
    const root = renderFace(bear());
    expect(root.querySelector('img')).toBeNull();
    // The default fill is the color-identity field carrying the monogram; it is
    // the only art this project ever ships (ADR 0031).
    const field = root.querySelector('[data-monogram]')!;
    expect(field.getAttribute('data-monogram')).toBe('R');
    expect(rule('\\.artField')).toContain('var(--face-accent)');
  });

  it('window mode: one image in the art window at every framed tier', () => {
    for (const tier of ['mini', 'support', 'field', 'hand'] as const) {
      const root = renderFace(bear(), { tier, art: { url: 'blob:test-art' } });
      const img = root.querySelector('img')!;
      expect(img.getAttribute('src'), tier).toBe('blob:test-art');
      expect(img.getAttribute('data-art-mode'), tier).toBe('window');
      // The illustration REPLACES the procedural field rather than nesting in
      // it, so the window is one node either way.
      expect(root.querySelector('[data-monogram]'), tier).toBeNull();
      // The information budget is unchanged — the tier's own, which at `mini`
      // is the colour-identity strip rather than a drawn name (§8.4).
      expect(root.getAttribute('aria-label'), tier).toBe('Runeclaw Bear');
      if (tier !== 'mini') expect(root.textContent, tier).toContain('Runeclaw Bear');
      expect(root.textContent, tier).toContain('2/2');
      cleanup();
    }
    // A digest chip stays procedural in every art mode (§12).
    const chip = renderFace(bear(), { tier: 'chip', art: { url: 'blob:test-art' } });
    expect(chip.querySelector('img')).toBeNull();
  });

  it('land tiles stay procedural in every art mode — never a whole-card image', () => {
    const root = renderFace(bear({ landTile: true, landGlyph: 'land-forest' }), {
      tier: 'field',
      art: { url: 'blob:full', full: true },
    });
    // The full-card image would be a portrait card inside a 1.45 tile.
    expect(root.querySelector('img')?.getAttribute('data-art-mode')).not.toBe('full');
  });

  it('full-card mode replaces the drawn face; every overlay still draws on top', () => {
    const root = renderFace(
      bear({
        markedDamage: 2,
        attacking: true,
        selected: true,
        actionable: true,
        stackCount: 3,
        counters: [{ kind: '+1/+1', count: 1 }],
      }),
      { tier: 'support', art: { url: 'blob:full-card', full: true } },
    );
    expect(root.querySelector('img')?.getAttribute('data-art-mode')).toBe('full');
    // Rune's printed-text elements are suppressed — the image carries them…
    expect(root.textContent).not.toContain('Runeclaw Bear');
    expect(root.textContent).not.toContain('Creature — Bear');
    // …while every authoritative overlay of §12's invariant table remains.
    expect(root.textContent).toContain('2/2'); // current, server-computed P/T
    expect(root.textContent).toContain('2 dmg');
    expect(root.textContent).toContain('+1/+1');
    expect(root.textContent).toContain('×3');
    expect(root.dataset.attacking).toBe('true');
    expect(root.dataset.selected).toBe('true');
    expect(root.dataset.actionable).toBe('true');
    expect(root.dataset.tapped).toBeUndefined();
    expect(root.getAttribute('aria-label')).toBe('Runeclaw Bear');
  });

  it('keeps the edge-bar and marker anchors present in full-card mode', () => {
    const root = renderFace(bear({ actionable: true, hasActivatedAbility: true, blocking: true }), {
      tier: 'support',
      art: { url: 'blob:full-card', full: true },
    });
    // The title plate and the status band are always rendered (empty in
    // full-card mode) so the pseudo-element channels have somewhere to draw.
    const body = inner(root);
    expect(body.getElementsByClassName(s.title)).toHaveLength(1);
    expect(body.getElementsByClassName(s.status)).toHaveLength(1);
  });

  it('never lets an illustration decide the card box, in any mode', () => {
    const box = (extra: Partial<CardFaceProps>) => {
      const root = renderFace(bear(), { tier: 'field', ...extra });
      const v = ['--face-w', '--face-h', '--foot-w', '--foot-h', '--band-art-h'].map((k) =>
        root.style.getPropertyValue(k),
      );
      cleanup();
      return v;
    };
    const bare = box({});
    expect(box({ art: { url: 'blob:a' } })).toEqual(bare);
    expect(box({ art: { url: 'blob:a', full: true } })).toEqual(bare);
  });
});

describe('keyword strip geometry (one svg, combined paths)', () => {
  it('serializes stroke and fill primitives from the shared glyph source', () => {
    const strokeOnly = glyphStripGeometry(['kw-flying']);
    expect(strokeOnly.stroke).toContain('M');
    expect(strokeOnly.fill).toBe('');
    const mixed = glyphStripGeometry(['kw-flying', 'kw-deathtouch']);
    expect(mixed.fill).not.toBe('');
    expect(mixed.width).toBeGreaterThan(strokeOnly.width);
  });

  it('closes polygons and keeps polylines open', () => {
    expect(glyphStripGeometry(['land-swamp']).stroke).toContain('Z');
    expect(glyphStripGeometry(['land-plains']).stroke).not.toContain('Z');
  });

  it('draws the glyphs as black strokes on a parchment plate (§7.5)', () => {
    const body = rule('\\.keywords');
    expect(body).toContain('background: var(--plate)');
    expect(body).toContain('color: var(--keyword-color)');
    expect(body).toContain('box-shadow: 0 0 0 var(--rule-w) var(--plate-rim)');
  });
});

describe('the frame material and light model (card-representation §3.1, §3.11)', () => {
  it('paints the slab, the darker bottom paper edge, and one gold hairline', () => {
    const slab = rule('\\.inner');
    expect(slab).toContain('var(--face-edge-shade)');
    expect(slab).toContain('var(--frame-edge-bottom)');
    expect(slab).toContain('border-radius: var(--face-radius)');
    const hairline = rule('\\.inner::before');
    expect(hairline).toContain('inset: var(--rule-inset)');
    expect(hairline).toContain('var(--rune-gold)');
    // Lit on the top/left run, shaded on the bottom/right — one key light.
    expect(hairline).toContain('border-right-color: var(--rune-gold-shade)');
    expect(hairline).toContain('border-bottom-color: var(--rune-gold-shade)');
  });

  it('keeps color identity an EDGE accent, never a body fill (§3.4)', () => {
    const root = renderFace(bear({ colorIdentity: 'R' }));
    // The slab is the neutral slate at every identity…
    expect(root.style.getPropertyValue('--face-body')).toBe(SURFACES.frameEdge);
    expect(root.style.getPropertyValue('--status-band')).toBe(SURFACES.statusBand);
    cleanup();
    const green = renderFace(bear({ colorIdentity: 'G' }));
    expect(green.style.getPropertyValue('--face-body')).toBe(SURFACES.frameEdge);
    // …and only the accent variable changes with identity.
    expect(green.style.getPropertyValue('--face-accent')).not.toBe(
      renderFace(bear({ colorIdentity: 'U' })).style.getPropertyValue('--face-accent'),
    );
  });

  it('floats the information on discrete parchment plates with gold rims', () => {
    for (const plate of ['\\.title', '\\.type', '\\.rules', '\\.pt']) {
      const body = rule(plate);
      expect(body, plate).toContain('background: var(--plate)');
      expect(body, plate).toContain('var(--plate-rim)');
      expect(body, plate).toContain('color: var(--plate-ink)');
    }
  });

  it('resolves every band from the metrics, never from a literal', () => {
    for (const band of ['\\.title', '\\.identityStrip', '\\.type', '\\.rules', '\\.status']) {
      expect(rule(band), band).toMatch(/top: var\(--band-\w+-top\)/);
      expect(rule(band), band).toMatch(/height: var\(--band-\w+-h\)/);
    }
  });

  it('anchors the art window on the same band variables as the metrics', () => {
    const root = renderFace(bear(), { tier: 'field' });
    const m = faceMetrics('field', 'permanent');
    expect(Number.parseFloat(root.style.getPropertyValue('--band-art-h'))).toBeCloseTo(
      m.bands.art.h,
      1,
    );
    expect(rule('\\.artField')).toContain('top: var(--band-art-top)');
    expect(rule('\\.artField')).toContain('height: var(--band-art-h)');
  });
});

describe('motion contract (ADR 0030: transform/opacity only)', () => {
  it('transitions only transform and opacity', () => {
    const transitions = css.match(/transition:[^;]+;/g) ?? [];
    expect(transitions.length).toBeGreaterThan(0);
    for (const t of transitions) {
      const cleaned = t.replace('transition:', '').replace(';', '');
      if (cleaned.trim() === 'none') continue;
      for (const part of cleaned.split(',')) {
        expect(part.trim()).toMatch(/^(transform|opacity)\s/);
      }
    }
  });

  it('snaps every transition under prefers-reduced-motion', () => {
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
    const rmBlock = css.slice(css.indexOf('@media (prefers-reduced-motion: reduce)'));
    expect(rmBlock).toContain('transition: none');
  });

  it('never hard-codes a color — every paint rides a token custom property', () => {
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(rules).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(rules).not.toMatch(/rgba?\(/);
  });

  it('never hard-codes a card dimension — every length rides the metrics', () => {
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    // No authored px length at all: the frame's whole geometry is resolved in
    // `theme.ts` from the token ratios and published as custom properties.
    expect(rules).not.toMatch(/:\s*-?\d+(\.\d+)?px/);
  });

  it('stacks full-card art below every authoritative overlay (image 0, above 1)', () => {
    // jsdom computes no stacking, so the contract is pinned at the source: the
    // art image sits at z-index 0 (in the `CardArt` primitive's stylesheet,
    // which owns every image) and every overlay at 1 or above here.
    const artCss = readFileSync(join(HERE, 'card-art.module.css'), 'utf8');
    expect(artCss.match(/\.full\s*\{[^}]*\}/s)?.[0] ?? '').toContain('z-index: 0');
    expect(artCss.match(/\.window\s*\{[^}]*\}/s)?.[0] ?? '').toContain('z-index: 0');
    for (const overlay of [
      '\\.title',
      '\\.identityStrip',
      '\\.status',
      '\\.pt',
      '\\.badge',
      '\\.tab',
      '\\.cost',
    ]) {
      expect(rule(overlay), overlay).toMatch(/z-index: [1-9]/);
    }
    expect(rule('\\.inner::after')).toContain('z-index: 1');
  });
});
