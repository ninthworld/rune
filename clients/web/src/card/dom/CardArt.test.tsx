/**
 * Card-art containment (issue #527) — the card frame, not the image file,
 * decides a card's footprint.
 *
 * Two layers of assertion, because jsdom does no layout:
 *
 * 1. **Component level.** Every art surface renders exactly one `CardArt`
 *    image in a declared mode, and the face's published footprint/box variables
 *    (`--foot-w`, `--foot-h`, `--face-w`, `--face-h` — the only things that
 *    decide the reserved rectangle) are byte-identical across "no art", "art
 *    arrived", "art arrived late", "full-card art", every tier, and every
 *    intrinsic image size. Nothing about the image reaches the box.
 * 2. **Stylesheet level.** The containment contract itself is CSS, so it is
 *    pinned at the source: every mode declares an explicit used width and an
 *    explicit height (or a declared `aspect-ratio`), never `auto`; every mode
 *    declares its `object-fit`; and no other stylesheet in the client sizes a
 *    card image. `width/height: auto` on an absolutely positioned replaced
 *    element is the exact defect this issue fixes — it takes the image's
 *    intrinsic size and drops the over-constrained inset — so it is asserted
 *    against directly.
 *
 * What jsdom cannot show — that the painted raster is actually clipped to the
 * rounded frame — is browser verification, left to the maintainer per the repo
 * testing policy.
 */
import { cleanup, render } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { ART, FRAME, SURFACES } from '../../tokens';
import type { CardDisplayData } from '../cardFactory';
import { CardArt, CardArtSlot } from './CardArt';
import { CardFace, type CardFaceProps } from './CardFace';
import {
  BATTLEFIELD_TIERS,
  cardArtSlotVars,
  cardArtVars,
  faceFootprint,
  type CardFaceTier,
} from './theme';

afterEach(cleanup);

const HERE = dirname(fileURLToPath(import.meta.url));

/** Tiers that can carry an illustration, battlefield plus screen-space. */
const ART_TIERS: CardFaceTier[] = [...BATTLEFIELD_TIERS, 'hand', 'inspect'];

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

function renderFace(tier: CardFaceTier, extra: Partial<CardFaceProps> = {}) {
  const { container } = render(<CardFace data={bear()} tier={tier} {...extra} />);
  return container.firstElementChild as HTMLElement;
}

/**
 * The complete set of geometry a face publishes — the reserved rectangle and
 * the drawn card box. If art can move a card, it moves one of these.
 */
function boxOf(root: HTMLElement): Record<string, string> {
  const keys = ['--foot-w', '--foot-h', '--face-w', '--face-h', '--face-header-h'];
  return Object.fromEntries(keys.map((k) => [k, root.style.getPropertyValue(k)]));
}

/** Every art case a surface must survive with an identical footprint. */
const ART_CASES: { label: string; art: CardFaceProps['art'] }[] = [
  { label: 'no image', art: undefined },
  { label: 'window illustration', art: { url: 'blob:window' } },
  { label: 'full-card image', art: { url: 'blob:full', full: true } },
  // The store hands over a URL identically whether the blob came from the
  // IndexedDB cache synchronously, from a late network response, or from a
  // retry — the face has no other input, which is the point.
  { label: 'cache-hit image', art: { url: 'blob:cached' } },
  { label: 'late image', art: { url: 'blob:late' } },
  // A failed load never reaches the face at all: `artStore` publishes no URL
  // for a failed entry, so the failure case IS the "no image" case (asserted
  // separately below), and a *broken* URL still gets a mode-fixed box.
  { label: 'broken url', art: { url: 'blob:does-not-decode' } },
];

describe('CardArt: the tier fixes the footprint, the image never does (#527)', () => {
  it('publishes an identical card box for every art case, at every tier', () => {
    for (const tier of ART_TIERS) {
      const baseline = boxOf(renderFace(tier));
      cleanup();
      for (const { label, art } of ART_CASES) {
        const root = renderFace(tier, { art });
        expect(boxOf(root), `${tier} / ${label}`).toEqual(baseline);
        cleanup();
      }
    }
  });

  it('keeps the box identical when art arrives after the first paint', () => {
    const { container, rerender } = render(<CardFace data={bear()} tier="field" />);
    const root = () => container.firstElementChild as HTMLElement;
    const before = boxOf(root());
    expect(root().querySelector('img')).toBeNull();
    rerender(<CardFace data={bear()} tier="field" art={{ url: 'blob:late' }} />);
    expect(boxOf(root())).toEqual(before);
    expect(root().querySelector('img')).not.toBeNull();
    // …and identical again when the art store is cleared under the card.
    rerender(<CardFace data={bear()} tier="field" />);
    expect(boxOf(root())).toEqual(before);
    expect(root().querySelector('img')).toBeNull();
  });

  it('still reserves exactly the tier footprint the scene staged', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      for (const tapped of [false, true]) {
        const expected = faceFootprint(tier, tapped);
        const root = renderFace(tier, {
          data: bear({ tapped }),
          art: { url: 'blob:4096px-square' },
        });
        expect(root.style.getPropertyValue('--foot-w')).toBe(`${expected.w}px`);
        expect(root.style.getPropertyValue('--foot-h')).toBe(`${expected.h}px`);
        cleanup();
      }
    }
  });

  it('ignores the intrinsic dimensions of whatever decoded into the frame', () => {
    // jsdom loads no pixels, so the intrinsic size is stated on the element the
    // way a browser would report it. The assertion is that NOTHING in the
    // render path consults it: the box stays the mode's, at every extreme.
    const intrinsics: [string, number, number][] = [
      ['huge square', 4096, 4096],
      ['tiny square', 32, 32],
      ['extreme landscape', 4000, 40],
      ['extreme portrait', 40, 4000],
      ['undecoded', 0, 0],
    ];
    const baseline = boxOf(renderFace('field'));
    cleanup();
    const probe = render(<CardArt url="blob:probe" mode="window" />);
    const declaredStyle = (probe.container.firstElementChild as HTMLElement).getAttribute('style');
    cleanup();
    for (const [label, naturalWidth, naturalHeight] of intrinsics) {
      const root = renderFace('field', { art: { url: `blob:${label}` } });
      const img = root.querySelector('img')!;
      Object.defineProperty(img, 'naturalWidth', { value: naturalWidth });
      Object.defineProperty(img, 'naturalHeight', { value: naturalHeight });
      expect(boxOf(root), label).toEqual(baseline);
      // Same declared box for every intrinsic size — and only declared values.
      expect(img.getAttribute('style'), label).toBe(declaredStyle);
      cleanup();
    }
  });

  it('never lets the image carry its own dimensions onto the element', () => {
    for (const tier of ART_TIERS) {
      for (const art of [{ url: 'blob:a' }, { url: 'blob:b', full: true }]) {
        const img = renderFace(tier, { art }).querySelector('img');
        if (!img) continue;
        // No width/height presentation attributes and no inline sizing: the
        // box comes from the mode's stylesheet rule alone.
        expect(img.getAttribute('width')).toBeNull();
        expect(img.getAttribute('height')).toBeNull();
        for (const property of ['width', 'height', 'aspect-ratio', 'object-fit']) {
          expect(img.style.getPropertyValue(property)).toBe('');
        }
        expect(img.getAttribute('data-art-mode')).not.toBeNull();
        cleanup();
      }
    }
  });

  it('routes every art surface through the one primitive, one node each', () => {
    for (const tier of ART_TIERS) {
      for (const art of [{ url: 'blob:a' }, { url: 'blob:b', full: true }]) {
        const root = renderFace(tier, { art });
        const images = root.querySelectorAll('img');
        // Dense tiers draw no window illustration; when a surface does draw
        // one it is exactly one CardArt element in a declared mode.
        expect(images.length).toBeLessThanOrEqual(1);
        for (const img of Array.from(images)) {
          expect(['window', 'full', 'panel', 'panelFull']).toContain(
            img.getAttribute('data-art-mode'),
          );
        }
        cleanup();
      }
    }
  });

  it('stays inside the ≤ 12-node battlefield budget in full-card mode too', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      const root = renderFace(tier, { art: { url: 'blob:full', full: true } });
      expect(root.querySelectorAll('*').length + 1).toBeLessThanOrEqual(12);
      cleanup();
    }
  });

  it('publishes its focal anchor, ratios, and empty fill from tokens', () => {
    const vars = cardArtVars() as Record<string, string>;
    expect(vars['--art-focus']).toBe(`${ART.focusX * 100}% ${ART.focusY * 100}%`);
    expect(vars['--art-panel-aspect']).toBe(`${ART.panelAspect}`);
    expect(vars['--art-card-aspect']).toBe(`${ART.cardAspect}`);
    expect(vars['--art-empty']).toBe(SURFACES.cardBody);
    // The anchor is a deliberate off-center crop point, not a default.
    expect(ART.focusY).toBeLessThan(0.5);
    // Every image carries them, so containment does not depend on the surface.
    const { container } = render(<CardArt url="blob:a" mode="panel" />);
    const img = container.firstElementChild as HTMLElement;
    for (const key of Object.keys(vars)) {
      expect(img.style.getPropertyValue(key)).toBe(vars[key]);
    }
  });

  it('is decoration only: hidden from assistive tech, never a drag source', () => {
    const { container } = render(<CardArt url="blob:a" mode="window" />);
    const img = container.firstElementChild as HTMLImageElement;
    expect(img.getAttribute('alt')).toBe('');
    expect(img.getAttribute('aria-hidden')).toBe('true');
    expect(img.getAttribute('draggable')).toBe('false');
  });
});

describe('CardArt containment contract (stylesheet source, #527)', () => {
  const css = readFileSync(join(HERE, 'card-art.module.css'), 'utf8');
  const rule = (selector: string): string =>
    new RegExp(`\\.${selector}\\s*\\{[^}]*\\}`, 's').exec(css)?.[0] ?? '';
  const MODES = ['window', 'full', 'panel', 'panelFull'];

  it('gives every mode an explicit width and an explicit height or ratio', () => {
    for (const mode of MODES) {
      const body = rule(mode);
      expect(body, mode).not.toBe('');
      expect(body, `${mode} declares a width`).toMatch(/(^|[;{]\s*)width:/);
      expect(body, `${mode} declares a height or a ratio`).toMatch(
        /(^|[;{]\s*)(height|aspect-ratio):/,
      );
    }
  });

  it('never leaves a dimension to the image (the #527 defect itself)', () => {
    // `width: auto` / `height: auto` on an absolutely positioned replaced
    // element is intrinsic sizing — the blowout. It may not reappear.
    for (const mode of MODES) {
      expect(rule(mode), mode).not.toMatch(/(width|height):\s*auto/);
    }
    expect(css).not.toMatch(/(width|height):\s*auto/);
  });

  it('fits the raster into the box with a declared focal anchor', () => {
    expect(rule('art')).toContain('object-fit: cover');
    expect(rule('art')).toContain('object-position: var(--art-focus)');
    // Cover everywhere — the one exception is a whole printed card, which is
    // shown whole rather than cropped, in a box that is still declared.
    for (const mode of ['window', 'full', 'panel']) {
      expect(rule(mode), mode).not.toContain('object-fit');
    }
    expect(rule('panelFull')).toContain('object-fit: contain');
  });

  it('clips replaced content and reads as the token fill when empty', () => {
    expect(rule('art')).toContain('overflow: hidden');
    expect(rule('art')).toContain('background: var(--art-empty)');
    expect(rule('art')).toContain('box-sizing: border-box');
  });

  it('keeps the frame image below every authoritative overlay', () => {
    for (const mode of ['window', 'full']) {
      expect(rule(mode), mode).toContain('z-index: 0');
    }
  });

  it('paints no literal color or size of its own — tokens only', () => {
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(rules).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(rules).not.toMatch(/rgba?\(/);
    // The only bare lengths are the 0/1px frame-radius correction; every other
    // dimension arrives as a custom property.
    expect(rules).not.toMatch(/:\s*\d{2,}px/);
  });
});

/**
 * The second half of "no layout shift" (issue #527): a mode-fixed <img> only
 * holds its rectangle once it is MOUNTED. A surface that mounts it when the
 * download lands still grows by an art block, and one that swaps `panel` for
 * the much taller `panelFull` still jumps. The screen-space surfaces therefore
 * reserve the slot permanently, at one size for both modes.
 *
 * jsdom performs no layout, so nothing here claims to measure a rendered
 * height. What IS asserted is the declared contract that decides it: the slot
 * element exists in every state, it is byte-identical across late load and
 * across an art-mode change, and the stylesheet sizes it from a single
 * mode-independent rule whose ratio contains both modes.
 */
describe('the reserved art slot: presence and mode cannot shift a surface (#527)', () => {
  /** Everything declared about the slot that could make it a different box. */
  function slotOf(root: HTMLElement) {
    const slot = root.querySelector<HTMLElement>('[data-art-slot]');
    expect(slot).not.toBeNull();
    return {
      tag: slot!.tagName,
      className: slot!.className,
      style: slot!.getAttribute('style'),
    };
  }

  it('reserves the slot at the inspect tier with no art at all', () => {
    const root = renderFace('inspect');
    const slot = root.querySelector<HTMLElement>('[data-art-slot]');
    expect(slot).not.toBeNull();
    expect(slot!.querySelector('img')).toBeNull();
    // The empty region reads as a card with no illustration, not as a hole:
    // the frame's own procedural monogram, on the token fill.
    expect(slot!.getAttribute('data-art-mono')).toBe('R');
  });

  it('keeps the reserved slot identical when art arrives late', () => {
    const { container, rerender } = render(<CardFace data={bear()} tier="inspect" />);
    const root = () => container.firstElementChild as HTMLElement;
    const before = slotOf(root());
    expect(root().querySelector('img')).toBeNull();
    rerender(<CardFace data={bear()} tier="inspect" art={{ url: 'blob:late' }} />);
    // The image landed INSIDE the rectangle that was already there…
    const img = root().querySelector('img')!;
    expect(img.parentElement!.getAttribute('data-art-slot')).not.toBeNull();
    expect(slotOf(root())).toEqual(before);
    // Clearing the store under the card puts the monogram back, same slot.
    rerender(<CardFace data={bear()} tier="inspect" />);
    expect(slotOf(root())).toEqual(before);
    expect(root().querySelector('img')).toBeNull();
  });

  it('keeps the reserved slot identical when the art mode changes', () => {
    const { container, rerender } = render(
      <CardFace data={bear()} tier="inspect" art={{ url: 'blob:a' }} />,
    );
    const root = () => container.firstElementChild as HTMLElement;
    const windowed = slotOf(root());
    expect(root().querySelector('img')!.getAttribute('data-art-mode')).toBe('panel');
    rerender(<CardFace data={bear()} tier="inspect" art={{ url: 'blob:a', full: true }} />);
    // Only the contained image changed mode; the reservation did not move.
    expect(root().querySelector('img')!.getAttribute('data-art-mode')).toBe('panelFull');
    expect(slotOf(root())).toEqual(windowed);
    // The whole-card image is shown inside the slot, not in place of the face:
    // the reading surface keeps its text under both art styles.
    expect(root().textContent).toContain('Runeclaw Bear');
    expect(root().textContent).toContain('Creature — Bear');
  });

  it('renders the same slot standalone, whatever the surface supplies', () => {
    const empty = render(<CardArtSlot mode="panel" monogram="R" />);
    const emptyShape = slotOf(empty.container);
    cleanup();
    const filled = render(<CardArtSlot mode="panelFull" url="blob:a" monogram="R" />);
    expect(slotOf(filled.container)).toEqual(emptyShape);
  });

  it('adds no reserved slot — and no node — to any battlefield tier', () => {
    for (const tier of BATTLEFIELD_TIERS) {
      for (const { art } of ART_CASES) {
        const root = renderFace(tier, { art });
        expect(root.querySelector('[data-art-slot]'), tier).toBeNull();
        expect(root.querySelectorAll('*').length + 1, tier).toBeLessThanOrEqual(12);
        cleanup();
      }
    }
  });

  it('publishes the slot geometry from tokens, never inline', () => {
    const vars = cardArtSlotVars() as Record<string, string>;
    expect(vars['--art-slot-aspect']).toBe(`${ART.slotAspect}`);
    expect(vars['--art-radius']).toBe(`${ART.radius}px`);
    expect(vars['--art-empty']).toBe(SURFACES.cardBody);
    expect(vars['--art-mono-size']).toBe(`${ART.slotMonogram}px`);
    expect(vars['--art-mono-alpha']).toBe(`${FRAME.monogramAlpha}`);
    expect(vars['--art-mono-color']).toBe(SURFACES.typeText);
    const { container } = render(<CardArtSlot mode="panel" />);
    const slot = container.firstElementChild as HTMLElement;
    for (const key of Object.keys(vars)) expect(slot.style.getPropertyValue(key)).toBe(vars[key]);
    // No literal dimension reaches the element: the vars above are all of it.
    for (const property of ['width', 'height', 'aspect-ratio']) {
      expect(slot.style.getPropertyValue(property)).toBe('');
    }
  });

  it('reserves a box that contains BOTH screen-space modes at any width', () => {
    // Height at width W is W / aspect, so the containing box is the SMALLER
    // ratio. Reserving per-mode would fix the late load and leave the mode
    // switch — which is why there is one slot ratio, not two.
    expect(ART.slotAspect).toBeLessThanOrEqual(ART.panelAspect);
    expect(ART.slotAspect).toBeLessThanOrEqual(ART.cardAspect);
    expect(ART.slotAspect).toBe(Math.min(ART.panelAspect, ART.cardAspect));
  });
});

describe('the reserved art slot, at the stylesheet source (#527)', () => {
  const css = readFileSync(join(HERE, 'card-art.module.css'), 'utf8');
  const rule = (selector: string): string =>
    new RegExp(`\\.${selector}\\s*\\{[^}]*\\}`, 's').exec(css)?.[0] ?? '';

  it('sizes the slot from one declared, mode-independent rule', () => {
    const body = rule('slot');
    expect(body).not.toBe('');
    expect(body).toMatch(/(^|[;{]\s*)width:\s*100%/);
    expect(body).toContain('aspect-ratio: var(--art-slot-aspect)');
    expect(body).toContain('box-sizing: border-box');
    // A consuming surface's border must ride inside the reserved rectangle.
    expect(body).not.toMatch(/(^|[;{]\s*)(height|min-height|max-height|padding|margin):/);
    // Nothing may stretch or compress it inside a flex column.
    expect(body).toContain('flex: none');
  });

  it('has no per-mode variant of the slot anywhere in the sheet', () => {
    // If a mode could select a different `.slot` rule, the mode could move the
    // surface — the exact defect this slot exists to remove.
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    const selectors = new Set(rules.match(/\.slot[^{,\s]*/g) ?? []);
    expect(selectors).toEqual(new Set(['.slot', '.slot::after']));
  });

  it('centers the shorter mode and fills the leftover with the token matte', () => {
    const body = rule('slot');
    expect(body).toContain('align-items: center');
    expect(body).toContain('justify-content: center');
    expect(body).toContain('background: var(--art-empty)');
    expect(body).toContain('overflow: hidden');
  });

  it('draws the empty state as the frame monogram, with zero extra nodes', () => {
    const mark = rule('slot::after');
    expect(mark).toContain('content: attr(data-art-mono)');
    expect(mark).toContain('position: absolute');
    // Accent where the surface publishes a frame, neutral token where it does not.
    expect(mark).toContain('color: var(--face-accent, var(--art-mono-color))');
    expect(mark).toContain('font-size: var(--art-mono-size)');
  });

  it('keeps both screen-space modes full-width so neither can exceed the slot', () => {
    for (const mode of ['panel', 'panelFull']) {
      expect(rule(mode), mode).toMatch(/(^|[;{]\s*)width:\s*100%/);
      expect(rule(mode), mode).toContain('flex: none');
    }
  });
});

describe('no other stylesheet sizes a card image (#527)', () => {
  const read = (...parts: string[]): string =>
    readFileSync(resolve(HERE, '..', '..', ...parts), 'utf8');
  const ruleOf = (css: string, selector: string): string =>
    new RegExp(`\\.${selector}\\s*\\{[^}]*\\}`, 's').exec(css)?.[0] ?? '';

  it('leaves the card frame stylesheet with no art image rules at all', () => {
    const css = read('card', 'dom', 'card-face.module.css');
    for (const dead of ['artWindow', 'artFull', 'inspectArt']) {
      expect(ruleOf(css, dead), dead).toBe('');
    }
  });

  it('leaves the inspect panel chrome with decoration only', () => {
    const css = read('table', 'chrome.module.css');
    const body = ruleOf(css, 'inspectArt');
    expect(body).not.toBe('');
    expect(body).not.toMatch(/(^|[;{]\s*)(width|height|max-height|max-width):/);
    expect(body).not.toContain('object-fit');
    expect(body).not.toContain('aspect-ratio');
    // And the chrome carries NO art-mode variant at all: the reserved slot is
    // one identical element under both ADR 0024 art styles, so the panel's
    // no-shift guarantee does not rest on which declarations are geometric.
    expect(ruleOf(css, 'inspectArtFull')).toBe('');
    expect(css).not.toContain('inspectArtFull');
  });
});
