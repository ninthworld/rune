/**
 * The shell's viewport, safe-area, and stacking contract (issue #528).
 *
 * These are geometry and source-consistency tests, and that is the honest limit
 * of what they prove: **jsdom computes no layout**. Vitest does not process the
 * CSS modules at all, so nothing here can show a rendered pixel, a resolved
 * `calc()`, or a real paint order. What it CAN prove, and does:
 *
 * - the band arithmetic every supported viewport is laid out from — containment
 *   and pairwise non-overlap (invariant I1), the hand band's fit (I2);
 * - that the stylesheets carry no bare shell-level `z-index` and that the
 *   `--rune-z-*` ladder in `tokens.css` is exactly `LAYER`, with the decision
 *   rung above every fixed region (I3);
 * - that the shipped defects cannot come back: a negative hand offset, a
 *   percentage fan span, a decision sheet ranked below the top bar, and an
 *   overlapping compact hand row.
 *
 * Whether the browser then paints those bands where the arithmetic says is the
 * maintainer's verification under the repo testing policy.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { CONTROL } from '../controls/controlTokens';
import {
  LAYER,
  SHELL,
  bottomChromeHeight,
  handBandHeight,
  handCardBounds,
  handFanFraction,
  handFanSpacing,
  isCompactShell,
  rectContains,
  rectsOverlap,
  shellBands,
  shellStyleVars,
  type ShellRect,
} from './shellLayout';

/**
 * The supported viewport range: `docs/design/presentation-budgets.md` §Device
 * and browser envelope (desktop down to 1280×800, tablet landscape 1180×820,
 * phone portrait 390×844) plus the extra geometries issue #528 names — the
 * 1280×720 short desktop, a 16:9 laptop, and a 2:1 ultrawide-ish desktop.
 */
const SUPPORTED = [
  { label: 'desktop 1280×720 (short 16:9)', width: 1280, height: 720 },
  { label: 'desktop 1280×800 (envelope floor)', width: 1280, height: 800 },
  { label: 'desktop 1440×900 (envelope reference)', width: 1440, height: 900 },
  { label: 'desktop 1680×945 (16:9)', width: 1680, height: 945 },
  { label: 'desktop 2048×1024 (2:1)', width: 2048, height: 1024 },
  { label: 'tablet landscape 1180×820', width: 1180, height: 820 },
  { label: 'phone portrait 390×844', width: 390, height: 844 },
];

/** The chrome regions that float over the scene, as (name, rect) pairs. */
function chromeOf(viewport: { width: number; height: number }): [string, ShellRect][] {
  const bands = shellBands(viewport);
  return [
    ['hand', bands.hand],
    ['cluster', bands.cluster],
  ];
}

describe('shell bands — invariant I1: containment and non-overlap', () => {
  for (const vp of SUPPORTED) {
    it(`keeps every chrome region inside the viewport at ${vp.label}`, () => {
      const bands = shellBands(vp);
      for (const [name, rect] of chromeOf(vp)) {
        expect(rect.w, `${name} width`).toBeGreaterThan(0);
        expect(rect.h, `${name} height`).toBeGreaterThan(0);
        expect(rectContains(bands.viewport, rect), `${name} inside viewport`).toBe(true);
      }
    });

    it(`never overlaps two chrome regions at ${vp.label}`, () => {
      const named = chromeOf(vp);
      for (let i = 0; i < named.length; i += 1) {
        for (let j = i + 1; j < named.length; j += 1) {
          const [aName, a] = named[i]!;
          const [bName, b] = named[j]!;
          expect(rectsOverlap(a, b), `${aName} overlaps ${bName}`).toBe(false);
        }
      }
    });

    it(`never lets chrome stand on the staging box at ${vp.label}`, () => {
      // The invariant that replaced "no region overlaps another" when the scene
      // grew to fill the viewport. Chrome floats OVER the scene by design; what
      // it may never do is cover a staged object, and the staging box is the
      // rect the plane is allowed to put those in.
      const bands = shellBands(vp);
      for (const [name, rect] of chromeOf(vp)) {
        expect(rectsOverlap(rect, bands.staging), `${name} covers staged objects`).toBe(false);
      }
    });

    it(`gives the battlefield the whole viewport at ${vp.label}`, () => {
      // ADR 0032's first consequence: cards get the viewport back. The arena is
      // visible behind the controls rather than ending where they begin.
      const bands = shellBands(vp);
      expect(bands.scene).toEqual(bands.viewport);
    });

    it(`leaves the staging box at or above the plane's floor at ${vp.label}`, () => {
      // `LivePlane` clamps its staged height to `sceneMinH`; a staging box
      // shorter than that stages a plane taller than the box that clips it,
      // cutting off the receiver's own band.
      expect(shellBands(vp).staging.h).toBeGreaterThanOrEqual(SHELL.sceneMinH);
    });
  }

  it('insets the whole shell by the browser safe area, not just the edges', () => {
    const inset = { top: 47, bottom: 34, left: 0, right: 0 };
    const bands = shellBands({ width: 390, height: 844 }, inset);
    expect(bands.viewport).toEqual({ x: 0, y: 47, w: 390, h: 763 });
    for (const [name, rect] of [
      ['scene', bands.scene],
      ['hand', bands.hand],
      ['cluster', bands.cluster],
    ] as [string, ShellRect][]) {
      expect(rectContains(bands.viewport, rect), `${name} inside safe viewport`).toBe(true);
    }
    // The cluster's bottom clears the home indicator.
    expect(bands.cluster.y + bands.cluster.h).toBeLessThanOrEqual(844 - inset.bottom);
  });
});

describe('shell bands — the stack stage claims width only when drawn', () => {
  // #534: "Empty stack/log consumes no permanent battlefield width." Reserving
  // the right-hand column unconditionally would be simpler and would fail this.
  it('costs the battlefield nothing while the stack is empty', () => {
    const empty = shellBands({ width: 1440, height: 900 });
    expect(empty.staging.w).toBe(empty.viewport.w);
  });

  it('yields the right-hand column once the stack is drawn', () => {
    const vp = { width: 1440, height: 900 };
    const empty = shellBands(vp);
    const live = shellBands(vp, {}, { stackPresent: true });
    expect(live.staging.w).toBeLessThan(empty.staging.w);
    // Exactly the cluster column plus its margins — the stack rail IS that
    // column and the cluster sits at its foot (control-language §4.4/D7).
    expect(empty.staging.w - live.staging.w).toBe(live.cluster.w + 2 * CONTROL.clusterMargin);
  });

  it('keeps the staging box clear of chrome whether or not the stack is drawn', () => {
    const vp = { width: 1280, height: 800 };
    for (const stackPresent of [false, true]) {
      const bands = shellBands(vp, {}, { stackPresent });
      expect(rectsOverlap(bands.hand, bands.staging), `hand, stack=${stackPresent}`).toBe(false);
      expect(rectsOverlap(bands.cluster, bands.staging), `cluster, stack=${stackPresent}`).toBe(
        false,
      );
    }
  });
});

describe('shell bands — the compact composition', () => {
  it('takes the compact composition only below the breakpoint', () => {
    expect(isCompactShell({ width: SHELL.compactBreakpoint - 1 })).toBe(true);
    expect(isCompactShell({ width: SHELL.compactBreakpoint })).toBe(false);
    expect(isCompactShell({ width: 390 })).toBe(true);
    expect(isCompactShell({ width: 1180 })).toBe(false);
  });

  it('stacks the hand above the cluster rather than beside it', () => {
    // A phone has no room for a 268px control column beside a seven-card fan,
    // so the compact composition gives the hand the full width and lifts it
    // clear of the cluster instead.
    const bands = shellBands({ width: 390, height: 844 });
    expect(bands.compact).toBe(true);
    expect(bands.hand.w).toBe(390);
    expect(bands.hand.x).toBe(0);
    expect(rectsOverlap(bands.hand, bands.cluster)).toBe(false);
    expect(bands.hand.y + bands.hand.h).toBeLessThanOrEqual(bands.cluster.y);
  });

  it('keeps the hand beside the cluster on the full composition', () => {
    const bands = shellBands({ width: 1440, height: 900 });
    expect(bands.compact).toBe(false);
    expect(rectsOverlap(bands.hand, bands.cluster)).toBe(false);
    // Side by side, sharing the bottom edge — not stacked.
    expect(bands.hand.x + bands.hand.w).toBeLessThanOrEqual(bands.cluster.x);
  });
});

describe('hand staging — invariant I2: the hand fits its band', () => {
  it('reserves the card, the largest lift, the floor, and headroom', () => {
    expect(handBandHeight()).toBe(
      SHELL.handFloor + SHELL.handLiftMax + SHELL.handCardH + SHELL.handHeadroom,
    );
    // The band must swallow the biggest transform any card takes — a transform
    // does not change layout, so anything larger is clipped by the band.
    const tallest = SHELL.handFloor + SHELL.handLiftMax + SHELL.handCardH;
    expect(handBandHeight()).toBeGreaterThanOrEqual(tallest);
    expect(SHELL.handLiftMax).toBeGreaterThanOrEqual(SHELL.handLift);
    expect(SHELL.handLiftMax).toBeGreaterThanOrEqual(SHELL.handLiftSelected);
  });

  it('rests every card at a positive offset from the band floor', () => {
    // The shipped rule was `bottom: -72px` inside an `overflow: hidden` band,
    // which cut roughly half of every hand card off the bottom of the screen.
    expect(SHELL.handFloor).toBeGreaterThan(0);
  });

  it('bounds the fan fractions to the usable span', () => {
    expect(handFanFraction(0, 7)).toBe(0);
    expect(handFanFraction(6, 7)).toBe(1);
    expect(handFanFraction(0, 1)).toBe(0.5);
    for (let n = 1; n <= 20; n += 1) {
      for (let i = 0; i < n; i += 1) {
        const t = handFanFraction(i, n);
        expect(t).toBeGreaterThanOrEqual(0);
        expect(t).toBeLessThanOrEqual(1);
      }
    }
  });

  for (const vp of SUPPORTED) {
    it(`keeps every card of a 7-card opening hand inside the band at ${vp.label}`, () => {
      const band = shellBands(vp).hand;
      for (let i = 0; i < 7; i += 1) {
        const { left, right } = handCardBounds(handFanFraction(i, 7), band.w);
        expect(left, `card ${i} left edge`).toBeGreaterThanOrEqual(0);
        expect(right, `card ${i} right edge`).toBeLessThanOrEqual(band.w);
      }
    });

    it(`exposes at least 44 px of every opening-hand card at ${vp.label}`, () => {
      expect(handFanSpacing(7, shellBands(vp).hand.w)).toBeGreaterThanOrEqual(SHELL.minHit);
    });
  }

  it('contains the fan at any band width, at any hand size', () => {
    for (const w of [200, 260, 390, 482, 882, 1250]) {
      for (const n of [1, 2, 7, 12, 20]) {
        for (let i = 0; i < n; i += 1) {
          const { left, right } = handCardBounds(handFanFraction(i, n), w);
          expect(left, `w=${w} n=${n} i=${i}`).toBeGreaterThanOrEqual(0);
          expect(right, `w=${w} n=${n} i=${i}`).toBeLessThanOrEqual(w);
        }
      }
    }
  });
});

describe('shell style variables', () => {
  it('publishes the geometry the stylesheet lays out from', () => {
    const vp = { width: 1440, height: 900 };
    const bands = shellBands(vp);
    const vars = shellStyleVars(vp) as Record<string, string>;
    // The stylesheet positions the two chrome regions absolutely from these;
    // it declares no dimension of its own, so what the tests reason about is
    // what ships.
    expect(vars['--shell-hand-w']).toBe(`${bands.hand.w}px`);
    expect(vars['--shell-cluster-w']).toBe(`${bands.cluster.w}px`);
    expect(vars['--shell-cluster-h']).toBe(`${bands.cluster.h}px`);
    expect(vars['--shell-bottom-chrome-h']).toBe(`${bottomChromeHeight()}px`);
    expect(vars['--shell-hand-h']).toBe(`${handBandHeight()}px`);
    expect(vars['--shell-hand-card-w']).toBe(`${SHELL.handCardW}px`);
    expect(vars['--shell-hand-floor']).toBe(`${SHELL.handFloor}px`);
  });

  it('narrows the published hand width on the full composition only', () => {
    // The hand yields the cluster's column when there is room to sit beside it,
    // and takes the full width when there is not.
    const full = shellStyleVars({ width: 1440, height: 900 }) as Record<string, string>;
    const compact = shellStyleVars({ width: 390, height: 844 }) as Record<string, string>;
    expect(full['--shell-hand-w']).not.toBe('1440px');
    expect(compact['--shell-hand-w']).toBe('390px');
  });
});

/** Read a stylesheet from the client source tree. */
function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

describe('stacking ladder — invariant I3', () => {
  const tokens = css('../../chrome/tokens.css');

  it('declares exactly the LAYER ladder in tokens.css', () => {
    const declared = new Map<string, number>();
    for (const [, name, value] of tokens.matchAll(/--rune-z-([a-z-]+):\s*(-?\d+);/g)) {
      declared.set(name!, Number(value));
    }
    const expected = new Map(
      Object.entries(LAYER).map(([name, value]) => [
        name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`),
        value as number,
      ]),
    );
    expect(Object.fromEntries(declared)).toEqual(Object.fromEntries(expected));
  });

  it('ranks a pending decision above every fixed shell region', () => {
    // The shipped shell had the decision sheet at 14 and the top bar at 20, so
    // the bar painted over a top-anchored mulligan prompt and swallowed clicks
    // aimed at it — a covered mulligan is a soft-lock.
    for (const region of [LAYER.scene, LAYER.shell, LAYER.shellRaised, LAYER.shellTop]) {
      expect(LAYER.decision).toBeGreaterThan(region);
    }
  });

  it('orders the ladder strictly, bottom to top', () => {
    const order: (keyof typeof LAYER)[] = [
      'scene',
      'shell',
      'shellRaised',
      'shellTop',
      'decision',
      'drag',
      'popover',
      'overlay',
      'toast',
    ];
    const values = order.map((name) => LAYER[name]);
    expect(values).toEqual([...values].sort((a, b) => a - b));
    expect(new Set(values).size).toBe(values.length);
  });

  it('keeps the tokens.css shell fallbacks in step with SHELL', () => {
    expect(tokens).toContain(`--shell-hand-card-w: ${SHELL.handCardW}px;`);
    expect(tokens).toContain(`--shell-hand-gutter: ${SHELL.handGutter}px;`);
  });
});

describe('stylesheet contract', () => {
  const shell = css('./live-match.module.css');
  const chrome = css('../chrome.module.css');

  /**
   * A bare `z-index` is allowed only for ordering siblings INSIDE one region
   * (a lifted hand card, a sheet's close button). Anything at or above the
   * `shell` rung must read a `--rune-z-*` token, or it is competing with the
   * shell regions outside the one place the ladder is decided.
   */
  it('uses no bare shell-level z-index in the match stylesheets', () => {
    for (const [name, sheet] of [
      ['live-match.module.css', shell],
      ['chrome.module.css', chrome],
    ] as const) {
      for (const [, value] of sheet.matchAll(/z-index:\s*(-?\d+);/g)) {
        expect(Math.abs(Number(value)), `${name} bare z-index ${value}`).toBeLessThan(LAYER.shell);
      }
    }
  });

  it('never positions a hand card with a negative offset', () => {
    const handCard = /\.handCard \{([^}]*)\}/.exec(shell)?.[1] ?? '';
    expect(handCard).toContain('bottom: var(--shell-hand-floor);');
    expect(/bottom:\s*-/.test(shell.replace(/\/\*[\s\S]*?\*\//g, ''))).toBe(false);
  });

  it('insets the fan by half a card instead of a raw percentage span', () => {
    expect(shell).toContain(
      'left: calc(var(--hand-inset) + (100% - 2 * var(--hand-inset)) * var(--hand-t));',
    );
    expect(shell).toContain(
      '--hand-inset: calc(var(--shell-hand-card-w) / 2 + var(--shell-hand-gutter));',
    );
  });

  it('sizes the shell from the dynamic viewport and the browser safe area', () => {
    expect(shell).toContain('height: 100dvh;');
    expect(shell).toContain('env(safe-area-inset-top, 0px)');
    expect(shell).toContain('env(safe-area-inset-bottom, 0px)');
    // `100vw` includes a classic desktop scrollbar and overflows the shell.
    expect(shell).not.toContain('width: 100vw;');
  });

  it('carves no permanent track for retired chrome', () => {
    // ADR 0032's removal, asserted at the stylesheet rather than by eye. If any
    // of these comes back as a grid track, the battlefield stops owning the
    // viewport and #534's first acceptance criterion silently regresses.
    for (const retired of [
      '--shell-top-h',
      '--shell-rail-w',
      '--shell-bottom-h',
      '--shell-identity-w',
      '--shell-decisions-min-w',
      '--shell-controls-h',
    ]) {
      expect(shell, `${retired} is still laid out`).not.toContain(retired);
    }
    expect(shell).not.toContain('grid-template-rows');
  });

  /**
   * The scene's own box, which #534 dropped when it replaced the grid shell.
   * `.scene` survived only inside two `[data-moment]` descendant selectors, so
   * the class still resolved and the section still mounted — as a static block
   * with no layout. Its sole child is `LivePlane`'s host, which states no height
   * and clips to `overflow: hidden`, so the whole battlefield collapsed to zero
   * height: no environment, no seats, no cards, no controls, no effects, with a
   * live hand and a live cluster still floating over the black.
   *
   * jsdom computes no layout, so nothing that renders the tree can catch this.
   * The stylesheet is where it is provable, which is why it is asserted here.
   */
  it('gives the scene a box that spans the safe viewport', () => {
    const rule = (selector: string): string =>
      /\{([^}]*)\}/.exec(shell.slice(shell.indexOf(`${selector} {`)))?.[1] ?? '';
    const scene = rule('.scene');
    expect(scene, '.scene has no base rule — the battlefield has no box').toContain(
      'position: absolute;',
    );
    // An absolute child is laid out against the padding box, which INCLUDES
    // `.shell`'s safe-area padding, so the insets have to be repeated here.
    expect(scene).toContain('env(safe-area-inset-top, 0px)');
    expect(scene).toContain('env(safe-area-inset-bottom, 0px)');
    // The bottom rung: the arena paints under both floating chrome regions.
    expect(scene).toContain('z-index: var(--rune-z-scene);');
    // …and the host inside it is sized by the section rather than by itself.
    const child = rule('.scene > *');
    expect(child, '.scene > * is gone — LivePlane’s host has no height').toContain('height: 100%;');
    expect(child).toContain('width: 100%;');
  });

  it('sizes the two chrome regions from the published rects', () => {
    const rule = (selector: string): string =>
      /\{([^}]*)\}/.exec(shell.slice(shell.indexOf(`${selector} {`)))?.[1] ?? '';
    expect(rule('.hand')).toContain('width: var(--shell-hand-w);');
    expect(rule('.hand')).toContain('height: var(--shell-hand-h);');
    expect(rule('.cluster')).toContain('width: var(--shell-cluster-w);');
    expect(rule('.cluster')).toContain('var(--rune-cluster-margin)');
  });

  it('lifts the compact hand clear of the cluster instead of sharing a cell', () => {
    // The shipped compact grid let an opaque decisions panel span both rows and
    // paint over most of a seven-card fan at 390x844 — the cards the mulligan
    // asked about were covered by the prompt asking about them.
    const compactBlock = shell.slice(shell.indexOf('@media (max-width: 899px)'));
    expect(compactBlock).toContain('var(--shell-cluster-h)');
    expect(compactBlock).not.toContain('.decisions');
  });

  it('reserves the safe area in the decision layer', () => {
    // #567 replaced the full-viewport decision sheet with the decision area at
    // the head of the lower-right action column, so the containment rules moved
    // with it: the offsets are composed from the shell's own published rects
    // plus the browser's insets, and it still declares the `decision` rung.
    const area = css('../decision/decision.module.css');
    const block = area.slice(area.indexOf('.area {'), area.indexOf('.frame {'));
    expect(block).toContain('z-index: var(--rune-z-decision);');
    expect(block).toContain('env(safe-area-inset-right, 0px)');
    expect(block).toContain('env(safe-area-inset-bottom, 0px)');
    // It stands ON the cluster's own published height, so growing the cluster
    // moves the decision rather than letting the two overlap.
    expect(block).toContain('var(--shell-cluster-h)');
    // The region blocks nothing outside its own plate: the cards a mulligan is
    // asking about stay clickable (#451), and nothing dims them either.
    expect(block).toContain('pointer-events: none;');

    // The retired sheet's classes are gone from the shell's stylesheet, so
    // there is no second decision layer left to drift.
    expect(chrome).not.toContain('.sheetBackdrop');
    expect(chrome).not.toContain('.promptStrip');
  });

  it('clears the whole hand band on the compact composition', () => {
    // On compact the hand spans the full width ABOVE the cluster, so an area
    // offset only by the cluster would sit on the very cards a mulligan is
    // asking the player to bottom — #528's defect, one composition over.
    const area = css('../decision/decision.module.css');
    const compact = area.slice(area.indexOf('@media (max-width: 899px)'));
    expect(compact).toContain('var(--shell-hand-h)');
    expect(compact).toContain('var(--shell-cluster-h)');
    // The same breakpoint the shell stylesheet and `isCompactShell` use.
    expect(compact).toContain(`max-width: ${SHELL.compactBreakpoint - 1}px`);
  });
});
