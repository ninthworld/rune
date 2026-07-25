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
import { cleanup, render, act } from '@testing-library/react';
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

  it('draws SVG in every slot today, and no <img> (nothing is fetched at T1)', () => {
    const { root } = mount();
    expect(root.querySelectorAll('svg').length).toBeGreaterThan(0);
    expect(root.querySelectorAll('img')).toHaveLength(0);
  });

  it('authors L0–L2 on the crop’s viewBox, and L3 in canvas space', () => {
    const { root } = mount();
    const nodes = layerNodes(root);
    for (const id of ['l0', 'l1', 'l2']) {
      const box = nodes.get(id)!.querySelector('svg')!.getAttribute('viewBox')!;
      const [, , w, h] = box.split(' ').map(Number);
      // 16:9 uses 76.2 % of the 2333-wide authoring canvas, full height.
      expect(w).toBeCloseTo(2333 * 0.762, 0);
      expect(h).toBe(1000);
    }
    // L3 does not crop (§4.4): its sprites are anchored in composed-canvas
    // fractions so 16:9 and 21:9 place them identically.
    expect(nodes.get('l3')!.querySelector('svg')!.getAttribute('viewBox')).toBe('0 0 1000 1000');
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

  it('draws both §2.4 lips, which are the only L2 content allowed to cross Zone A', () => {
    const { root } = mount();
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
    // two identical `id`s would make one steal the other's gradients.
    const first = render(
      <SceneEnvironment quality="high" reducedMotion={false} viewport={DESKTOP} />,
    );
    const second = render(
      <SceneEnvironment quality="high" reducedMotion={false} viewport={DESKTOP} />,
    );
    const idsOf = (c: HTMLElement): string[] => [...c.querySelectorAll('[id]')].map((n) => n.id);
    const a = idsOf(first.container);
    const b = idsOf(second.container);
    expect(a.length).toBeGreaterThan(0);
    expect(a.some((id) => b.includes(id))).toBe(false);
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
