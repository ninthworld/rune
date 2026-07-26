/**
 * The card frame's **plates** (issue #570, `card-representation.md` §3.12).
 *
 * Three contracts are asserted, and none of them is a rendering claim: jsdom
 * applies no CSS module, resolves no `border-image`, and paints nothing.
 * Whether the bevel reads as metal in a browser is browser verification and
 * belongs to the maintainer.
 *
 * 1. **The projection is the manifest.** Every URL, slice, and band the
 *    stylesheet consumes comes from the committed manifest — never a hash
 *    transcribed into TypeScript.
 * 2. **The material is constant.** Each surface publishes ONE property that
 *    already carries its slice and its band, the band as a `calc()` on the
 *    `--face-w` the tier publishes anyway. That is what keeps one asset serving
 *    every tier without the face's style attribute — the thing the plane
 *    reconciler rewrites on every view — growing per plate value.
 * 3. **No plate is load-bearing.** Nothing a plate publishes may reach a
 *    property that can affect layout, so a plate that 404s, a browser that
 *    declines `border-image`, and a build with no frames tree at all produce
 *    the same boxes as the frame rendered before this set landed.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { PRODUCTION_FRAME_PLATES } from '../../assets/productionManifest';
import { RUNE_FRAME } from '../../tokens';
import { PLATE_MATERIAL, bandRatio } from './plates';
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
  it('publishes every plate from the manifest, never a transcribed hash', () => {
    for (const [property, key] of [
      ['--plate-frame-edge', 'frameEdge'],
      ['--plate-art-seam', 'artSeam'],
      ['--plate-header', 'headerField'],
      ['--plate-info', 'infoStrip'],
      ['--plate-status', 'statusStrip'],
      ['--plate-pt', 'ptPlate'],
      ['--plate-identity', 'identityWeave'],
    ] as const) {
      const plate = PRODUCTION_FRAME_PLATES[key];
      expect(PLATE_MATERIAL[property], property).toContain(`url(${plate.src})`);
    }
  });

  it('carries the band as a calc on the tier width, so one asset serves every tier', () => {
    // The nine-slice contract, and the reason the material can be constant: the
    // drawn band is the authored ratio of W at the chip and at the inspect
    // panel alike, resolved by the browser from a variable the face publishes
    // for its own geometry — so neither the plate nor the band is per-tier data.
    for (const [property, key] of [
      ['--plate-frame-edge', 'frameEdge'],
      ['--plate-header', 'headerField'],
      ['--plate-pt', 'ptPlate'],
    ] as const) {
      expect(PLATE_MATERIAL[property], property).toContain(
        `calc(var(--face-w) * ${PRODUCTION_FRAME_PLATES[key].band})`,
      );
      // The slice travels with the source, so the stylesheet cannot pair one
      // plate's image with another's inset.
      expect(PLATE_MATERIAL[property], property).toContain(`${PRODUCTION_FRAME_PLATES[key].slice}`);
    }
    // Not a coincidence to be re-derived anywhere: the frame edge's band IS
    // `ruleInset + rule`, which is what lets the plate hand off to the card
    // body with no seam — and it is pushed back out to the card's boundary.
    expect(bandRatio('frameEdge')).toBeCloseTo(RUNE_FRAME.ruleInset + RUNE_FRAME.rule, 10);
    expect(PLATE_MATERIAL['--plate-frame-edge']).toContain('/ var(--rule-inset)');
  });

  it('reaches the face as ONE property per surface', () => {
    // Publishing the source, slice, and band separately added sixteen custom
    // properties to every card and cost ~29% of the reconnect rebuild budget on
    // a 120-permanent board. The face's style attribute is rewritten by the
    // plane reconciler on every view, so the whole material set has to stay
    // this small.
    const vars = cardFaceVars(bear, 'field', 'rest') as Record<string, string>;
    // `--plate-radius` and `--plate-rim` predate this set and are frame tokens,
    // not plate values, so the count is taken from what this module publishes.
    expect(Object.keys(PLATE_MATERIAL)).toHaveLength(7);
    for (const [property, value] of Object.entries(PLATE_MATERIAL)) {
      expect(vars[property], property).toBe(value);
    }
  });

  it('answers an inert value for a plate that did not ship', () => {
    // A category is optional under ADR 0031. `none` is a valid `border-image`
    // and a valid background layer, so an absent plate is an inert declaration
    // rather than an invalid one that would drop the whole rule.
    expect(bandRatio('notAPlate')).toBe(0);
  });
});

describe('the frame stylesheet composes plates without depending on them', () => {
  it('keeps the gold hairline a real border under the frame-edge plate', () => {
    const inner = rule(faceCss, '\\.inner::before');
    expect(inner).toContain('border: var(--rule-w) solid var(--rune-gold)');
    expect(inner).toContain('border-image: var(--plate-frame-edge)');
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
      expect(body, selector).toContain(`border-image: var(${plate})`);
    }
    expect(rule(faceCss, '\\.status')).toContain('background: var(--status-band)');
    // `fill` paints the middle patch: these are surfaces the text sits in, not
    // rings around it. It travels in the published value, with the slice.
    expect(PLATE_MATERIAL['--plate-header']).toContain(' fill /');
    expect(PLATE_MATERIAL['--plate-art-seam']).not.toContain('fill');
  });

  it('gives the P/T plate its own asset and its own tighter band', () => {
    // §3.9 makes the P/T the single authoritative characteristic surface; §3.12
    // makes it a distinct object rather than another printed field, so it does
    // not share the strip's plate.
    expect(rule(faceCss, '\\.pt')).toContain('border-image: var(--plate-pt)');
    expect(PLATE_MATERIAL['--plate-pt']).not.toBe(PLATE_MATERIAL['--plate-header']);
  });

  it('carries every plate on a ZERO-width border, so no plate can move a box', () => {
    // The degraded frame has to be the frame as it rendered before this set
    // landed — not merely one that still has the right colours. A carrier with
    // a real border width would shrink its content box, re-crop the art, and
    // shift a line of text whether or not the plate ever painted; that is a
    // layout change caused by an asset, which is precisely what "never
    // load-bearing" must exclude.
    //
    // `border-image-width` is a multiple of `border-width` ONLY when given as a
    // `<number>`; given a length it is independent, so the image still paints
    // its full band and overflows inward. `border-style` may not be `none`,
    // which is why the zero-width declaration is present at all.
    for (const [css, selector] of [
      [faceCss, '\\.artField'],
      [faceCss, '\\.title'],
      [faceCss, '\\.type'],
      [faceCss, '\\.rules'],
      [faceCss, '\\.status'],
      [faceCss, '\\.pt'],
      [artCss, '\\.window'],
    ] as const) {
      const body = rule(css, selector);
      expect(body, selector).toContain('border: 0 solid transparent');
      expect(body, selector).not.toContain('box-sizing');
    }
  });

  it('lets no plate value reach a property that can affect layout', () => {
    // The mechanical form of the contract above, over BOTH stylesheets: every
    // custom property this module publishes may only be read by a paint-only
    // property. Nothing else can then make a box depend on an asset — including
    // a future edit that reaches for a plate band as a padding or an inset.
    const paintOnly = /^(?:border-image|background)$/;
    let seen = 0;
    for (const css of [faceCss, artCss]) {
      for (const declaration of css.replace(/\/\*[\s\S]*?\*\//g, '').split(';')) {
        const [property, ...rest] = declaration.split(':');
        const value = rest.join(':');
        for (const name of Object.keys(PLATE_MATERIAL)) {
          if (!value.includes(`var(${name}`)) continue;
          seen += 1;
          expect(property.trim(), `${name} in ${property.trim()}`).toMatch(paintOnly);
        }
      }
    }
    // Every surface actually composes one, so the sweep above is not vacuous.
    expect(seen).toBeGreaterThanOrEqual(9);
  });

  it('surrounds the art window identically whether or not an illustration loaded', () => {
    // Contract 3 of `card-art.module.css`, extended to the frame's material:
    // the procedural field and the <img> take the SAME seam plate, so the
    // window's surround is load-state invariant.
    expect(rule(faceCss, '\\.artField')).toContain('border-image: var(--plate-art-seam)');
    // The frame-relative variable is optional on a surface that publishes no
    // frame at all (the inspect popover), so it carries its inert default.
    expect(rule(artCss, '\\.window')).toContain('border-image: var(--plate-art-seam, none)');
  });

  it('draws the identity surfaces THROUGH the material, never as a flat block', () => {
    for (const selector of ['\\.artField', '\\.identityStrip']) {
      const body = rule(faceCss, selector);
      expect(body, selector).toContain('var(--plate-identity),');
      expect(body, selector).toContain('background-blend-mode: overlay');
    }
    // The accent stays the bottom layer and the material is tinted through it.
    expect(rule(faceCss, '\\.identityStrip')).toContain(
      'background: var(--plate-identity), var(--face-accent)',
    );
  });
});
