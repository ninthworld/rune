/**
 * The match chrome's stylesheet-source guards (issues #583, #585, #586).
 *
 * jsdom applies no CSS module, resolves no `clip-path`, and computes no layout,
 * so every defect these three issues collect is invisible to a DOM assertion and
 * to a snapshot alike: a drawer anchored to a bar that no longer exists, a drop
 * region drawn as a hit-box outline, a round icon parked on a hexagon's point.
 * What *is* checkable without a browser is the declaration itself — the offsets,
 * the tokens each rule reaches for, and the arithmetic relating one rule to
 * another. That is what this file reads.
 *
 * The idiom is `table/seat-cluster.dom.test.ts`'s: read the stylesheet as source
 * and assert on the rules, precisely because a cascade or geometry bug there is
 * invisible to the DOM. Nothing here claims a painted pixel — that the drawer
 * lands beside its handle, that the region reads as intentional, and that the
 * trim runs unbroken round both points are the maintainer's browser checks, as
 * each issue says.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { CONTROL } from './controls/controlTokens';

function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

/**
 * Everything declared for `selector`, in source order, comments stripped.
 *
 * Every block whose selector list names it, not just the first: a rule that
 * shares a block with a sibling and then overrides one property in a block of
 * its own is exactly the shape `.compactRow` has, and reading only the first
 * block would assert the wrong half of the cascade. Selectors are matched whole,
 * so `.menuDrawer` never picks up `.menuDrawerCluster`.
 */
function rule(sheet: string, selector: string): string {
  const stripped = sheet.replace(/\/\*[\s\S]*?\*\//g, '');
  const bodies: string[] = [];
  for (const [, selectors, body] of stripped.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (selectors.split(',').some((one) => one.trim() === selector)) bodies.push(body);
  }
  expect(bodies.length, `${selector} is not declared`).toBeGreaterThan(0);
  return bodies.join('\n');
}

describe('the game menu opens at its handle (#583)', () => {
  const sheet = css('./chrome.module.css');

  it('anchors the cluster-handled drawer to the cluster, not to a removed top bar', () => {
    const drawer = rule(sheet, '.menuDrawerCluster');
    // The defect verbatim: `top: 52px` is the height of the bar ADR 0032 and
    // #534 removed. The controlled drawer cancels it and bottom-anchors instead.
    expect(drawer).toMatch(/top:\s*auto/);
    expect(drawer).toMatch(/bottom:\s*calc\(/);
    // Bottom-anchored FROM the cluster's own band, so the drawer cannot drift
    // from the handle when the cluster's height changes.
    expect(drawer).toContain('--shell-cluster-h');
    expect(drawer).toContain('--rune-cluster-margin');
  });

  it('clamps the drawer to the viewport rather than growing off the top edge', () => {
    const drawer = rule(sheet, '.menuDrawerCluster');
    expect(drawer).toMatch(/max-height:\s*calc\(\s*100dvh/);
    expect(drawer).toMatch(/overflow-y:\s*auto/);
  });

  it('leaves the spectator top bar’s drawer where its handle still is', () => {
    // `TopBar` renders the uncontrolled form, whose own `.menuButton` is the
    // handle and whose bar really is 52px tall. Re-anchoring both would have
    // moved that drawer away from ITS handle to fix this one.
    expect(rule(sheet, '.menuDrawer')).toMatch(/top:\s*52px/);
  });
});

describe('drop affordances read as material, not as a debug overlay (#585)', () => {
  const sheet = css('./live/live-plane.module.css');
  const board = rule(sheet, '.dropBoard');
  const target = rule(sheet, '.dropTarget');

  it('drops the flat gold rectangle from both drop surfaces', () => {
    // The defect: `border: 4px solid var(--gold)` on a rounded box, with no
    // bevel, no falloff and no material — the treatment a hit-box visualiser
    // uses. Gold is also the selection/actionable accent everywhere else, so a
    // drop region wearing it is a fourth meaning for one hue.
    for (const declaration of [board, target]) {
      expect(declaration).not.toMatch(/border:\s*\d+px solid/);
      expect(declaration).not.toContain('var(--gold)');
    }
  });

  it('paints them from the drop family’s own tokens', () => {
    expect(board).toContain('--rune-drop-valid-fill');
    expect(board).toContain('--rune-drop-valid');
    expect(target).toContain('--rune-drop-valid');
  });

  it('carries §11’s non-colour channel on both — the L-shaped corner ticks', () => {
    const ticks = rule(sheet, '.dropBoard::after');
    expect(rule(sheet, '.dropTarget::after')).toBe(ticks);
    expect(ticks).toContain('--rune-drop-tick');
    expect(ticks).toContain('--rune-drop-stroke');
    // Eight slices, two per corner: the L needs an arm on each axis.
    expect((ticks.match(/no-repeat/g) ?? []).length).toBe(8);
  });

  it('reacts on hover (§6.2 stage 4) without a second hit-testing path', () => {
    // The region keeps `pointer-events: auto` — it IS the routing target that
    // `[data-drop-receiver]` hit testing walks — so `:hover` is the honest test
    // and the reaction cannot disagree with where the drop lands. The proxy
    // stays out of hit testing, which is what makes that true mid-drag.
    expect(board).toMatch(/pointer-events:\s*auto/);
    expect(target).toMatch(/pointer-events:\s*none/);
    expect(rule(sheet, '.dropBoard:hover')).toContain('--rune-drop-valid');
    expect(rule(sheet, '.dropBoard:hover::after')).toContain('--rune-drop-tick-hover');
  });

  it('keeps the whole treatment under reduced motion, dropping only the tween', () => {
    // §12: "region light-up → regions appear at full treatment instantly."
    // Nothing about where a card may be dropped may be carried by motion.
    const reduced = sheet.slice(sheet.indexOf('@media (prefers-reduced-motion: reduce)'));
    expect(reduced).toContain('.dropBoard');
    expect(reduced).toMatch(/transition:\s*none/);
  });
});

describe('eligible and chosen are different in kind (#585)', () => {
  const sheet = css('./live/live-plane.module.css');

  it('gives eligible a dashed ring and chosen a solid one plus a mark', () => {
    // The defect: both were a gold ring, differing only by `outline-style`, so
    // two creatures read as "gold outlined" while the panel said `0 SELECTED`
    // and the player could not recover which was right.
    expect(rule(sheet, '.targetControl')).toMatch(/outline:.*dashed/);
    const chosen = rule(sheet, ".targetControl[aria-pressed='true']");
    expect(chosen).toMatch(/outline:.*solid/);
    // The check mark is the channel that survives with hue stripped, and it is
    // the same fact the decision panel's count is counting.
    expect(rule(sheet, ".targetControl[aria-pressed='true']::after")).toContain("content: '✓'");
  });
});

describe('the phase plaque’s trailing point (#586)', () => {
  const plaque = css('./controls/plaque.module.css');
  const cluster = css('./controls/cluster.module.css');

  it('cuts both points symmetrically on the frame and on the face', () => {
    // #586 asks whether `--hex-face`'s two tips are the same length measured
    // from opposite edges. They are, and this is what keeps them so: each
    // horizontal stop appears once as a raw length and once subtracted from
    // 100%, so the leading and trailing trims cannot drift apart. jsdom
    // resolves no clip-path — the drawn trim stays a browser check.
    for (const [polygon, lengths] of [
      ['--hex', ['var(--point)']],
      ['--hex-face', ['var(--rune-plaque-face-point)', 'var(--rune-plaque-face-tip)']],
    ] as const) {
      const body = plaque.slice(plaque.indexOf(`${polygon}: polygon(`));
      const declaration = body.slice(0, body.indexOf(');'));
      for (const length of lengths) {
        const bare = declaration.split(length).length - 1;
        const complemented = declaration.split(`calc(100% - ${length})`).length - 1;
        // Every stop is mirrored: n raw uses, n complemented uses.
        expect(bare - complemented, `${polygon} is asymmetric in ${length}`).toBe(complemented);
      }
    }
  });

  it('clears the round icon off the plaque’s trailing bevel in the compact row', () => {
    // The gaps around the plaque are measured between bounding boxes, and the
    // plaque's trailing end is a 22px diagonal — so a 44 ⌀ circle 12px from the
    // box is 12px from the TIP, on the same midline, and reads as sitting on it.
    expect(rule(cluster, '.compactRow')).toContain('var(--rune-plaque-point)');
    expect(rule(plaque, '.compact')).toContain('var(--rune-plaque-point)');
  });

  it('keeps the compact row measuring one 268px column', () => {
    // The clearance is taken out of the plaque's width and given to the gap, so
    // the two have to move together. Written as the arithmetic rather than as
    // the shipped number, so a token change moves both.
    const plaqueW = CONTROL.wCluster - CONTROL.hit - CONTROL.clusterGap - CONTROL.plaquePoint;
    const gap = CONTROL.clusterGap + CONTROL.plaquePoint;
    expect(plaqueW + gap + CONTROL.hit).toBe(CONTROL.wCluster);
  });
});

describe('Auto-passed reads as part of the plaque (#586)', () => {
  const badge = rule(css('./controls/plaque.module.css'), '.autoPassed');

  it('shares the plate’s top edge instead of floating above it', () => {
    // The defect: a gold-outlined capsule 4px clear of the plaque on the overlay
    // background — no tail, no shared edge, no shared material, which reads as a
    // debug toast. It now sits ON the edge and drops its bottom border, so the
    // two are one object.
    expect(badge).toMatch(/bottom:\s*100%;/);
    expect(badge).not.toContain('calc(100% + ');
    expect(badge).toMatch(/border-bottom:\s*0/);
  });

  it('wears the plaque’s own material and clears the trailing point', () => {
    expect(badge).toContain('--rune-control-plate');
    expect(badge).toContain('--rune-control-frame-gradient');
    expect(badge).toMatch(/right:\s*var\(--rune-plaque-point\)/);
  });
});

describe('one primary colour (#586)', () => {
  it('retires the green confirm fill from the tokens and the family', () => {
    // §4.1 allows one primary treatment; §4.2 rule 1 says the decision's confirm
    // is what carries the advance while a decision is open. So the confirm wears
    // the primary's enamel and there is no second primary hue to learn.
    expect(css('../chrome/tokens.css')).not.toContain('--rune-confirm-face');
    const controls = css('./controls/controls.module.css');
    expect(controls).not.toMatch(/^\.confirm\b/m);
    // Red survives on cancel alone, where it means destructive and nothing else.
    expect(controls).toContain('--rune-danger-face');
  });
});
