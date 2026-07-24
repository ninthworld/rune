/**
 * DOM card face — state channels (visual-system §7), presentation states
 * (§5: tap, elevation, ×N piles, art modes), and the transform/opacity-only
 * motion contract (ADR 0030, issue #479). Channels are asserted through the
 * face's stable data-attributes and CSS custom properties — the same hooks the
 * consuming surfaces use — plus a source-level check that transitions never
 * animate anything but transform and opacity and that reduced motion snaps.
 */
import { cleanup, render } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { FRAME, TAP } from '../../tokens';
import type { CardDisplayData } from '../cardFactory';
import { CardFace, type CardFaceProps } from './CardFace';
import { PROVISIONAL, faceAlpha } from './theme';
import { glyphStripGeometry } from './glyphStrip';
import s from './card-face.module.css';

afterEach(cleanup);

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

describe('CardFace non-color state channels (visual-system §7)', () => {
  it('flags every channel through its stable data-attribute', () => {
    const root = renderFace(
      bear({
        tapped: true,
        selected: true,
        targeting: true,
        actionable: true,
        attacking: true,
        blocking: true,
        dimmed: true,
        hasActivatedAbility: true,
        stackCount: 3,
      }),
    );
    for (const key of [
      'tapped',
      'selected',
      'targeting',
      'actionable',
      'attacking',
      'blocking',
      'dimmed',
      'ability',
    ]) {
      expect(root.dataset[key]).toBe('true');
    }
    expect(root.dataset.stack).toBe('3');
  });

  it('rotates a tapped card ~25° and reserves the swept footprint', () => {
    const tapped = renderFace(bear({ tapped: true }));
    const degrees = Math.round((TAP.angle * 180) / Math.PI);
    expect(tapped.style.getPropertyValue('--tap-rot')).toBe(`${degrees}deg`);
    cleanup();
    const untapped = renderFace(bear());
    expect(untapped.style.getPropertyValue('--tap-rot')).toBe('0deg');
  });

  it('dims a tapped card — but a tapped attacker keeps full presence', () => {
    expect(faceAlpha(bear({ tapped: true }))).toBe(FRAME.tappedAlpha);
    expect(faceAlpha(bear({ tapped: true, attacking: true }))).toBe(1);
    // An ineligible card during targeting recedes multiplicatively.
    expect(faceAlpha(bear({ tapped: true, dimmed: true }))).toBeCloseTo(
      FRAME.tappedAlpha * FRAME.dimmedAlpha,
    );
    const root = renderFace(bear({ tapped: true }));
    expect(root.style.getPropertyValue('--face-alpha')).toBe(`${FRAME.tappedAlpha}`);
  });

  it('walks the elevation ladder through transform + shadow variables', () => {
    for (const elevation of ['rest', 'lifted', 'held'] as const) {
      const root = renderFace(bear(), { elevation });
      expect(root.dataset.elevation).toBe(elevation);
      expect(root.style.getPropertyValue('--lift')).toBe(`${PROVISIONAL.lift[elevation]}px`);
      expect(root.style.getPropertyValue('--elev-shadow')).toBe(PROVISIONAL.shadow[elevation]);
      cleanup();
    }
  });
});

describe('CardFace art modes (ADR 0024, unchanged)', () => {
  it('renders procedurally with the monogram placeholder by default', () => {
    const root = renderFace(bear());
    expect(root.querySelector('img')).toBeNull();
    expect(inner(root).getAttribute('data-monogram')).toBe('R');
  });

  it('window mode: illustration at field/hand only, budget unchanged', () => {
    const art = { url: 'blob:test-art' };
    for (const tier of ['field', 'hand'] as const) {
      const root = renderFace(bear(), { tier, art });
      expect(root.querySelector('img')?.getAttribute('src')).toBe('blob:test-art');
      // The information budget stays: name, type, P/T still render.
      expect(root.textContent).toContain('Runeclaw Bear');
      expect(root.textContent).toContain('2/2');
      // The monogram placeholder yields to the illustration.
      expect(inner(root).getAttribute('data-monogram')).toBe('');
      cleanup();
    }
    // Dense tiers keep their full procedural budget — no window art.
    for (const tier of ['mini', 'support'] as const) {
      const root = renderFace(bear(), { tier, art });
      expect(root.querySelector('img')).toBeNull();
      cleanup();
    }
  });

  it('full-card mode replaces the face; server-computed overlays stay', () => {
    const root = renderFace(bear({ markedDamage: 2, attacking: true, selected: true }), {
      tier: 'support',
      art: { url: 'blob:full-card', full: true },
    });
    expect(root.querySelector('img')?.getAttribute('src')).toBe('blob:full-card');
    // Printed identity is suppressed (the image carries it)…
    expect(root.textContent).not.toContain('Runeclaw Bear');
    expect(root.textContent).not.toContain('Creature — Bear');
    // …while the authoritative overlays remain: P/T pill, damage badge, and the
    // combat/selection channels.
    expect(root.textContent).toContain('2/2');
    expect(root.textContent).toContain('2 dmg');
    expect(root.dataset.attacking).toBe('true');
    expect(root.dataset.selected).toBe('true');
    expect(root.getAttribute('aria-label')).toBe('Runeclaw Bear');
  });

  it('keeps the actionable bar and ability marker anchored above full art', () => {
    const root = renderFace(bear({ actionable: true, hasActivatedAbility: true }), {
      tier: 'support',
      art: { url: 'blob:full-card', full: true },
    });
    // The channels stay lit…
    expect(root.dataset.actionable).toBe('true');
    expect(root.dataset.ability).toBe('true');
    // …and their pseudo-element anchors exist even in full-card mode: the name
    // (combat bars) and the type (ability marker dot) elements are always
    // rendered, empty, so the channels have somewhere to draw.
    const inner1 = inner(root);
    expect(inner1.getElementsByClassName(s.name)).toHaveLength(1);
    expect(inner1.getElementsByClassName(s.type)).toHaveLength(1);
    // The image stacks below every overlay (z-index 0 vs 1) — asserted at the
    // stylesheet level below, since jsdom does not compute stacking.
    expect(root.querySelector('img')?.className).toContain(s.artFull);
  });
});

describe('CardFace keyword strip geometry (one svg, combined paths)', () => {
  it('serializes stroke and fill primitives from the shared glyph source', () => {
    const strokeOnly = glyphStripGeometry(['kw-flying']);
    expect(strokeOnly.stroke).toContain('M');
    expect(strokeOnly.fill).toBe('');
    const mixed = glyphStripGeometry(['kw-flying', 'kw-deathtouch']);
    expect(mixed.fill).not.toBe('');
    // The second glyph draws offset one glyph box to the right.
    expect(mixed.width).toBeGreaterThan(strokeOnly.width);
  });

  it('closes polygons and keeps polylines open', () => {
    const swamp = glyphStripGeometry(['land-swamp']);
    expect(swamp.stroke).toContain('Z');
    const plains = glyphStripGeometry(['land-plains']);
    expect(plains.stroke).not.toContain('Z');
  });
});

describe('CardFace motion contract (ADR 0030: transform/opacity only)', () => {
  const css = readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), 'card-face.module.css'),
    'utf8',
  );

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

  it('stacks full-card art below every overlay (image 0, overlays 1)', () => {
    // jsdom computes no stacking, so the contract is pinned at the source: the
    // full-art image sits at z-index 0 and the content/overlay layer (name,
    // type, cost, P/T, badges, the gold-bar pseudo) at z-index 1.
    const artRule = css.match(/\.artFull\s*\{[^}]*\}/s)?.[0] ?? '';
    expect(artRule).toContain('z-index: 0');
    const overlayRule = css.match(/\.name,[\s\S]*?\{[^}]*\}/)?.[0] ?? '';
    expect(overlayRule).toContain('z-index: 1');
    const goldBarRule = css.match(/\.inner::before\s*\{[^}]*\}/s)?.[0] ?? '';
    expect(goldBarRule).toContain('z-index: 1');
  });
});
