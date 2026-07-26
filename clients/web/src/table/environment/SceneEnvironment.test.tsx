/**
 * The rendered-contract gates for the shared environment mount (issue #530):
 * the §7 noninteractive contract, the §10.2 slot identity in the DOM, the §11
 * device-local theme preference, and the ADR 0019 token discipline of the
 * stylesheet.
 *
 * **What jsdom can and cannot show.** jsdom applies no CSS-module stylesheet
 * and performs no layout, so nothing here proves an appearance: no rendered
 * pixel, no computed contrast, no real rect. Every assertion below is about the
 * *declared* contract — which nodes exist, which attributes they carry, which
 * token each custom property resolves to, and what the stylesheet source
 * contains. The visual result is the maintainer's to verify in a browser.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, act } from '@testing-library/react';
import {
  DEFAULT_SCENE_THEME,
  SCENE_MOTION,
  SCENE_THEMES,
  type SceneThemeName,
} from '../../sceneTokens';
import {
  getPresentationSnapshot,
  resetPresentationSettings,
  setEnvironmentTheme,
} from '../settings/presentationSettings';
import { SceneEnvironment } from './SceneEnvironment';
import { environmentSceneVars, environmentThemeOptions } from './environmentScene';
import { ENV_LAYERS, ENV_MANIFESTS, ENV_VARIANTS, type EnvManifestKey } from './manifest';
import { planEnvironment } from './quality';

const DESKTOP = { width: 1680, height: 945 };

afterEach(() => {
  cleanup();
  resetPresentationSettings();
  localStorage.clear();
});

/** The mounted environment root, scoped to this render (mounts do not auto-clean). */
function mount(props: Partial<React.ComponentProps<typeof SceneEnvironment>> = {}) {
  const view = render(
    <SceneEnvironment quality="high" reducedMotion={false} viewport={DESKTOP} {...props} />,
  );
  const root = view.container.querySelector<HTMLElement>('[data-testid="scene-environment"]');
  expect(root).not.toBeNull();
  return { view, root: root! };
}

/** The layer nodes, keyed by `data-layer`. */
function layerNodes(root: HTMLElement): Map<string, HTMLElement> {
  const map = new Map<string, HTMLElement>();
  for (const node of root.querySelectorAll<HTMLElement>('[data-layer]')) {
    map.set(node.dataset.layer!, node);
  }
  return map;
}

describe('SceneEnvironment — §7 the noninteractive contract', () => {
  it('marks the whole subtree aria-hidden and exposes no accessible node', () => {
    const { root } = mount();
    expect(root.getAttribute('aria-hidden')).toBe('true');
    // Nothing inside is announced, focusable, or a landmark.
    expect(root.querySelector('[role]')).toBeNull();
    expect(root.querySelector('[aria-label]')).toBeNull();
    for (const svg of root.querySelectorAll('svg')) {
      expect(svg.getAttribute('aria-hidden')).toBe('true');
      expect(svg.getAttribute('focusable')).toBe('false');
    }
  });

  it('contains no focus stop, tab stop, or interactive element', () => {
    // §7 rule 1 and rule 6: "no hidden hotspots, no prebuilt clickable
    // decorations". A future interactive decoration is a new issue and a new
    // ADR consequence, not a flag flip — so any of these appearing is a bug.
    const { root } = mount();
    expect(root.querySelectorAll('button, a, input, select, textarea')).toHaveLength(0);
    expect(root.querySelector('[tabindex]')).toBeNull();
    expect(root.querySelector('[onclick]')).toBeNull();
  });

  it('never carries a hit-enabling style, and the stylesheet never re-enables input', () => {
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    );
    expect(css).toContain('pointer-events: none');
    // Nothing anywhere in the sheet turns it back on, and there is no cursor,
    // hover, or focus rule to suggest the backdrop is ever a target.
    expect(css).not.toContain('pointer-events: auto');
    expect(css).not.toContain(':hover');
    expect(css).not.toContain(':focus');
    expect(css).not.toContain('cursor:');
  });
});

describe('SceneEnvironment — §10.2 the slot identity in the DOM', () => {
  it('renders one node per layer, in fixed z-order, tagged with its manifest key', () => {
    const { root } = mount();
    const nodes = layerNodes(root);
    expect([...nodes.keys()]).toEqual(['l0', 'l1', 'l2', 'l3']);
    for (const [id, node] of nodes) {
      expect(node.dataset.key).toBe(`env/${DEFAULT_SCENE_THEME}/${id}`);
      expect(node.dataset.treatment).toBe('plate');
    }
  });

  it('draws the shipped plate in L0–L2 and the sprite atlas in L3 (T2)', () => {
    const { root } = mount();
    const nodes = layerNodes(root);
    const assets = ENV_MANIFESTS[DEFAULT_SCENE_THEME].assets;
    for (const [id, variant] of [
      ['l0', 'l0'],
      ['l1', 'l1'],
      ['l2', 'l2'],
    ] as const) {
      const node = nodes.get(id)!;
      expect(node.dataset.source).toBe('raster');
      const img = node.querySelector('img')!;
      expect(img.getAttribute('src')).toBe(assets[variant].src);
      // Never announced and never a hit target, in either form.
      expect(img.getAttribute('aria-hidden')).toBe('true');
      expect(img.getAttribute('alt')).toBe('');
    }
    // L3 is not a plate (§4.4): one cropped sprite per prop, all six sharing the
    // single atlas URL so the browser issues one request.
    const sprites = [...nodes.get('l3')!.querySelectorAll<HTMLImageElement>('[data-prop] img')];
    expect(sprites).toHaveLength(6);
    expect(new Set(sprites.map((img) => img.getAttribute('src')))).toEqual(
      new Set([assets.l3.src]),
    );
  });

  it('holds each plate slot with its T0 composition until the plate has pixels', () => {
    // §8.2: T0 is always the first frame. Before any decode reports back, the
    // slot is the token composition — never a hole, and never the plate's own
    // empty box.
    const { root } = mount();
    const nodes = layerNodes(root);
    for (const id of ['l0', 'l1'] as const) {
      const underlay = nodes.get(id)!.querySelector<HTMLElement>('[data-underlay]')!;
      expect(underlay.dataset.underlay).toBe(id);
      expect(underlay.dataset.revealed).toBe('false');
    }
  });

  it('retires the T0 composition once the plate reveals, so it cannot veil a layer below', () => {
    // Issue #581. A shipped plate is not necessarily opaque — L1's is a cut-out
    // disc — while its T0 plaza fill is, and by §4.3 that fill spans the whole
    // canvas at every landscape aspect. Left painted underneath, it draws from a
    // layer node that outranks L0 and hides the entire far surround, which is
    // the flat token field the issue reports in place of the Runic Vale.
    const { root } = mount();
    const nodes = layerNodes(root);
    for (const node of nodes.values()) {
      for (const plate of node.querySelectorAll<HTMLImageElement>('img')) {
        fireEvent.load(plate);
      }
    }
    const underlays = [...root.querySelectorAll<HTMLElement>('[data-underlay]')];
    // Every plated slot still has its composition mounted (the fade needs a node
    // to fade), and every one of them has stood down.
    expect(underlays.map((node) => node.dataset.underlay)).toEqual(['l0', 'l1']);
    for (const underlay of underlays) {
      expect(underlay.dataset.revealed).toBe('true');
    }
    // The plates themselves are the visible form now.
    for (const id of ['l0', 'l1', 'l2'] as const) {
      expect(nodes.get(id)!.querySelector('img')!.dataset.loaded).toBe('true');
    }
  });

  it('re-enters a slot unrevealed when the theme changes, so the new plate is never a hole', () => {
    // The slot is keyed by the plate URL: a theme switch brings the retired
    // composition back to hold the slot while the new plate is in flight.
    const { root, view } = mount();
    for (const plate of root.querySelectorAll<HTMLImageElement>('img')) fireEvent.load(plate);
    expect(root.querySelector<HTMLElement>('[data-underlay]')!.dataset.revealed).toBe('true');
    act(() => setEnvironmentTheme('moonlitRuins'));
    view.rerender(<SceneEnvironment quality="high" reducedMotion={false} viewport={DESKTOP} />);
    const underlay = root.querySelector<HTMLElement>('[data-underlay]')!;
    expect(underlay.dataset.revealed).toBe('false');
  });

  it('cross-fades the composition out on the same staging class the plate fades in on', () => {
    // jsdom applies no stylesheet, so the retirement is pinned in the source:
    // one duration token for both halves means reduced motion snaps them
    // together, with no media query to remember (ADR 0019).
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    );
    expect(css).toContain("[data-revealed='true']");
    expect(css).toMatch(/\.underlay\b[^}]*transition:\s*opacity var\(--env-motion-staging\)/);
    expect(css).toMatch(/\.underlay\[data-revealed='true'\]\s*\{\s*opacity:\s*0;/);
  });

  it('authors L0–L2 on the crop’s viewBox whenever it falls back to the placeholder', () => {
    // The placeholder is permanent (§10.5, last paragraph): it is the T0 form,
    // the per-layer failure fallback, and Lite's L0. Its crop path is the same
    // one the plate takes, which is what makes the swap a swap.
    const failed = ENV_VARIANTS.map((v) => `env/${DEFAULT_SCENE_THEME}/${v}` as EnvManifestKey);
    const { root } = mount({ failedKeys: failed, quality: 'high' });
    const nodes = layerNodes(root);
    const box = nodes.get('l1')!.querySelector('svg')!.getAttribute('viewBox')!;
    const [, , w, h] = box.split(' ').map(Number);
    // 16:9 uses 76.2 % of the 2333-wide authoring canvas, full height.
    expect(w).toBeCloseTo(2333 * 0.762, 0);
    expect(h).toBe(1000);
  });

  it('anchors L3 in composed-canvas fractions, so 16:9 and 21:9 place a prop alike', () => {
    // §4.4's mechanism, asserted on the raster sprites rather than on the
    // silhouettes: a prop's box is a fraction of the canvas, so widening the
    // viewport to 21:9 moves it by exactly the width change and nothing else.
    const wide = { width: 2100, height: 900 };
    const { view, root } = mount();
    const at = (r: HTMLElement, key: string): DOMStringMap =>
      r.querySelector<HTMLElement>(`[data-prop='${key}']`)!.dataset;
    expect(at(root, 'lantern-top-left').anchor).toBe('top-left');
    view.unmount();
    const second = mount({ viewport: wide });
    const node = second.root.querySelector<HTMLElement>("[data-prop='crystal-top-right']")!;
    // Right-anchored: measured inboard from the right edge of the canvas.
    const right = wide.width - (parseFloat(node.style.left) + parseFloat(node.style.width));
    expect(right).toBeGreaterThan(0);
    expect(right).toBeLessThan(wide.width * 0.1);
  });

  it('places every prop on the manifest anchor its raster sprite will use', () => {
    const { root } = mount();
    const props = [...root.querySelectorAll<HTMLElement>('[data-prop]')];
    const manifest = ENV_MANIFESTS[DEFAULT_SCENE_THEME];
    expect(props.map((node) => node.dataset.prop)).toEqual(manifest.props.map((p) => p.key));
    for (const [index, node] of props.entries()) {
      expect(node.dataset.anchor).toBe(manifest.props[index]!.anchor);
      expect(node.dataset.mass).toBe(manifest.props[index]!.mass);
    }
  });

  it('draws both §2.4 lips whenever L2 takes its procedural form', () => {
    // On the plate the lips are painted in; the placeholder draws them, and the
    // portrait recomposition re-anchors them to the canvas, which is the one
    // case a plate cannot serve.
    const { root } = mount({ viewport: { width: 390, height: 844 } });
    const lips = [...root.querySelectorAll<HTMLElement>('[data-lip]')];
    expect(lips.map((node) => node.dataset.lip)).toEqual(['top', 'bottom']);
  });

  it('drops L2 to its lips and L3 entirely in phone portrait', () => {
    const { root } = mount({ viewport: { width: 390, height: 844 } });
    const nodes = layerNodes(root);
    expect(root.dataset.portrait).toBe('true');
    expect(nodes.get('l2')!.dataset.treatment).toBe('lips-only');
    expect(nodes.has('l3')).toBe(false);
    // The lips survive the recomposition; the rim and verge do not.
    expect(root.querySelectorAll('[data-lip]')).toHaveLength(2);
  });

  it('keeps L1 drawing at Lite and reports the layers that stepped off', () => {
    const { root } = mount({ quality: 'lite' });
    const nodes = layerNodes(root);
    expect(nodes.get('l1')!.dataset.treatment).toBe('plate');
    expect(nodes.get('l1')!.dataset.key).toBe(`env/${DEFAULT_SCENE_THEME}/l1-half`);
    expect(nodes.get('l0')!.dataset.treatment).toBe('token-gradient');
    expect(nodes.has('l2')).toBe(false);
    expect(nodes.has('l3')).toBe(false);
  });

  it('falls one failed layer back without touching a sibling (§8.3)', () => {
    const failed: EnvManifestKey[] = [`env/${DEFAULT_SCENE_THEME}/l1`];
    const { root } = mount({ failedKeys: failed });
    const nodes = layerNodes(root);
    expect(nodes.get('l1')!.dataset.treatment).toBe('token-gradient');
    expect(nodes.get('l1')!.dataset.degraded).toBe('true');
    expect(nodes.get('l0')!.dataset.treatment).toBe('plate');
    expect(nodes.get('l0')!.dataset.degraded).toBe('false');
  });

  it('falls a wholly failed theme back to the default, still illustrated', () => {
    setEnvironmentTheme('moonlitRuins');
    const failed = ENV_VARIANTS.map((v) => `env/moonlitRuins/${v}` as EnvManifestKey);
    const { root } = mount({ failedKeys: failed });
    expect(root.dataset.themeFallback).toBe('true');
    expect(root.dataset.theme).toBe(DEFAULT_SCENE_THEME);
    // Not a hole and not a flat black field: the T0 plaza composition still draws.
    expect(layerNodes(root).get('l1')!.dataset.treatment).toBe('token-gradient');
    expect(root.querySelectorAll('svg').length).toBeGreaterThan(0);
  });

  it('gives two mounted environments distinct gradient ids', () => {
    // The pregame stage and the match can both be mounted during a transition;
    // two identical `id`s would make one steal the other's gradients. Mounted
    // with the L1 plate failed, so both fall to the placeholder composition and
    // the gradient ids are live — the state the collision would actually bite in.
    const failed: EnvManifestKey[] = [`env/${DEFAULT_SCENE_THEME}/l1`];
    const props = {
      quality: 'high' as const,
      reducedMotion: false,
      viewport: DESKTOP,
      failedKeys: failed,
    };
    const first = render(<SceneEnvironment {...props} />);
    const second = render(<SceneEnvironment {...props} />);
    const idsOf = (c: HTMLElement): string[] => [...c.querySelectorAll('[id]')].map((n) => n.id);
    const a = idsOf(first.container);
    const b = idsOf(second.container);
    expect(a.length).toBeGreaterThan(0);
    expect(a.some((id) => b.includes(id))).toBe(false);
  });
});

describe('SceneEnvironment — §8.3 a plate that fails at runtime', () => {
  it('degrades only the layer whose <img> errored, and leaves its siblings drawn', () => {
    // The live failure path, not a synthetic `failedKeys`: the raster branch's
    // own `onError` fires and the mount re-plans. No sibling moves, no rect
    // changes, and the match was interactive throughout.
    const { root } = mount();
    const nodes = layerNodes(root);
    expect(nodes.get('l1')!.dataset.source).toBe('raster');
    act(() => {
      fireEvent.error(nodes.get('l1')!.querySelector('img')!);
    });
    const after = layerNodes(root);
    expect(after.get('l1')!.dataset.treatment).toBe('token-gradient');
    expect(after.get('l1')!.dataset.degraded).toBe('true');
    expect(after.get('l1')!.querySelector('svg')).not.toBeNull();
    // Every other layer keeps its resolved plate.
    for (const id of ['l0', 'l2'] as const) {
      expect(after.get(id)!.dataset.source).toBe('raster');
      expect(after.get(id)!.dataset.degraded).toBe('false');
    }
    // Still four slots, still in z-order: a failure is never a hole.
    expect([...after.keys()]).toEqual(['l0', 'l1', 'l2', 'l3']);
  });

  it('drops L3 to no props when the sprite atlas fails, and never retries', () => {
    const { root } = mount();
    const sprites = root.querySelectorAll<HTMLImageElement>("[data-layer='l3'] img");
    expect(sprites.length).toBe(6);
    act(() => {
      fireEvent.error(sprites[0]!);
    });
    // §8.3's T0 form for L3 is nothing at all — L3 has no token composition.
    // The three layers that carry the theme's identity are untouched.
    expect(layerNodes(root).has('l3')).toBe(false);
    expect(layerNodes(root).get('l1')!.dataset.source).toBe('raster');
  });
});

describe('SceneEnvironment — §8.2 the arena is never absent while a plate loads', () => {
  /** The §8.2 plate reveal gate: `false` until the decode reported back. */
  const revealed = (node: HTMLElement): string | undefined =>
    node.querySelector<HTMLImageElement>('img')?.dataset.loaded;

  it('keeps the T0 plaza and medallion under an unrevealed L1 plate', () => {
    // jsdom never decodes, so this IS the "slow request / lazily-scheduled L1"
    // state, held indefinitely — precisely the frame the arena must not be a
    // hole in. The plate is present but unrevealed; the plaza composition and
    // the medallion mark are already drawn beneath it.
    const { root } = mount();
    const l1 = layerNodes(root).get('l1')!;
    expect(l1.dataset.source).toBe('raster');
    expect(revealed(l1)).toBe('false');
    const under = l1.querySelector('svg')!;
    expect(under).not.toBeNull();
    // §8.3's "L1 fails ⇒ T0 plaza composition" and §8.2's "T0 throughout
    // loading" are the same composition, so the medallion is in both.
    expect(under.querySelector('[data-medallion]')).not.toBeNull();
    // L0 keeps its token surround underneath for the same reason: two children
    // in the slot — the composition, and the plate stacked over it.
    const l0 = layerNodes(root).get('l0')!;
    expect(l0.children).toHaveLength(2);
    expect(l0.firstElementChild!.tagName).toBe('DIV');
    expect(revealed(l0)).toBe('false');
  });

  it('reveals the plate only on load, and cross-fades rather than swapping', () => {
    const { root } = mount();
    const l1 = layerNodes(root).get('l1')!;
    act(() => {
      fireEvent.load(l1.querySelector('img')!);
    });
    expect(revealed(l1)).toBe('true');
    // The composition underneath is NOT torn down by the reveal: the plate is
    // opaque and covers it, so there is no frame between the two.
    expect(l1.querySelector('svg')).not.toBeNull();
    // The reveal is an opacity transition on the `staging` class, collapsed to
    // 0ms under reduced motion — not a display swap, and not a JS timer.
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    );
    expect(css).toContain("[data-loaded='true']");
    expect(css).toContain('transition: opacity var(--env-motion-staging)');
  });

  it('re-enters a new theme’s plate unrevealed, so a theme change never flashes', () => {
    const { root } = mount();
    const l1 = () => layerNodes(root).get('l1')!;
    act(() => {
      fireEvent.load(l1().querySelector('img')!);
    });
    expect(revealed(l1())).toBe('true');
    act(() => setEnvironmentTheme('sunlitObservatory'));
    // A different source is a different plate: it starts hidden and the T0
    // composition is what carries the frame until it decodes.
    expect(l1().querySelector('img')!.getAttribute('src')).toBe(
      ENV_MANIFESTS.sunlitObservatory.assets.l1.src,
    );
    expect(revealed(l1())).toBe('false');
    expect(l1().querySelector('svg')).not.toBeNull();
  });

  it('reveals a plate that was already decoded before React attached onLoad', () => {
    // The HTTP-cache race: a cached plate can complete before `onLoad` is
    // bound. A node stranded at opacity 0 there would be a hole on the FAST
    // path, so the attach itself reads `complete`.
    const proto = window.HTMLImageElement.prototype;
    const complete = Object.getOwnPropertyDescriptor(proto, 'complete');
    const natural = Object.getOwnPropertyDescriptor(proto, 'naturalWidth');
    Object.defineProperty(proto, 'complete', { configurable: true, get: () => true });
    Object.defineProperty(proto, 'naturalWidth', { configurable: true, get: () => 3360 });
    try {
      const { root } = mount();
      expect(revealed(layerNodes(root).get('l1')!)).toBe('true');
    } finally {
      if (complete) Object.defineProperty(proto, 'complete', complete);
      else delete (proto as unknown as Record<string, unknown>).complete;
      if (natural) Object.defineProperty(proto, 'naturalWidth', natural);
      else delete (proto as unknown as Record<string, unknown>).naturalWidth;
    }
  });

  it('still falls a failed plate back to its T0 form, with the arena unbroken', () => {
    // The carried §8.3 path, re-asserted against the underlay: the plate node
    // goes away and the composition that was already beneath it stays put, so
    // the failure changes no rect and produces no empty frame.
    const { root } = mount();
    const before = layerNodes(root).get('l1')!.querySelectorAll('svg').length;
    act(() => {
      fireEvent.error(layerNodes(root).get('l1')!.querySelector('img')!);
    });
    const after = layerNodes(root).get('l1')!;
    expect(after.dataset.treatment).toBe('token-gradient');
    expect(after.querySelector('img')).toBeNull();
    expect(after.querySelectorAll('svg')).toHaveLength(before);
  });

  it('never leaves L1 without a composition, at any tier or aspect', () => {
    // The §8.2/§8.3 invariant stated directly: whatever the plan resolves to,
    // the L1 slot always has drawn art on the very first frame — no decode, no
    // network, no timer involved.
    const cases: Parameters<typeof mount>[0][] = [
      {},
      { quality: 'standard' },
      { quality: 'lite' },
      { reducedMotion: true },
      { viewport: { width: 390, height: 844 } },
      { viewport: { width: 2560, height: 1080 } },
      { failedKeys: [`env/${DEFAULT_SCENE_THEME}/l1`] },
    ];
    for (const props of cases) {
      const { view, root } = mount(props);
      const l1 = layerNodes(root).get('l1')!;
      expect(l1.querySelector('svg'), JSON.stringify(props)).not.toBeNull();
      view.unmount();
    }
  });
});

describe('SceneEnvironment — §4 the crop path is one path for both forms', () => {
  it('declares `cover` for every plate — the §4.2 centred crop, not a stretch', () => {
    // jsdom applies no CSS-module stylesheet and performs no layout, so the
    // rendered crop is not observable here. What IS checkable is that the plate
    // declares `object-fit: cover`, which for a 21:9 source is exactly §4.2:
    // every landscape aspect below 21:9 matches the plate's height and takes a
    // centred horizontal slice, and ultrawide reveals rather than stretches.
    // The pixels are the maintainer's to confirm in a browser.
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    );
    expect(css).toContain('object-fit: cover');
  });

  it('crops the placeholder to the §4.2 table at every supported aspect', () => {
    const rows: [number, { width: number; height: number }][] = [
      [1, { width: 2333, height: 1000 }], // 21:9 — the whole plate
      [0.762, { width: 1680, height: 945 }], // 16:9
      [0.686, { width: 1680, height: 1050 }], // 16:10
      [0.643, { width: 1500, height: 1000 }], // 3:2
      [0.617, { width: 1180, height: 820 }], // tablet landscape floor
      [0.571, { width: 1024, height: 768 }], // 4:3 — the tightest landscape crop
    ];
    const failed: EnvManifestKey[] = [`env/${DEFAULT_SCENE_THEME}/l1`];
    for (const [fraction, viewport] of rows) {
      const { view, root } = mount({ viewport, failedKeys: failed });
      const box = layerNodes(root).get('l1')!.querySelector('svg')!.getAttribute('viewBox')!;
      const [x, , w, h] = box.split(' ').map(Number);
      expect(w! / 2333).toBeCloseTo(fraction, 2);
      // Centred horizontally at every aspect, full height, never stretched.
      expect(x!).toBeCloseTo((2333 - w!) / 2, 1);
      expect(h).toBe(1000);
      view.unmount();
    }
  });
});

describe('SceneEnvironment — the completed theme family (#559)', () => {
  it('draws every alternate as four separate layers with six addressable props', () => {
    setEnvironmentTheme('moonlitRuins');
    const { root } = mount();
    expect(root.dataset.composition).toBe('layered');
    expect(root.dataset.composed).toBe('false');
    const nodes = layerNodes(root);
    expect(nodes.get('l1')!.querySelector('img')!.getAttribute('src')).toBe(
      ENV_MANIFESTS.moonlitRuins.assets.l1.src,
    );
    expect(nodes.get('l0')!.dataset.source).toBe('raster');
    expect(nodes.get('l2')!.dataset.source).toBe('raster');
    expect(nodes.get('l3')!.dataset.source).toBe('raster');
    expect(root.querySelectorAll('[data-prop]')).toHaveLength(6);
    // Three plates plus six frames sharing one atlas URL.
    expect(root.querySelectorAll('img')).toHaveLength(9);
  });

  it('switches between layered themes without re-mounting', () => {
    const { root } = mount();
    expect(root.dataset.composition).toBe('layered');
    expect(layerNodes(root).has('l3')).toBe(true);
    act(() => setEnvironmentTheme('verdantCanals'));
    // The SAME root node re-renders: the backdrop is never re-mounted, so
    // nothing behind it flashes and the match is not interrupted (§11).
    expect(root.dataset.composition).toBe('layered');
    expect(layerNodes(root).get('l1')!.dataset.key).toBe('env/verdantCanals/l1');
    act(() => setEnvironmentTheme('runicVale'));
    expect(root.dataset.composition).toBe('layered');
    expect(layerNodes(root).get('l1')!.dataset.key).toBe('env/runicVale/l1');
  });

  it('falls back only the failed alternate-theme layer', () => {
    setEnvironmentTheme('sunlitObservatory');
    const { root } = mount();
    act(() => {
      fireEvent.error(layerNodes(root).get('l1')!.querySelector('img')!);
    });
    // §8.3 — the L1 plaza falls back without disturbing the alternate theme's
    // surround, edge, or six independently addressable props.
    expect(root.dataset.composed).toBe('false');
    expect(root.dataset.theme).toBe('sunlitObservatory');
    expect(layerNodes(root).get('l1')!.dataset.source).toBe('procedural');
    expect(layerNodes(root).get('l0')!.dataset.source).toBe('raster');
    expect(layerNodes(root).get('l2')!.dataset.source).toBe('raster');
    expect(layerNodes(root).get('l3')!.dataset.source).toBe('raster');
    expect(root.querySelectorAll('[data-prop]')).toHaveLength(6);
  });
});

describe('SceneEnvironment — §1.1 parallax is transform-only and staging-driven', () => {
  it('displaces each layer by its own factor × E × bias, and nothing else', () => {
    const { root } = mount({ bias: { x: 1, y: 0 } });
    const nodes = layerNodes(root);
    for (const [id, node] of nodes) {
      const factor = ENV_LAYERS[id as keyof typeof ENV_LAYERS].parallax;
      const match = /^translate3d\((-?[\d.]+)px, (-?[\d.]+)px, 0\)$/.exec(node.style.transform);
      expect(match).not.toBeNull();
      // §1.1: offset = factor × E, with E = 12 px at desktop High.
      expect(Number(match![1])).toBeCloseTo(factor * 12, 4);
      expect(Number(match![2])).toBe(0);
    }
  });

  it('collapses parallax to zero under reduced motion', () => {
    const { root } = mount({ bias: { x: 1, y: 1 }, reducedMotion: true });
    for (const node of layerNodes(root).values()) {
      expect(node.style.transform).toBe('translate3d(0px, 0px, 0)');
    }
  });

  it('centres by default — the environment never moves without a staging delta', () => {
    const { root } = mount();
    for (const node of layerNodes(root).values()) {
      expect(node.style.transform).toBe('translate3d(0px, 0px, 0)');
    }
  });
});

describe('SceneEnvironment — §7.1 the ambient tier reaches the DOM', () => {
  it('publishes the resolved ambient level for the stylesheet to gate on', () => {
    const ambientAt = (props: Parameters<typeof mount>[0]): string | undefined => {
      const { view, root } = mount(props);
      const level = root.dataset.ambient;
      view.unmount();
      return level;
    };
    expect(ambientAt({ quality: 'high' })).toBe('l0+l3');
    expect(ambientAt({ quality: 'standard' })).toBe('l0-half');
    expect(ambientAt({ quality: 'lite' })).toBe('off');
    expect(ambientAt({ quality: 'high', reducedMotion: true })).toBe('off');
  });
});

describe('environmentSceneVars — ADR 0019 token discipline', () => {
  it('publishes all twelve palette slots from the token set, never a literal', () => {
    const plan = planEnvironment({
      theme: DEFAULT_SCENE_THEME,
      quality: 'high',
      reducedMotion: false,
      viewport: DESKTOP,
    });
    const vars = environmentSceneVars(plan, false);
    const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
    expect(vars['--env-surround-top']).toBe(theme.surroundTop);
    expect(vars['--env-surround-base']).toBe(theme.surroundBase);
    expect(vars['--env-water']).toBe(theme.water);
    expect(vars['--env-plaza-core']).toBe(theme.plazaCore);
    expect(vars['--env-plaza-edge']).toBe(theme.plazaEdge);
    expect(vars['--env-paving']).toBe(theme.paving);
    expect(vars['--env-medallion']).toBe(theme.medallion);
    expect(vars['--env-rim']).toBe(theme.rim);
    expect(vars['--env-verge']).toBe(theme.verge);
    expect(vars['--env-prop-warm']).toBe(theme.propWarm);
    expect(vars['--env-prop-cool']).toBe(theme.propCool);
    expect(vars['--env-glow']).toBe(theme.glow);
  });

  it('runs every layer transition on the `staging` class and collapses it to 0ms', () => {
    const plan = planEnvironment({
      theme: DEFAULT_SCENE_THEME,
      quality: 'high',
      reducedMotion: false,
      viewport: DESKTOP,
    });
    expect(environmentSceneVars(plan, false)['--env-motion-staging']).toBe(
      `${SCENE_MOTION.staging.ms}ms`,
    );
    expect(environmentSceneVars(plan, true)['--env-motion-staging']).toBe('0ms');
    expect(environmentSceneVars(plan, false)['--env-ease-staging']).toBe(SCENE_MOTION.staging.ease);
  });

  it('changes nothing but the duration under reduced motion', () => {
    const plan = planEnvironment({
      theme: DEFAULT_SCENE_THEME,
      quality: 'high',
      reducedMotion: false,
      viewport: DESKTOP,
    });
    const strip = (vars: object): Record<string, unknown> =>
      Object.fromEntries(Object.entries(vars).filter(([key]) => !key.startsWith('--env-motion')));
    expect(strip(environmentSceneVars(plan, true))).toEqual(
      strip(environmentSceneVars(plan, false)),
    );
  });

  it('introduces no literal hex and no literal duration in the stylesheet', () => {
    // The same gate the pregame surfaces are held to. Comments carry prose, not
    // values, so they are stripped before scanning.
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    ).replace(/\/\*[\s\S]*?\*\//g, '');
    expect(css.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull();
    expect(css.match(/\b\d+(\.\d+)?m?s\b/g)).toBeNull();
    expect(css).toContain('var(--env-motion-staging)');
  });

  it('never applies a runtime blur — §1.2 bans it at every tier', () => {
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/environment/environment.module.css'),
      'utf8',
    );
    expect(css).not.toContain('blur(');
    const { root } = mount();
    expect(root.outerHTML).not.toContain('blur');
  });
});

describe('environment theme preference — §11 device-local, never protocol', () => {
  it('defaults to Runic Vale with nothing stored', () => {
    expect(getPresentationSnapshot().environmentTheme).toBe(DEFAULT_SCENE_THEME);
  });

  it('persists a choice under the documented key and republishes it', () => {
    setEnvironmentTheme('verdantCanals');
    expect(localStorage.getItem('rune.presentation.environmentTheme')).toBe('verdantCanals');
    expect(getPresentationSnapshot().environmentTheme).toBe('verdantCanals');
    resetPresentationSettings();
    expect(getPresentationSnapshot().environmentTheme).toBe('verdantCanals');
  });

  it('rewrites a stale stored key rather than re-resolving it every mount (§8.3)', () => {
    // `emberReach` was a shipped theme before the approved images renamed the
    // family (§12 conflict 1); a device that stored it must self-heal.
    localStorage.setItem('rune.presentation.environmentTheme', 'emberReach');
    resetPresentationSettings();
    expect(getPresentationSnapshot().environmentTheme).toBe(DEFAULT_SCENE_THEME);
    expect(localStorage.getItem('rune.presentation.environmentTheme')).toBeNull();
  });

  it('ignores an unknown value rather than storing it', () => {
    setEnvironmentTheme('nopeNotATheme' as SceneThemeName);
    expect(getPresentationSnapshot().environmentTheme).toBe(DEFAULT_SCENE_THEME);
    expect(localStorage.getItem('rune.presentation.environmentTheme')).toBeNull();
  });

  it('applies a mid-match change live, without a reload or a re-mount', () => {
    const { root } = mount();
    expect(root.dataset.theme).toBe(DEFAULT_SCENE_THEME);
    act(() => setEnvironmentTheme('sunlitObservatory'));
    // The SAME node re-renders — the backdrop is never re-mounted by a theme
    // change, so nothing behind it flashes.
    expect(root.dataset.theme).toBe('sunlitObservatory');
    // The slot identity is unchanged — only the palette and the plate set move.
    expect(layerNodes(root).get('l1')!.dataset.key).toBe('env/sunlitObservatory/l1');
  });

  it('offers every shipped theme in the settings surface, labelled from the tokens', () => {
    const options = environmentThemeOptions();
    expect(options.map((o) => o.value).sort()).toEqual(
      (Object.keys(SCENE_THEMES) as SceneThemeName[]).sort(),
    );
    for (const option of options) {
      expect(option.label).toBe(SCENE_THEMES[option.value].label);
    }
  });
});
