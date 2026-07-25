/**
 * The DOM card face — the **surface contract** (card-representation §4), the
 * per-tier information budget (§8), the DOM node ceiling (§9), and the
 * compression ladder (§10). Issue #529.
 *
 * jsdom performs no layout and applies no CSS module, so nothing here claims a
 * rendered geometry: what is asserted is the declared contract — which bands
 * and channels each surface renders, which it must NOT, and how many element
 * nodes that costs. The painted result is browser verification.
 */
import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PALETTE, RUNE_FRAME } from '../../tokens';
import type { CardDisplayData } from '../cardFactory';
import { CardFace, type CardFaceProps } from './CardFace';
import { BATTLEFIELD_TIERS, faceFootprint, type CardFaceTier } from './theme';
import s from './card-face.module.css';

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
function renderFace(data: CardDisplayData, tier: CardFaceTier, extra: Partial<CardFaceProps> = {}) {
  const { container } = render(<CardFace data={data} tier={tier} {...extra} />);
  return container.firstElementChild as HTMLElement;
}

/** Total element count of a face, root included. */
function nodeCount(root: HTMLElement): number {
  return root.querySelectorAll('*').length + 1;
}

/**
 * The maximum-supported face: every channel lit, a five-symbol cost, a keyword
 * overflow mixing stroke and fill primitives, three counter kinds plus damage
 * plus `blocked ×N`, and an ×N fold. The ceiling is hard and input-independent,
 * so THIS is the face every budget assertion measures — not a favorable fixture.
 */
const maximal = (overrides: Partial<CardDisplayData> = {}) =>
  bear({
    manaCost: '{2}{G}{G}{W}{W}',
    keywords: [
      'flying',
      'deathtouch',
      'reach',
      'vigilance',
      'haste',
      'trample',
      'lifelink',
      'first_strike',
      'double_strike',
    ],
    counters: [
      { kind: '+1/+1', count: 3 },
      { kind: 'charge', count: 2 },
      { kind: 'stun', count: 1 },
    ],
    markedDamage: 4,
    blockedBy: 3,
    stackCount: 5,
    summoningSick: true,
    tapped: true,
    selected: true,
    targeting: true,
    dimmed: true,
    actionable: true,
    attacking: true,
    blocking: true,
    hasActivatedAbility: true,
    ...overrides,
  });

describe('the surface contract (card-representation §4)', () => {
  it('renders the battlefield permanent with NO mana cost and NO type bar', () => {
    for (const tier of ['mini', 'support', 'field'] as const) {
      const root = renderFace(bear(), tier);
      expect(root.dataset.kind, tier).toBe('permanent');
      // The name (as prose above `mini`, as the accessible name everywhere) and
      // the server-computed P/T are present…
      expect(root.getAttribute('aria-label'), tier).toBe('Runeclaw Bear');
      if (tier !== 'mini') expect(root.textContent, tier).toContain('Runeclaw Bear');
      expect(root.textContent, tier).toContain('2/2');
      // …and the two bands the frame family removes are absent. This is a
      // normative rule of §3.3, transcribed from all three baselines — not a
      // truncation, so it must not be "restored" by a later change.
      expect(root.textContent, tier).not.toContain('Creature — Bear');
      expect(root.textContent, tier).not.toContain('{1}');
      expect(root.textContent, tier).not.toMatch(/1·G|1\s*G/);
      cleanup();
    }
  });

  it('renders the hand and stack card with all four bands', () => {
    for (const tier of ['hand', 'stack'] as const) {
      const root = renderFace(bear(), tier, { rulesText: 'Deal 3 damage to any target.' });
      expect(root.dataset.kind, tier).toBe('card');
      expect(root.textContent, tier).toContain('Runeclaw Bear');
      expect(root.textContent, tier).toContain('Creature — Bear');
      expect(root.textContent, tier).toContain('Deal 3 damage to any target.');
      expect(root.textContent, tier).toContain('2/2');
      // The cost disc is the full card's title-right channel: one pip per symbol.
      expect(root.textContent, tier).toContain('G');
      cleanup();
    }
  });

  it('renders a battlefield land as the resource tile, not a square plaque', () => {
    const tile = renderFace(
      bear({
        name: 'Forest',
        typeLine: 'Basic Land — Forest',
        landGlyph: 'land-forest',
        landTile: true,
      }),
      'field',
    );
    expect(tile.dataset.kind).toBe('land');
    expect(tile.style.getPropertyValue('--face-w')).toBe('96px');
    expect(tile.style.getPropertyValue('--face-h')).toBe('66px');
    // No title bar on a basic land tile — the glyph carries identity (§4).
    expect(tile.textContent).not.toContain('Forest');
    expect(tile.querySelector('svg')?.getAttribute('aria-label')).toBe('forest');
  });

  it('never collapses a nonbasic or actionable land to an anonymous tile (§15.9)', () => {
    // Nonbasic: the caller supplies no basic-land glyph, so the strip appears.
    const nonbasic = renderFace(
      bear({ name: 'Moonlit Causeway', typeLine: 'Land', landTile: true }),
      'field',
    );
    expect(nonbasic.textContent).toContain('Moonlit Causeway');
    cleanup();
    // Actionable basic: the strip appears even though a glyph is available.
    const actionable = renderFace(
      bear({
        name: 'Forest',
        typeLine: 'Basic Land — Forest',
        landGlyph: 'land-forest',
        landTile: true,
        actionable: true,
      }),
      'field',
    );
    expect(actionable.textContent).toContain('Forest');
  });

  it('turns a land back into an ordinary portrait card off the battlefield', () => {
    for (const tier of ['hand', 'stack', 'inspect'] as const) {
      const root = renderFace(bear({ typeLine: 'Land', landTile: true }), tier);
      expect(root.dataset.kind, tier).toBe('card');
      cleanup();
    }
  });

  it('carries the same frame anatomy through every tier — never a different box', () => {
    // Issue #529: simplify progressively, never switch to unrelated rectangles.
    for (const tier of BATTLEFIELD_TIERS) {
      const root = renderFace(bear(), tier);
      // Every battlefield face keeps the slab, an art window and the status
      // band with the authoritative P/T plate.
      expect(root.querySelector('[data-monogram]'), tier).not.toBeNull();
      expect(root.textContent, tier).toContain('2/2');
      expect(root.style.getPropertyValue('--band-art-h'), tier).not.toBe('');
      expect(root.style.getPropertyValue('--band-status-h'), tier).not.toBe('');
      cleanup();
    }
  });

  it('trades the mini title bar for the colour-identity strip (§8.4)', () => {
    const root = renderFace(bear({ keywords: ['flying'], hasActivatedAbility: true }), 'mini');
    const body = root.firstElementChild!;
    // One band node either way — the parchment name plate steps aside and the
    // strip stands in its box — so the swap costs nothing against the ceiling.
    expect(body.getElementsByClassName(s.title)).toHaveLength(0);
    expect(body.getElementsByClassName(s.identityStrip)).toHaveLength(1);
    // The strip carries no prose: identity moves to the accent it paints, the
    // glyph plate, and the inspect path, exactly as at `chip`.
    expect(root.textContent).not.toContain('Runeclaw Bear');
    expect(root.style.getPropertyValue('--face-accent')).toBe(PALETTE.G);
    expect(root.querySelector('svg')?.getAttribute('aria-label')).toContain('flying');
    // …and the card is never anonymous: the accessible name is on the root, and
    // the state channels the title band carried are still declared.
    expect(root.getAttribute('aria-label')).toBe('Runeclaw Bear');
    expect(root.dataset.ability).toBe('true');
    // The P/T plate — the authoritative surface — is untouched.
    expect(root.textContent).toContain('2/2');
  });

  it('keeps the rungs on either side of `mini` composed as they were', () => {
    // `support` keeps the parchment name plate…
    const support = renderFace(bear(), 'support').firstElementChild!;
    expect(support.getElementsByClassName(s.title)).toHaveLength(1);
    expect(support.getElementsByClassName(s.identityStrip)).toHaveLength(0);
    cleanup();
    // …and `chip` keeps no title band at all — the strip is one rung, not a new
    // floor applied to everything below it.
    const chip = renderFace(bear(), 'chip').firstElementChild!;
    expect(chip.getElementsByClassName(s.title)).toHaveLength(0);
    expect(chip.getElementsByClassName(s.identityStrip)).toHaveLength(0);
  });

  it('drops only the chip title bar, and keeps the glyph + P/T (§8.4)', () => {
    const root = renderFace(
      bear({ name: 'Forest', typeLine: 'Basic Land — Forest', landGlyph: 'land-forest' }),
      'chip',
    );
    expect(root.textContent).not.toContain('Forest');
    expect(root.querySelector('svg')).not.toBeNull();
    expect(root.style.getPropertyValue('--band-title-h')).toBe('0px');
  });
});

describe('the information budget per tier (card-representation §8)', () => {
  it('mini / support / field carry identity, P/T, glyphs, badges and the marker', () => {
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
      // Identity is the name plate at `support` and above and the colour
      // identity strip at `mini` (§8.4); the accessible name is on the root at
      // every rung, so the card is never anonymous to assistive technology.
      expect(root.getAttribute('aria-label'), tier).toBe('Runeclaw Bear');
      if (tier !== 'mini') expect(root.textContent, tier).toContain('Runeclaw Bear');
      expect(root.textContent, tier).toContain('2/2');
      expect(root.textContent, tier).toContain('+1/+1 ×2');
      expect(root.textContent, tier).toContain('1 dmg');
      expect(root.querySelector('svg')?.getAttribute('aria-label'), tier).toContain('flying');
      // The latent-ability marker is a state channel, not extra content.
      expect(root.dataset.ability, tier).toBe('true');
      cleanup();
    }
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
    expect(root.textContent).toContain('Creature — Bear');
  });

  it('caps the glyph plates per tier and overflows into the extra plate (§3.9)', () => {
    const many = bear({
      keywords: ['flying', 'reach', 'vigilance', 'haste', 'trample', 'lifelink'],
    });
    // The cap tightens as the tier steps down — ladder rung 5 (§10).
    const shown = (tier: CardFaceTier) => {
      const root = renderFace(many, tier);
      const n = Number(root.querySelector('svg')!.getAttribute('data-keywords'));
      const overflow = root.querySelector('[data-plate-extra]')!.getAttribute('data-plate-extra');
      cleanup();
      return { n, overflow };
    };
    const field = shown('field');
    const mini = shown('mini');
    expect(field.n).toBeGreaterThan(mini.n);
    // Nothing is silently dropped: the overflow count rides the extra plate.
    expect(field.overflow).toBe(`+${6 - field.n}`);
    expect(mini.overflow).toBe(`+${6 - mini.n}`);
  });

  it('shows summoning sickness as a plate, never as a dim (§6.2)', () => {
    const root = renderFace(bear({ summoningSick: true }), 'field');
    expect(root.dataset.sick).toBe('true');
    expect(root.querySelector('[data-plate-extra]')!.getAttribute('data-plate-extra')).not.toBe('');
    // The alpha channel stays free for tap — sickness no longer dims.
    expect(root.style.getPropertyValue('--face-alpha')).toBe('1');
  });

  it('moves the ×N fold onto the top-edge tab channel (§7.4, §15.4)', () => {
    const root = renderFace(bear({ stackCount: 14 }), 'field');
    expect(root.textContent).toContain('×14');
    expect(root.dataset.stack).toBe('14');
    // …and the bottom-right stays the P/T channel, never the count.
    expect(root.textContent).toContain('2/2');
  });
});

describe('the DOM node budget (presentation-budgets §Performance, §9)', () => {
  it('keeps the maximum-supported battlefield face within 12 element nodes', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      for (const art of [undefined, { url: 'blob:art' }, { url: 'blob:art', full: true }]) {
        for (const landTile of [false, true]) {
          const root = renderFace(maximal({ landTile, landGlyph: 'land-forest' }), tier, { art });
          expect(
            nodeCount(root),
            `${tier} land=${landTile} art=${art?.url ?? 'none'}`,
          ).toBeLessThanOrEqual(12);
          cleanup();
        }
      }
    }
  });

  it('never scales the face with its inputs (hard ceiling, not typical-case)', () => {
    const small = nodeCount(
      renderFace(
        bear({
          keywords: ['flying', 'deathtouch'],
          counters: [{ kind: '+1/+1', count: 1 }],
        }),
        'field',
      ),
    );
    cleanup();
    // Nine keywords instead of two and three counter kinds plus damage plus
    // `blocked ×N` instead of one counter: the one-svg strip (combined paths)
    // and the consolidated badge row keep the count identical.
    const large = nodeCount(
      renderFace(maximal({ stackCount: 1, summoningSick: false, landTile: false }), 'field'),
    );
    expect(large).toBe(small);
  });

  it('costs exactly one node for the ×N fold, at any count', () => {
    const single = nodeCount(renderFace(bear(), 'support'));
    cleanup();
    const folded = nodeCount(renderFace(bear({ stackCount: 14 }), 'support'));
    cleanup();
    const huge = nodeCount(renderFace(bear({ stackCount: 240 }), 'support'));
    expect(folded).toBe(single + 1);
    expect(huge).toBe(folded);
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
      hasActivatedAbility: true,
    });
    const root = renderFace(lit, 'field', { elevation: 'held' });
    // Rings and blooms, edge bars, the gold hairline, tap, dim, elevation, the
    // ability dot and the pile splay all ride box-shadows, pseudo-elements,
    // transform and opacity — never elements.
    expect(nodeCount(root)).toBe(baseline);
  });

  it('spends the SAME node on the art window whether or not art is loaded', () => {
    // This is what buys the frame a generous art window inside the ceiling: the
    // procedural color-identity field and a `CardArt` image are one node each,
    // and the image replaces the field rather than nesting inside it.
    for (const tier of ['mini', 'support', 'field'] as const) {
      const procedural = nodeCount(renderFace(bear({ keywords: ['flying'] }), tier));
      cleanup();
      const illustrated = nodeCount(
        renderFace(bear({ keywords: ['flying'] }), tier, {
          art: { url: 'blob:art' },
        }),
      );
      expect(illustrated, tier).toBe(procedural);
      cleanup();
    }
  });

  it('keeps every badge, glyph count and fold readable in the consolidated nodes', () => {
    const root = renderFace(maximal(), 'field');
    for (const label of ['+1/+1 ×3', 'charge ×2', 'stun', '4 dmg', 'blocked ×3', '×5', '2/2']) {
      expect(root.textContent).toContain(label);
    }
  });
});

describe('the compression and degradation ladder (card-representation §10)', () => {
  const tiers: CardFaceTier[] = ['field', 'support', 'mini', 'chip'];

  it('never removes the P/T plate at any rung (steps 5–6 guarantee)', () => {
    for (const tier of tiers) {
      const root = renderFace(bear({ keywords: ['flying', 'trample', 'lifelink'] }), tier);
      expect(root.textContent, tier).toContain('2/2');
      cleanup();
    }
  });

  it('simplifies secondary glyphs before anything else (step 5)', () => {
    const data = bear({
      keywords: ['flying', 'reach', 'vigilance', 'haste', 'trample', 'lifelink'],
    });
    const counts = tiers
      .filter((t) => t !== 'chip')
      .map((tier) => {
        const root = renderFace(data, tier);
        const n = Number(root.querySelector('svg')!.getAttribute('data-keywords'));
        cleanup();
        return n;
      });
    // Monotonically non-increasing as the rung steps down.
    for (let i = 1; i < counts.length; i += 1) {
      expect(counts[i]).toBeLessThanOrEqual(counts[i - 1]!);
    }
  });

  it('has NO battlefield rules area to shorten — step 4 is a no-op (§16.24)', () => {
    for (const tier of tiers) {
      const root = renderFace(bear(), tier, { rulesText: 'Whenever this creature attacks…' });
      expect(root.textContent, tier).not.toContain('Whenever this creature attacks');
      cleanup();
    }
  });

  it('is a pure function of the current view — never sticky state', () => {
    // A card that stepped down and back up renders byte-identically to one that
    // never moved: degradation is a function of the tier it is asked for.
    const first = renderFace(bear({ keywords: ['flying'] }), 'field').outerHTML;
    cleanup();
    renderFace(bear({ keywords: ['flying'] }), 'mini');
    cleanup();
    const again = renderFace(bear({ keywords: ['flying'] }), 'field').outerHTML;
    expect(again).toBe(first);
  });

  it('keeps the fold silhouette capped while the tab carries the exact count', () => {
    const four = renderFace(bear({ stackCount: 4 }), 'field');
    const capped = four.style.getPropertyValue('--splay-layers');
    cleanup();
    const forty = renderFace(bear({ stackCount: 40 }), 'field');
    expect(forty.style.getPropertyValue('--splay-layers')).toBe(capped);
    expect(forty.textContent).toContain('×40');
  });
});

describe('the reserved footprint (tap sweeps its own box)', () => {
  it('publishes the swept box for a tapped card at every tier and silhouette', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      for (const landTile of [false, true]) {
        const expected = faceFootprint(tier, true, landTile ? 'land' : 'permanent');
        const root = renderFace(bear({ tapped: true, landTile }), tier);
        expect(root.style.getPropertyValue('--foot-w'), tier).toBe(`${expected.w}px`);
        expect(root.style.getPropertyValue('--foot-h'), tier).toBe(`${expected.h}px`);
        cleanup();
      }
    }
  });

  it('publishes the drawn card box separately from the reserved footprint', () => {
    const root = renderFace(bear({ tapped: true }), 'field');
    expect(root.style.getPropertyValue('--face-w')).toBe('96px');
    expect(root.style.getPropertyValue('--face-h')).toBe('96px');
    expect(root.style.getPropertyValue('--foot-w')).not.toBe('96px');
  });

  it('publishes the whole band stack the stylesheet draws from', () => {
    const root = renderFace(bear(), 'field');
    for (const key of [
      '--band-title-top',
      '--band-title-h',
      '--band-art-top',
      '--band-art-h',
      '--band-status-top',
      '--band-status-h',
      '--rule-w',
      '--rule-inset',
      '--frame-edge-w',
      '--frame-edge-bottom',
      '--face-radius',
      '--plate-radius',
    ]) {
      expect(root.style.getPropertyValue(key), key).toMatch(/^[\d.]+px$/);
    }
    // The outer radius is the authored fraction of W, not a fixed pixel value.
    expect(Number.parseFloat(root.style.getPropertyValue('--face-radius'))).toBeCloseTo(
      RUNE_FRAME.radius * 96,
      2,
    );
  });
});
