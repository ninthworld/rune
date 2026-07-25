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
import {
  LAYER,
  SHELL,
  bottomShellHeight,
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

/** Every named region of a composition, as (name, rect) pairs. */
function regionsOf(viewport: { width: number; height: number }): [string, ShellRect][] {
  const bands = shellBands(viewport);
  const named: [string, ShellRect][] = [
    ['top', bands.top],
    ['scene', bands.scene],
    ['identity', bands.identity],
    ['hand', bands.hand],
    ['decisions', bands.decisions],
  ];
  if (bands.rail) named.push(['rail', bands.rail]);
  return named;
}

describe('shell bands — invariant I1: containment and non-overlap', () => {
  for (const vp of SUPPORTED) {
    it(`keeps every region inside the viewport at ${vp.label}`, () => {
      const bands = shellBands(vp);
      for (const [name, rect] of regionsOf(vp)) {
        expect(rect.w, `${name} width`).toBeGreaterThan(0);
        expect(rect.h, `${name} height`).toBeGreaterThan(0);
        expect(rectContains(bands.viewport, rect), `${name} inside viewport`).toBe(true);
      }
    });

    it(`never overlaps two fixed regions at ${vp.label}`, () => {
      const named = regionsOf(vp);
      for (let i = 0; i < named.length; i += 1) {
        for (let j = i + 1; j < named.length; j += 1) {
          const [aName, a] = named[i]!;
          const [bName, b] = named[j]!;
          expect(rectsOverlap(a, b), `${aName} overlaps ${bName}`).toBe(false);
        }
      }
    });

    it(`leaves the battlefield at or above the plane's staging floor at ${vp.label}`, () => {
      // `LivePlane` clamps its staged height to `sceneMinH`; a scene band shorter
      // than that stages a plane taller than the box that clips it, cutting off
      // the receiver's own band.
      expect(shellBands(vp).scene.h).toBeGreaterThanOrEqual(SHELL.sceneMinH);
    });
  }

  it('insets the whole shell by the browser safe area, not just the edges', () => {
    const inset = { top: 47, bottom: 34, left: 0, right: 0 };
    const bands = shellBands({ width: 390, height: 844 }, inset);
    expect(bands.viewport).toEqual({ x: 0, y: 47, w: 390, h: 763 });
    for (const [name, rect] of [
      ['top', bands.top],
      ['scene', bands.scene],
      ['hand', bands.hand],
      ['decisions', bands.decisions],
    ] as [string, ShellRect][]) {
      expect(rectContains(bands.viewport, rect), `${name} inside safe viewport`).toBe(true);
    }
    // The hand band's bottom clears the home indicator.
    expect(bands.hand.y + bands.hand.h).toBeLessThanOrEqual(844 - inset.bottom);
  });
});

describe('shell bands — the compact composition', () => {
  it('takes the compact composition only below the breakpoint', () => {
    expect(isCompactShell({ width: SHELL.compactBreakpoint - 1 })).toBe(true);
    expect(isCompactShell({ width: SHELL.compactBreakpoint })).toBe(false);
    expect(isCompactShell({ width: 390 })).toBe(true);
    expect(isCompactShell({ width: 1180 })).toBe(false);
  });

  it('gives the hand its own full-width row, clear of the decisions column', () => {
    // The shipped compact grid spanned the hand under an opaque decisions panel
    // that covered most of a seven-card fan at 390×844.
    const bands = shellBands({ width: 390, height: 844 });
    expect(bands.compact).toBe(true);
    expect(bands.hand.w).toBe(390);
    expect(bands.hand.x).toBe(0);
    expect(rectsOverlap(bands.hand, bands.decisions)).toBe(false);
    expect(rectsOverlap(bands.hand, bands.identity)).toBe(false);
    // The hand row sits below the controls row, not behind it.
    expect(bands.hand.y).toBeGreaterThanOrEqual(bands.decisions.y + bands.decisions.h);
  });

  it('drops the rail on the compact composition and keeps it otherwise', () => {
    expect(shellBands({ width: 390, height: 844 }).rail).toBeUndefined();
    expect(shellBands({ width: 1280, height: 800 }).rail?.w).toBe(SHELL.railW);
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
    const full = shellStyleVars({ width: 1280 }) as Record<string, string>;
    expect(full['--shell-top-h']).toBe(`${SHELL.topH}px`);
    expect(full['--shell-bottom-h']).toBe(`${bottomShellHeight(false)}px`);
    expect(full['--shell-hand-h']).toBe(`${handBandHeight()}px`);
    expect(full['--shell-hand-card-w']).toBe(`${SHELL.handCardW}px`);
    expect(full['--shell-hand-floor']).toBe(`${SHELL.handFloor}px`);
    expect(full['--shell-rail-w']).toBe(`${SHELL.railW}px`);

    const compact = shellStyleVars({ width: 390 }) as Record<string, string>;
    expect(compact['--shell-top-h']).toBe(`${SHELL.topHCompact}px`);
    expect(compact['--shell-bottom-h']).toBe(`${bottomShellHeight(true)}px`);
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
    expect(tokens).toContain(`--shell-top-h: ${SHELL.topH}px;`);
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
    // `100vw` includes a classic desktop scrollbar and pushes the rail off-screen.
    expect(shell).not.toContain('width: 100vw;');
  });

  it('declares no dimensional literal in the shell grid', () => {
    const grid = shell.slice(shell.indexOf('.shell {'), shell.indexOf('.top {'));
    expect(grid).toContain('grid-template-rows: var(--shell-top-h)');
    expect(grid).toContain('var(--shell-bottom-h);');
    expect(grid).toContain('grid-template-columns: minmax(0, 1fr) var(--shell-rail-w);');
  });

  it('gives the compact hand its own row rather than a shared cell', () => {
    const compactBlock = shell.slice(shell.indexOf('@media (max-width: 899px)'));
    const rule = (selector: string): string =>
      /\{([^}]*)\}/.exec(compactBlock.slice(compactBlock.indexOf(`${selector} {`)))?.[1] ?? '';
    expect(rule('  .hand')).toContain('grid-row: 2;');
    expect(rule('  .decisions')).toContain('grid-row: 1;');
    // The shipped rule was `grid-row: 1 / 3` plus a z-index, which let the
    // opaque decisions panel paint over the fan.
    expect(rule('  .decisions')).not.toContain('grid-row: 1 / 3;');
    expect(rule('  .decisions')).not.toContain('z-index');
  });

  it('reserves the top-bar band and the safe area in the decision sheet layer', () => {
    const backdrop = chrome.slice(
      chrome.indexOf('.sheetBackdrop {'),
      chrome.indexOf('.sheetPanel {'),
    );
    expect(backdrop).toContain('calc(var(--shell-top-h) + 16px + env(safe-area-inset-top, 0px))');
    expect(backdrop).toContain('z-index: var(--rune-z-decision);');
    // The pass-through variant blocks nothing, so it must not dim the hand it
    // is asking the player to read.
    expect(backdrop).toContain('background: transparent;');
  });
});
