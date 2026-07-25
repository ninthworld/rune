/**
 * The seat identity cluster as the reconciler mounts it (issue #532).
 *
 * `seat-cluster.test.ts` proves the staged geometry; this file proves the DOM
 * that geometry becomes: one element per cluster part, the state channels as
 * data attributes a stylesheet can select on, the portrait URL and the seat
 * accent as custom properties (neither can be an `attr()`), fresh-mount
 * equivalence after a reconcile, and the scene-node cost at six seats.
 *
 * jsdom applies no CSS module and performs no layout, so nothing here asserts a
 * painted ring, a clip-path, or a contrast reading — only that the inputs the
 * stylesheet needs are present and correct.
 *
 * That blind spot is why the last suite reads `live-plane-cluster.module.css`
 * as **source**. A cascade bug — a state rule respecifying `background-image`
 * and silently dropping the §1.2 el. 1 rim underneath it — is invisible to a
 * DOM assertion and to a snapshot, because jsdom resolves neither. The
 * stylesheet-source idiom is the same one `card/dom/CardArt.test.tsx` and
 * `card/back/cardBack.test.tsx` use.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { stagePlane, type StagedPlane } from './plane';
import { PlaneReconciler, type PlaneFaceRenderer } from './planeReconciler';
import { SCENE_DOM_CEILING } from './live/presentationMode';
import { clusterTable } from './seat-cluster.fixture';
import { DESKTOP } from './plane.fixture';

const face: PlaneFaceRenderer = {
  signature: (render) => `${render.entityId}:${render.tier}`,
  render: (el, render) => {
    el.textContent = render.name;
  },
};

function seats(n: number, overrides: Record<number, Record<string, unknown>> = {}) {
  return Array.from({ length: n }, (_, i) => ({
    id: `p${i + 1}`,
    name: `Player ${i + 1}`,
    ...(overrides[i] ?? {}),
  }));
}

function mount(plane: StagedPlane): PlaneReconciler {
  const reconciler = new PlaneReconciler(document.createElement('div'), { face });
  reconciler.reconcile(plane);
  return reconciler;
}

function slot(r: PlaneReconciler, kind: string, seat: string): HTMLElement | null {
  return r.root.querySelector(`[data-slot="${kind}"][data-seat="${seat}"]`);
}

describe('seat cluster DOM — one element per part of the cluster', () => {
  it('mounts the medallion, nameplate, life medallion, and hand pip for every seat', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), {
      width: 1680,
      height: 945,
    });
    const r = mount(plane);
    for (const kind of ['crest', 'plate', 'life', 'pip']) {
      expect(r.root.querySelectorAll(`[data-slot="${kind}"]`)).toHaveLength(4);
    }
    // No library pip, at any rung: the library count is the library pile's
    // (`zone-geography.md` §4.1).
    expect(r.root.querySelectorAll('[data-slot="librarypip"]')).toHaveLength(0);
  });

  it('carries the life numeral, the hand count, and the glyph-count size step', () => {
    const plane = stagePlane(
      clusterTable({ seats: seats(2, { 0: { life: 999, hand: 20 } }) }),
      DESKTOP,
    );
    const r = mount(plane);
    expect(slot(r, 'life', 'p1')?.dataset.glyph).toBe('999');
    expect(slot(r, 'life', 'p1')?.dataset.glyphs).toBe('3');
    expect(slot(r, 'pip', 'p1')?.dataset.count).toBe('20');
    // The crest keeps the never-degrading readout it has always carried.
    expect(slot(r, 'crest', 'p1')?.dataset.life).toBe('999');
    expect(slot(r, 'crest', 'p1')?.dataset.hand).toBe('20');
  });

  it('publishes every state channel as its own attribute', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { command: ['{G}'] } }),
        active: 'p2',
        priority: 'p2',
        attacks: [['p3', 'p2']],
      }),
      { width: 1680, height: 945 },
    );
    const r = mount(plane);
    const crest = slot(r, 'crest', 'p2')!;
    expect(crest.dataset).toMatchObject({
      priority: 'true',
      active: 'true',
      attacked: 'true',
      focused: 'true',
      commander: 'true',
      eliminated: 'false',
      // Dormant until a per-seat connection field lands (§11, issue #553).
      disconnected: 'false',
    });
    const quiet = slot(r, 'crest', 'p4')!;
    expect(quiet.dataset).toMatchObject({ priority: 'false', active: 'false', attacked: 'false' });
  });

  it('swaps the life numeral for the struck rune on an eliminated seat', () => {
    const plane = stagePlane(
      clusterTable({ seats: seats(2, { 1: { eliminated: true } }) }),
      DESKTOP,
    );
    const r = mount(plane);
    expect(slot(r, 'life', 'p2')?.dataset.glyph).toBe('⊘');
    expect(slot(r, 'life', 'p2')?.dataset.eliminated).toBe('true');
    expect(slot(r, 'plate', 'p2')?.dataset.eliminated).toBe('true');
    // Count pips and the status rail are removed; the seat keeps its slot.
    expect(slot(r, 'pip', 'p2')).toBeNull();
    expect(r.root.querySelector('[data-slot="chip"][data-seat="p2"]')).toBeNull();
    expect(slot(r, 'crest', 'p2')).not.toBeNull();
  });

  it('carries the portrait URL and the seat accent as custom properties', () => {
    // `attr()` cannot produce a `url()`, and the accent is a token value rather
    // than an enum, so both ride as custom properties — which keeps the
    // stylesheet free of both hashes and hex.
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const r = mount(plane);
    const crest = slot(r, 'crest', 'p1')!;
    expect(crest.style.getPropertyValue('--portrait-src')).toMatch(
      /^url\("\/assets\/portraits\/.+\.webp"\)$/,
    );
    expect(crest.style.getPropertyValue('--seat-accent')).toMatch(/^#[0-9A-Fa-f]{6}$/);
    expect(crest.style.getPropertyValue('--cluster-d')).toMatch(/^\d+px$/);
    expect(crest.dataset.portrait).toBe('true');
    // §1.3: no substitute glyph. The crest publishes nothing a stylesheet could
    // paint a monogram, rune, or initial from — the aperture's token background
    // is the whole fallback, and the seat's name lives in the crest control's
    // accessible name instead.
    expect(crest.dataset.monogram).toBeUndefined();
    expect(crest.getAttribute('data-monogram')).toBeNull();
    expect(crest.textContent).toBe('');
  });

  it('lays the status rail out as one element per drawn chip', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { statuses: ['monarch', 'initiative', 'ring-tempts'] } }),
        active: 'p2',
        attacks: [['p3', 'p2']],
      }),
      { width: 1680, height: 945 },
    );
    const r = mount(plane);
    const chips = [...r.root.querySelectorAll('[data-slot="chip"][data-seat="p2"]')];
    expect(chips.map((c) => (c as HTMLElement).dataset.kind)).toEqual([
      'attacked',
      'status',
      'overflow',
    ]);
    expect((chips[0] as HTMLElement).dataset.value).toBe('×1');
    expect((chips[2] as HTMLElement).dataset.value).toBe('2');
  });

  it('draws the identity gem only when the command zone names the seat', () => {
    const withCommander = stagePlane(
      clusterTable({ seats: seats(2, { 0: { command: ['{2}{B}'] } }) }),
      DESKTOP,
    );
    expect(slot(mount(withCommander), 'gem', 'p1')?.dataset.identity).toBe('B');
    const without = stagePlane(clusterTable({ seats: seats(2) }), DESKTOP);
    expect(slot(mount(without), 'gem', 'p1')).toBeNull();
  });

  it('gives a compact tile the seat’s portrait and accent, at no extra node', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), {
      width: 390,
      height: 844,
    });
    expect(plane.tiles.length).toBeGreaterThan(0);
    const r = mount(plane);
    const tile = r.root.querySelector(
      `[data-slot="tile"][data-seat="${plane.tiles[0]!.seat}"]`,
    ) as HTMLElement;
    expect(tile.style.getPropertyValue('--portrait-src')).toMatch(/^url\(".+"\)$/);
    expect(tile.style.getPropertyValue('--seat-accent')).toMatch(/^#[0-9A-Fa-f]{6}$/);
  });
});

describe('seat cluster DOM — reconcile hygiene and budget', () => {
  it('updates a changed life in place and touches nothing else', () => {
    const before = stagePlane(clusterTable({ seats: seats(2, { 0: { life: 20 } }) }), DESKTOP);
    const after = stagePlane(clusterTable({ seats: seats(2, { 0: { life: 17 } }) }), DESKTOP);
    const r = mount(before);
    const el = slot(r, 'life', 'p1')!;
    r.reconcile(after);
    expect(slot(r, 'life', 'p1')).toBe(el);
    expect(el.dataset.glyph).toBe('17');
  });

  it('does zero work when the same plane is applied twice', () => {
    const plane = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const r = mount(plane);
    r.reconcile(stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP));
    expect(r.lastStats.chrome).toBe(0);
  });

  it('is byte-identical to a fresh mount after a reconcile (fresh-mount equivalence)', () => {
    const first = stagePlane(clusterTable({ seats: seats(4), active: 'p2' }), DESKTOP);
    const second = stagePlane(
      clusterTable({
        seats: seats(4, { 1: { life: 12, statuses: ['monarch'] } }),
        active: 'p3',
        priority: 'p3',
        attacks: [['p2', 'p1']],
      }),
      DESKTOP,
    );
    const incremental = mount(first);
    incremental.reconcile(second);
    expect(incremental.root.innerHTML).toBe(mount(second).root.innerHTML);
  });

  it('stays far inside the scene-node ceiling at six seats', () => {
    const plane = stagePlane(
      clusterTable({
        seats: seats(6, {
          1: { statuses: ['monarch', 'initiative'], command: ['{W}{U}'] },
          2: { statuses: ['x'] },
        }),
        active: 'p2',
        priority: 'p2',
        attacks: [
          ['p2', 'p1'],
          ['p3', 'p4'],
        ],
        commanderDamage: [['p2', 'p1', 18]],
        commanderTax: [['p2', 4]],
      }),
      { width: 1920, height: 1080 },
    );
    const r = mount(plane);
    const clusterNodes = r.root.querySelectorAll(
      '[data-slot="crest"], [data-slot="plate"], [data-slot="life"], [data-slot="pip"], [data-slot="gem"], [data-slot="chip"]',
    ).length;
    // Six seats, every state lit: the whole identity layer is a couple of dozen
    // elements against a 15 000-node scene ceiling.
    expect(clusterNodes).toBeLessThanOrEqual(6 * 10);
    expect(r.root.querySelectorAll('*').length).toBeLessThan(SCENE_DOM_CEILING / 10);
  });
});

describe('the portrait medallion at the stylesheet source (§1.2 el. 1, §1.3, §4)', () => {
  const CSS = readFileSync(
    resolve(process.cwd(), 'src/table/live/live-plane-cluster.module.css'),
    'utf8',
  );

  /** The stylesheet with its commentary stripped — declarations only. */
  const DECLS = CSS.replace(/\/\*[\s\S]*?\*\//g, '');

  /** Every rule in the file whose selector touches the crest, selector and body. */
  const CREST_RULES = [...DECLS.matchAll(/^[ \t]*(\.plane [^{}/*]*?)\s*\{([^}]*)\}/gm)]
    .map((match) => ({ selector: match[1]!.trim(), body: match[2]! }))
    .filter((rule) => rule.selector.includes("[data-slot='crest']"));

  const bodyOf = (selector: string): string =>
    CREST_RULES.find((rule) => rule.selector === selector)?.body ?? '';

  it('draws the layered rim as ONE gradient on the base rule', () => {
    // §1.2 el. 1: a `0.012` gold hairline, a `0.041` brushed band, and a `0.012`
    // inner hairline — three stops of one gradient rather than three nested
    // boxes, so the medallion costs the scene one node. Under it, the portrait
    // plate clipped to the aperture, and under THAT the token aperture fill.
    const base = bodyOf(".plane [data-slot='crest']");
    expect(base).not.toBe('');
    expect(base).toMatch(/^\s*background\s*:/m);
    expect(base).toContain('circle at 50% 34%');
    // Three gold stops: hairline, brushed band, inner hairline.
    expect((base.match(/var\(--gold\)/g) ?? []).length).toBeGreaterThanOrEqual(3);
    // The rim is a `border-box` layer; the art and the token fill are
    // `content-box`, which is what clips the plate to the aperture, not the rim.
    expect(base).toContain('border-box');
    expect(base).toContain('var(--portrait-src, none)');
    expect(base).toContain('var(--raised), var(--surface-base)');
    expect(base).toContain('content-box');
  });

  it('never lets a crest STATE rule respecify the medallion background', () => {
    // The finding this suite exists for. `background` and `background-image`
    // replace the entire layer stack, so any state rule that sets one on the
    // crest itself silently deletes the rim above — and `data-commander` is
    // every seat in a Commander game. States paint through box-shadow, outline,
    // border, filter, custom properties, or their own pseudo-element instead.
    const stateRules = CREST_RULES.filter(
      (rule) =>
        rule.selector !== ".plane [data-slot='crest']" &&
        !rule.selector.includes('::') &&
        /\[data-(?!slot)/.test(rule.selector),
    );
    expect(stateRules.length).toBeGreaterThan(0);
    for (const rule of stateRules) {
      expect(rule.body, rule.selector).not.toMatch(/^\s*background(-image)?\s*:/m);
    }
  });

  it('adds the commander crown mark as a pseudo-element, so the rim survives', () => {
    // §4's 5-o'clock mark. A pseudo-element is free in node terms — the crest is
    // still one element — and it composes with the rim instead of replacing it.
    const crown = bodyOf(".plane [data-slot='crest'][data-commander='true']::before");
    expect(crown).not.toBe('');
    expect(crown).toMatch(/content:\s*''/);
    expect(crown).toContain('position: absolute');
    // 5 o'clock on the rim, sized in D like everything else in the cluster.
    expect(crown).toContain('left: 78%');
    expect(crown).toContain('top: 78%');
    expect(crown).toContain('var(--cluster-d)');
    expect(crown).toContain('border-radius: 50%');
    expect(crown).toContain('background: var(--gold)');
    // And no non-pseudo commander rule exists to undo the base stack.
    expect(bodyOf(".plane [data-slot='crest'][data-commander='true']")).toBe('');
  });

  it('draws no substitute glyph in the aperture (§1.3)', () => {
    // "the aperture keeps its token background and accessible player name but
    // draws no substitute glyph". The procedural rune monogram is gone from the
    // stylesheet as well as from the data, so nothing can resurrect it.
    // Not anywhere in the file's declarations, under any selector.
    expect(DECLS).not.toContain('monogram');
    // And the crest's own pseudo-elements pull no text out of the DOM at all —
    // the pennant and the crown mark are shapes, and there is nothing else.
    expect(CREST_RULES.filter((r) => r.selector.includes('::')).length).toBeGreaterThan(0);
    for (const rule of CREST_RULES.filter((r) => r.selector.includes('::'))) {
      const content = /content:([^;]*);/.exec(rule.body)?.[1]?.trim();
      expect(content ?? "''", rule.selector).toBe("''");
      expect(rule.body, rule.selector).not.toContain('attr(');
    }
  });
});
