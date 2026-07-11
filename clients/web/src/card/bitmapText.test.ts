/**
 * The production text path: where a 2D canvas can rasterize a glyph atlas, the
 * card factory draws every label as a cached {@link BitmapText} tinted to its
 * token color — reusing one shared {@link BitmapFont} instead of re-rasterizing a
 * `Text` texture per card (ui-requirements §11). This suite installs a fake-but-
 * functional 2D context so `BitmapFont.from` succeeds under jsdom, exercising the
 * branch the headless `cardFactory.test.ts` (plain `Text`) can't reach.
 *
 * It lives in its own file on purpose: installing the atlas mutates pixi's global
 * `BitmapFont` registry, and Vitest isolates modules per file, so the sibling
 * factory suite still sees the no-atlas fallback.
 */
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { BitmapFont, BitmapText, Container, Text } from 'pixi.js';
import { buildCardDisplay, type CardDisplayData } from './cardFactory';
import { PT_TEXT } from '../tokens';

/** A minimal 2D context that no-ops draws but returns real metrics/pixel buffers,
 * enough for `BitmapFont.from` to build a glyph atlas without a GPU. */
function makeCtx(): unknown {
  const target: Record<string, unknown> = {
    measureText: (t: string) => ({
      width: t.length * 6,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: t.length * 6,
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
    }),
    getImageData: (_x: number, _y: number, w: number, h: number) => ({
      data: new Uint8ClampedArray(Math.max(1, w) * Math.max(1, h) * 4),
      width: w,
      height: h,
    }),
    createImageData: (w: number, h: number) => ({
      data: new Uint8ClampedArray(Math.max(1, w) * Math.max(1, h) * 4),
      width: w,
      height: h,
    }),
    canvas: { width: 256, height: 256 },
  };
  return new Proxy(target, {
    get: (t, p: string) => (p in t ? t[p] : () => undefined),
    set: (t, p: string, v) => ((t[p] = v), true),
  });
}

const hexToNumber = (hex: string): number => parseInt(hex.slice(1), 16);

const bears: CardDisplayData = {
  name: 'Grizzly Bears',
  typeLine: 'Creature — Bear',
  colorIdentity: 'G',
  manaCost: '{1}{G}',
  power: '2',
  toughness: '2',
};

/** Collect every BitmapText node in a display object, depth first. */
function bitmapTexts(node: Container): BitmapText[] {
  const found: BitmapText[] = [];
  const walk = (n: Container): void => {
    for (const child of n.children) {
      if (child instanceof BitmapText) found.push(child);
      if (child instanceof Container) walk(child as Container);
    }
  };
  walk(node);
  return found;
}

let realGetContext: HTMLCanvasElement['getContext'];

beforeAll(() => {
  realGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = (() => makeCtx()) as never;
});

afterAll(() => {
  HTMLCanvasElement.prototype.getContext = realGetContext;
});

describe('buildCardDisplay bitmap text path', () => {
  it('draws card labels as tinted BitmapText, not per-build Text', () => {
    const card = buildCardDisplay(bears, 'hand');
    const bmps = bitmapTexts(card);
    expect(bmps.length).toBeGreaterThan(0);

    // No re-rasterizing dynamic Text anywhere in the tree.
    const walkForText = (n: Container): boolean =>
      n.children.some(
        (c) => c instanceof Text || (c instanceof Container && walkForText(c as Container)),
      );
    expect(walkForText(card)).toBe(false);

    // P/T renders verbatim and is tinted with its color-identity token.
    const pt = bmps.find((t) => t.text === '2/2');
    expect(pt).toBeDefined();
    expect(pt?.tint).toBe(hexToNumber(PT_TEXT.G));
  });

  it('rasterizes the shared atlas once and reuses it across many cards', () => {
    // Prime the (memoized) font, then spy: further builds must not re-install it.
    buildCardDisplay(bears);
    expect(BitmapFont.available['RuneCard']).toBeDefined();

    const fromSpy = vi.spyOn(BitmapFont, 'from');
    for (let i = 0; i < 25; i += 1) buildCardDisplay({ ...bears, name: `Bear ${i}` });
    expect(fromSpy).not.toHaveBeenCalled(); // atlas reused, never rebuilt
    fromSpy.mockRestore();
  });
});
