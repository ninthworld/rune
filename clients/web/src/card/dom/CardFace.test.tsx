/**
 * DOM card face — information budget per tier and the DOM node budget
 * (issue #479). The budget per tier carries `ui-design-notes.md` §Card render;
 * the node ceiling is presentation-budgets §Performance (≤ 12 nodes per
 * battlefield-tier face).
 */
import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CardDisplayData } from '../cardFactory';
import { CardFace } from './CardFace';
import { BATTLEFIELD_TIERS, faceFootprint, type CardFaceTier } from './theme';

afterEach(cleanup);

/** A creature face exercising the full battlefield information budget. */
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

/** Render one face and return its root element. */
function renderFace(data: CardDisplayData, tier: CardFaceTier, extra = {}) {
  const { container } = render(<CardFace data={data} tier={tier} {...extra} />);
  return container.firstElementChild as HTMLElement;
}

/** Total element count of a face, root included. */
function nodeCount(root: HTMLElement): number {
  return root.querySelectorAll('*').length + 1;
}

describe('CardFace information budget per tier (ui-design-notes §Card render)', () => {
  it('chip: frame color, name, tap state — nothing else', () => {
    const root = renderFace(bear(), 'chip');
    expect(root.textContent).toContain('Runeclaw Bear');
    // No cost pips, no P/T pill, no type line at the digest tier.
    expect(root.textContent).not.toContain('2/2');
    expect(root.textContent).not.toContain('Creature');
    expect(root.querySelectorAll('span').length).toBe(0);
  });

  it('chip: a basic land renders its glyph in place of a name', () => {
    const root = renderFace(
      bear({ name: 'Forest', typeLine: 'Basic Land — Forest', landGlyph: 'land-forest' }),
      'chip',
    );
    expect(root.querySelector('svg')).not.toBeNull();
    expect(root.textContent).not.toContain('Forest');
  });

  it('mini / support / field add pips, P/T, keywords, badges, ability marker', () => {
    for (const tier of ['mini', 'support', 'field'] as const) {
      const root = renderFace(
        bear({
          keywords: ['flying'],
          counters: [{ kind: '+1/+1', count: 2 }],
          markedDamage: 1,
          hasActivatedAbility: true,
        }),
        tier,
      );
      expect(root.textContent).toContain('Runeclaw Bear');
      expect(root.textContent).toContain('Creature — Bear');
      expect(root.textContent).toContain('2/2');
      expect(root.textContent).toContain('+1/+1 ×2');
      expect(root.textContent).toContain('1 dmg');
      // Cost pips render one per symbol.
      expect(root.textContent).toContain('1');
      expect(root.textContent).toContain('G');
      // The keyword strip names its keywords accessibly.
      expect(root.querySelector('svg')?.getAttribute('aria-label')).toContain('flying');
      // The latent-ability marker is a state channel, not extra content.
      expect(root.dataset.ability).toBe('true');
    }
  });

  it('hand carries the field information set at a readable size', () => {
    const root = renderFace(bear({ keywords: ['trample'] }), 'hand');
    expect(root.textContent).toContain('Runeclaw Bear');
    expect(root.textContent).toContain('2/2');
    expect(root.querySelector('svg')).not.toBeNull();
  });

  it('inspect adds everything supplied — rules text included', () => {
    const root = renderFace(
      bear({ keywords: ['flying'], counters: [{ kind: 'charge', count: 3 }] }),
      'inspect',
      { rulesText: '{T}: Add {G}.\nFlying' },
    );
    expect(root.textContent).toContain('{T}: Add {G}.');
    expect(root.textContent).toContain('charge ×3');
    expect(root.textContent).toContain('2/2');
  });

  it('caps the keyword strip and degrades the overflow to +N', () => {
    const root = renderFace(
      bear({
        keywords: [
          'flying',
          'reach',
          'vigilance',
          'haste',
          'trample',
          'lifelink',
          'deathtouch',
          'first_strike',
          'double_strike',
        ],
      }),
      'mini',
    );
    const svg = root.querySelector('svg')!;
    expect(svg.getAttribute('data-overflow')).not.toBeNull();
    expect(svg.textContent).toContain('+');
  });
});

describe('CardFace DOM node budget (presentation-budgets §Performance)', () => {
  it('keeps a fully-loaded battlefield face within 12 element nodes', () => {
    // Every state channel lit at once, cost, a keyword mixing stroke+fill
    // primitives, a counter badge AND a damage badge: the maximal face the
    // budget guarantees.
    const data = bear({
      keywords: ['flying', 'deathtouch'],
      counters: [{ kind: '+1/+1', count: 2 }],
      markedDamage: 2,
      tapped: true,
      selected: true,
      actionable: true,
      attacking: true,
      hasActivatedAbility: true,
    });
    for (const tier of BATTLEFIELD_TIERS.filter((t) => t !== 'chip')) {
      const root = renderFace(data, tier);
      expect(nodeCount(root)).toBeLessThanOrEqual(12);
      cleanup();
    }
  });

  it('keeps the chip within the budget with room to spare', () => {
    const root = renderFace(
      bear({
        name: 'Forest',
        typeLine: 'Basic Land — Forest',
        landGlyph: 'land-forest',
        tapped: true,
        actionable: true,
        stackCount: 4,
      }),
      'chip',
    );
    expect(nodeCount(root)).toBeLessThanOrEqual(6);
  });

  it('adds ZERO nodes for every non-content state channel', () => {
    const base = bear({ keywords: ['flying'] });
    const baseline = nodeCount(renderFace(base, 'field'));
    cleanup();
    const lit = bear({
      keywords: ['flying'],
      tapped: true,
      selected: true,
      targeting: true,
      dimmed: true,
      actionable: true,
      attacking: true,
      blocking: true,
      summoningSick: false,
      hasActivatedAbility: true,
    });
    const root = renderFace(lit, 'field', { elevation: 'held' });
    // Rings, edge bars, tap, dim, elevation, and the ability marker all ride
    // box-shadows, pseudo-elements, transform, and opacity — never elements.
    expect(nodeCount(root)).toBe(baseline);
  });

  it('renders an ×N stack as one render plus exactly one badge node', () => {
    const single = nodeCount(renderFace(bear(), 'support'));
    cleanup();
    const stacked = renderFace(bear({ stackCount: 14 }), 'support');
    expect(nodeCount(stacked)).toBe(single + 1);
    expect(stacked.textContent).toContain('×14');
    expect(stacked.dataset.stack).toBe('14');
  });
});

describe('CardFace footprint (tap reserves the rotated bounding box)', () => {
  it('reserves the swept box for a tapped card at every tier', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      const untapped = faceFootprint(tier, false);
      const tapped = faceFootprint(tier, true);
      expect(tapped.w).toBeGreaterThan(untapped.w);
      expect(tapped.h).toBeGreaterThan(untapped.h);
      const root = renderFace(bear({ tapped: true }), tier);
      expect(root.style.getPropertyValue('--foot-w')).toBe(`${tapped.w}px`);
      expect(root.style.getPropertyValue('--foot-h')).toBe(`${tapped.h}px`);
      cleanup();
    }
  });
});
