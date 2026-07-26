/**
 * The card frame's **plates** (issue #570, `card-representation.md` §3.12).
 *
 * Two contracts are asserted, and neither is a rendering claim: jsdom applies
 * no CSS module, resolves no `border-image`, and paints nothing. Whether the
 * bevel reads as metal in a browser is browser verification and belongs to the
 * maintainer.
 *
 * 1. **The projection is the manifest.** Every URL, slice, and band the
 *    stylesheet consumes comes from the committed manifest — never a hash
 *    transcribed into TypeScript — and every band resolves from the tier's card
 *    width, which is what lets one asset serve the hand fan and the chip.
 * 2. **No plate is load-bearing.** Every rule that composes one still declares
 *    the token treatment underneath it, so a plate that 404s, a browser that
 *    declines `border-image`, and a build with the frames tree deleted all
 *    leave the frame rendering exactly as it did before this set landed.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { PRODUCTION_FRAME_PLATES } from '../../assets/productionManifest';
import { RUNE_FRAME, TIER } from '../../tokens';
import { PLATE_MATERIAL, bandRatio, plateGeometryVars } from './plates';
import { cardFaceVars } from './theme';
import type { CardDisplayData } from '../cardFactory';

const HERE = dirname(fileURLToPath(import.meta.url));
const faceCss = readFileSync(join(HERE, 'card-face.module.css'), 'utf8');
const artCss = readFileSync(join(HERE, 'card-art.module.css'), 'utf8');

function rule(css: string, selector: string): string {
  return new RegExp(`${selector}\\s*\\{[^}]*\\}`, 's').exec(css)?.[0] ?? '';
}

const bear: CardDisplayData = {
  name: 'Runeclaw Bear',
  typeLine: 'Creature — Bear',
  colorIdentity: 'G',
  manaCost: '{1}{G}',
  power: '2',
  toughness: '2',
};

describe('the frame-plate projection', () => {
  it('publishes every plate URL from the manifest, never a transcribed hash', () => {
    for (const [property, key] of [
      ['--plate-frame-edge', 'frameEdge'],
      ['--plate-art-seam', 'artSeam'],
      ['--plate-header', 'headerField'],
      ['--plate-info', 'infoStrip'],
      ['--plate-status', 'statusStrip'],
      ['--plate-pt', 'ptPlate'],
      ['--plate-identity', 'identityWeave'],
    ] as const) {
      expect(PLATE_MATERIAL[property], property).toBe(`url(${PRODUCTION_FRAME_PLATES[key].src})`);
    }
  });

  it('resolves every band from the tier width, so one asset serves every tier', () => {
    // The whole nine-slice contract in one assertion: the drawn band is the
    // authored ratio of W at the chip and at the inspect panel alike, so the
    // bevel is proportionate at both and neither needs its own file.
    for (const tier of ['chip', 'inspect'] as const) {
      const w = TIER[tier].w;
      const vars = plateGeometryVars(w);
      expect(vars['--plate-edge-band'], tier).toBe(`${round(bandRatio('frameEdge') * w)}px`);
      expect(vars['--plate-surface-band'], tier).toBe(`${round(bandRatio('headerField') * w)}px`);
      expect(vars['--plate-identity-size'], tier).toBe(`${round(0.5 * w)}px`);
    }
    expect(bandRatio('frameEdge')).toBeCloseTo(RUNE_FRAME.ruleInset + RUNE_FRAME.rule, 10);
  });

  it('reaches the face through the same variable channel as every other length', () => {
    const vars = cardFaceVars(bear, 'field', 'rest') as Record<string, string>;
    expect(vars['--plate-frame-edge']).toBe(PLATE_MATERIAL['--plate-frame-edge']);
    expect(vars['--plate-edge-band']).toBe(`${round(bandRatio('frameEdge') * TIER.field.w)}px`);
  });

  it('answers `none` for a plate that did not ship', () => {
    // A category is optional under ADR 0031. `none` is a valid
    // `border-image-source`, so an absent plate is an inert declaration rather
    // than an invalid one that would drop the whole rule.
    expect(bandRatio('notAPlate')).toBe(0);
  });
});

describe('the frame stylesheet composes plates without depending on them', () => {
  it('keeps the gold hairline a real border under the frame-edge plate', () => {
    const inner = rule(faceCss, '\\.inner::before');
    expect(inner).toContain('border: var(--rule-w) solid var(--rune-gold)');
    expect(inner).toContain('border-image-source: var(--plate-frame-edge)');
    // Pushed out to the card's own boundary and drawn exactly as deep as the
    // hairline sits, so the plate hands off to the card body with no seam.
    expect(inner).toContain('border-image-outset: var(--rule-inset)');
    expect(inner).toContain('border-image-width: var(--plate-edge-band)');
  });

  it('keeps the parchment fill and the gold rim under every surface plate', () => {
    for (const [selector, plate] of [
      ['\\.title', '--plate-header'],
      ['\\.type', '--plate-info'],
      ['\\.rules', '--plate-info'],
    ] as const) {
      const body = rule(faceCss, selector);
      expect(body, selector).toContain('background: var(--plate)');
      expect(body, selector).toContain('box-shadow: 0 0 0 var(--rule-w) var(--plate-rim)');
      expect(body, selector).toContain(`border-image-source: var(${plate})`);
      // `fill` paints the middle patch: these are surfaces the text sits in,
      // not rings around it.
      expect(body, selector).toContain('border-image-slice: var(--plate-surface-slice) fill');
      // The carrier must not change the band's box, so the plate can never
      // move a line of text: border-box plus a hairline border, and the line
      // box gives back exactly those two hairlines.
      expect(body, selector).toContain('box-sizing: border-box');
      expect(body, selector).toContain('border: var(--rule-w) solid transparent');
    }
    expect(rule(faceCss, '\\.status')).toContain('background: var(--status-band)');
    expect(rule(faceCss, '\\.pt')).toContain('border-image-source: var(--plate-pt)');
  });

  it('gives the P/T plate its own asset and its own tighter band', () => {
    // §3.9 makes the P/T the single authoritative characteristic surface; §3.12
    // makes it a distinct object rather than another printed field, so it does
    // not share the strip's plate.
    const pt = rule(faceCss, '\\.pt');
    expect(pt).toContain('border-image-slice: var(--plate-pt-slice) fill');
    expect(pt).toContain('border-image-width: var(--plate-pt-band)');
  });

  it('draws the identity surfaces THROUGH the material, never as a flat block', () => {
    for (const selector of ['\\.artField', '\\.identityStrip']) {
      const body = rule(faceCss, selector);
      expect(body, selector).toContain('background-image: var(--plate-identity)');
      expect(body, selector).toContain('background-blend-mode: overlay');
      expect(body, selector).toContain(
        'background-size: var(--plate-identity-size) var(--plate-identity-size)',
      );
    }
  });

  it('surrounds the art window identically whether or not an illustration loaded', () => {
    // Contract 3 of `card-art.module.css`, extended to the frame's material:
    // the procedural field and the <img> take the SAME seam plate on the SAME
    // band, so the window's surround is load-state invariant.
    expect(rule(faceCss, '\\.artField')).toContain('border-image-source: var(--plate-art-seam)');
    const window = rule(artCss, '\\.window');
    expect(window).toContain('border-image-source: var(--plate-art-seam, none)');
    expect(window).toContain('border-image-width: var(--plate-seam-band, 0px)');
    // The frame-relative variables are optional on a surface that publishes no
    // frame at all (the inspect popover), so each carries its inert default.
    expect(window).toContain('border: var(--plate-seam-band, 0px) solid transparent');
  });

  it('adds no element to any face — every plate rides a node that already exists', () => {
    // The ≤ 12-node battlefield ceiling (presentation-budgets §Performance) is
    // input-independent and binding. `border-image` and `background-image` are
    // paint on existing boxes, so the whole set costs zero nodes.
    expect(faceCss).not.toMatch(/\.plate[A-Z]/);
    const plateRules = faceCss.match(/border-image-source/g) ?? [];
    expect(plateRules.length).toBeGreaterThanOrEqual(6);
  });
});

function round(value: number): number {
  return Math.round(value * 100) / 100;
}
