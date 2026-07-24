/**
 * The passive effects layer (issue #482): structural-snapshot tests over the
 * built draw program (ADR 0011's approach — structure, not pixels), the
 * explicit idle-zero-cost gate, the quality/density particle budgets, the
 * carried blocker-link shape, endpoint tracking, and the reduced-motion forms.
 * All timestamps controlled — no wall clock, no rAF.
 */
import { describe, expect, it } from 'vitest';
import type { Rect } from './scene';
import { COMBAT_LINK, SURFACES } from '../tokens';
import { SCENE_HUES } from '../sceneTokens';
import {
  EFFECT_TIMING,
  EffectsLayer,
  PARTICLE_CAP,
  createEffectsTicker,
  type EffectDensity,
  type EffectQuality,
} from './effects';

/** A layer over a fixed two-entity board. */
function make(
  opts: {
    quality?: EffectQuality;
    density?: EffectDensity;
    reducedMotion?: boolean;
    rects?: Map<string, Rect>;
  } = {},
) {
  const rects =
    opts.rects ??
    new Map<string, Rect>([
      ['atk', { x: 100, y: 400, w: 66, h: 92 }],
      ['blk', { x: 400, y: 120, w: 66, h: 92 }],
    ]);
  const layer = new EffectsLayer({
    quality: opts.quality ?? 'high',
    density: opts.density ?? 'full',
    reducedMotion: opts.reducedMotion ?? false,
    rects: (ref) => rects.get(ref),
  });
  return { layer, rects };
}

describe('EffectsLayer zero idle cost (the binding ADR 0030 rule)', () => {
  it('performs NO draws across N ticks with no live effects', () => {
    const { layer } = make();
    for (let tick = 0; tick < 20; tick += 1) {
      expect(layer.advance(tick * 16)).toBe(false);
    }
    expect(layer.stats.draws).toBe(0);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('returns to zero cost after a transient retires', () => {
    const { layer } = make();
    layer.spawn({ category: 'resolution', target: { ref: 'atk' }, accent: SCENE_HUES.gold.value });
    layer.advance(0);
    layer.advance(EFFECT_TIMING.resolutionMs + 1); // expiry redraw
    const draws = layer.stats.draws;
    for (let tick = 0; tick < 10; tick += 1) {
      expect(layer.advance(1000 + tick * 16)).toBe(false);
    }
    expect(layer.stats.draws).toBe(draws);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('is passive: never a hit target', () => {
    const { layer } = make();
    expect(layer.root.eventMode).toBe('none');
  });
});

describe('EffectsLayer v1 vocabulary (data-driven categories)', () => {
  it('draws a dash-crawl path with an arrowhead terminating at the target', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'path:a1',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'blk' },
        accent: SURFACES.targeting,
      },
    ]);
    layer.advance(0);
    const segments = layer.lastProgram.filter(
      (op) => op.op === 'segment' && op.category === 'targeting-path',
    );
    expect(segments.length).toBeGreaterThan(3);
    // The pending path crawls: a later frame shifts the dash pattern.
    expect(layer.advance(300)).toBe(true);
    expect(layer.lastProgram).not.toEqual(segments);
  });

  it('draws the carried doubled-stroke blocker link with its end node — static', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'link:blk',
        category: 'blocker-link',
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
    ]);
    expect(layer.advance(0)).toBe(true);
    const strokes = layer.lastProgram.filter((op) => op.op === 'segment');
    const nodes = layer.lastProgram.filter((op) => op.op === 'circle');
    expect(strokes).toHaveLength(2); // the doubled parallel stroke (carried shape)
    expect(nodes).toHaveLength(1); // the blocker-end node
    // A link alone is static geometry: after the draw, back to zero cost.
    expect(layer.advance(16)).toBe(false);
    expect(layer.advance(32)).toBe(false);
  });

  it('retires an effect whose endpoint left play — no stale line, no idle leak', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'path:x',
        category: 'attack-path',
        from: { ref: 'atk' },
        to: { ref: 'gone' },
        accent: SURFACES.targeting,
      },
    ]);
    layer.advance(0);
    expect(layer.lastProgram).toHaveLength(0);
    // The unresolved path is retired from the live set, not just the program:
    // it must never keep "animating" an empty frame forever.
    expect(layer.hasLiveEffects()).toBe(false);
    expect(layer.needsFrame()).toBe(false);
    expect(layer.advance(16)).toBe(false);
    expect(layer.advance(32)).toBe(false);
    // A new authoritative set (same value) re-attempts the effect.
    layer.setPersistent([
      {
        id: 'path:x',
        category: 'attack-path',
        from: { ref: 'atk' },
        to: { ref: 'gone' },
        accent: SURFACES.targeting,
      },
    ]);
    expect(layer.advance(48)).toBe(true);
  });

  it('keeps resolvable effects while retiring the unresolvable in one set', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'link:blk',
        category: 'blocker-link',
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
      {
        id: 'path:x',
        category: 'attack-path',
        from: { ref: 'atk' },
        to: { ref: 'gone' },
        accent: SURFACES.targeting,
      },
    ]);
    layer.advance(0);
    // The link drew; the unresolved path retired — and with only static
    // geometry left, no further frames are needed.
    expect(layer.lastProgram.filter((op) => op.op === 'segment')).toHaveLength(2);
    expect(layer.hasLiveEffects()).toBe(true);
    expect(layer.needsFrame()).toBe(false);
    expect(layer.advance(16)).toBe(false);
  });

  it('reconciles the persistent set by value — an unchanged set costs nothing', () => {
    const { layer } = make();
    const effects = [
      {
        id: 'link:blk',
        category: 'blocker-link' as const,
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
    ];
    layer.setPersistent(effects);
    layer.advance(0);
    const draws = layer.stats.draws;
    layer.setPersistent([...effects.map((e) => ({ ...e }))]);
    expect(layer.advance(16)).toBe(false);
    expect(layer.stats.draws).toBe(draws);
  });

  it('parameterizes by category + rects + accent, never bespoke', () => {
    const { layer } = make();
    layer.spawn({
      category: 'impact',
      target: { ref: 'blk' },
      accent: SCENE_HUES.red.value,
      magnitude: 2,
    });
    layer.advance(0);
    for (const op of layer.lastProgram) {
      expect(op.category).toBe('impact');
      expect(op.color).toBe(SCENE_HUES.red.value);
    }
  });
});

describe('EffectsLayer particle budgets (quality caps × density control)', () => {
  it('pins the pooled caps to the budget numbers', () => {
    expect(PARTICLE_CAP).toEqual({ high: 400, standard: 150, lite: 40 });
    expect(EFFECT_TIMING.impactMs).toBeLessThanOrEqual(600);
    expect(EFFECT_TIMING.resolutionMs).toBeLessThanOrEqual(600);
  });

  it('never exceeds the quality cap across simultaneous bursts', () => {
    const { layer } = make({ quality: 'standard' });
    for (let i = 0; i < 12; i += 1) {
      layer.spawn({ category: 'impact', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    }
    expect(layer.stats.liveParticles).toBeLessThanOrEqual(PARTICLE_CAP.standard);
    layer.advance(0);
    layer.advance(EFFECT_TIMING.impactMs + 1);
    // The pool frees fully when the bursts retire.
    expect(layer.stats.liveParticles).toBe(0);
  });

  it('renders pulses only at Lite (particle spawns suppressed)', () => {
    const { layer } = make({ quality: 'lite' });
    layer.spawn({ category: 'impact', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    layer.advance(0);
    expect(layer.stats.liveParticles).toBe(0);
    const circles = layer.lastProgram.filter((op) => op.op === 'circle');
    expect(circles).toHaveLength(1); // the category's default pulse ring
    expect(circles[0]!.op === 'circle' && circles[0]!.fill).toBe(false);
  });

  it('honors the density control independently of quality', () => {
    const reduced = make({ quality: 'high', density: 'reduced' });
    reduced.layer.spawn({
      category: 'impact',
      target: { ref: 'atk' },
      accent: SCENE_HUES.red.value,
    });
    expect(reduced.layer.stats.liveParticles).toBe(Math.round(24 * 0.4));

    const minimal = make({ quality: 'high', density: 'minimal' });
    minimal.layer.spawn({
      category: 'impact',
      target: { ref: 'atk' },
      accent: SCENE_HUES.red.value,
    });
    expect(minimal.layer.stats.liveParticles).toBe(0);
  });
});

describe('EffectsLayer ticker stop policy (createEffectsTicker)', () => {
  /** Drive the extracted mount tick against counting host hooks. */
  function ticker(layer: EffectsLayer) {
    const host = { renders: 0, stops: 0, wakeDelays: [] as number[] };
    const tick = createEffectsTicker(layer, {
      render: () => {
        host.renders += 1;
      },
      scheduleWake: (delayMs) => {
        host.wakeDelays.push(delayMs);
      },
      stop: () => {
        host.stops += 1;
      },
    });
    return { tick, host };
  }

  it('stops after the first clean frame of a static blocker link', () => {
    const { layer } = make();
    const { tick, host } = ticker(layer);
    layer.setPersistent([
      {
        id: 'link:blk',
        category: 'blocker-link',
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
    ]);
    tick(0); // draws the link, then stops immediately because it is static
    expect(host.renders).toBe(1);
    expect(host.stops).toBe(1);
    expect(host.wakeDelays).toEqual([]);
  });

  it('stops after the static frame of a reduced-motion path', () => {
    const { layer } = make({ reducedMotion: true });
    const { tick, host } = ticker(layer);
    layer.setPersistent([
      {
        id: 'path:a1',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'blk' },
        accent: SURFACES.targeting,
      },
    ]);
    tick(0);
    expect(host.renders).toBe(1);
    expect(host.stops).toBe(1);
    expect(host.wakeDelays).toEqual([]);
  });

  it('sleeps through a reduced-motion hold and wakes once to retire it', () => {
    const { layer } = make({ reducedMotion: true });
    const { tick, host } = ticker(layer);
    layer.spawn({ category: 'impact', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    tick(0); // draw the flash, schedule expiry, and stop
    expect(host.renders).toBe(1);
    expect(host.stops).toBe(1);
    expect(host.wakeDelays).toEqual([EFFECT_TIMING.reducedHoldMs]);

    tick(EFFECT_TIMING.reducedHoldMs + 1); // simulate the scheduled retirement wake
    expect(host.renders).toBe(2);
    expect(host.stops).toBe(2);
    expect(host.wakeDelays).toEqual([EFFECT_TIMING.reducedHoldMs]);
  });

  it('runs a live crawl continuously and never stops it', () => {
    const { layer } = make();
    const { tick, host } = ticker(layer);
    layer.setPersistent([
      {
        id: 'path:a1',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'blk' },
        accent: SURFACES.targeting,
      },
    ]);
    for (let frame = 0; frame < 5; frame += 1) tick(frame * 16);
    expect(host.renders).toBe(5);
    expect(host.stops).toBe(0);
    expect(host.wakeDelays).toEqual([]);
  });

  it('renders an empty frame when clear removes already-presented pixels', () => {
    const { layer } = make();
    const { tick, host } = ticker(layer);
    layer.setPersistent([
      {
        id: 'link:blk',
        category: 'blocker-link',
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
    ]);
    tick(0);
    let wakes = 0;
    layer.wake = () => {
      wakes += 1;
    };

    layer.clear();
    expect(wakes).toBe(1);
    expect(layer.needsFrame()).toBe(true);
    tick(16);
    expect(layer.lastProgram).toEqual([]);
    expect(host.renders).toBe(2);
    expect(host.stops).toBe(2);
  });
});

describe('EffectsLayer endpoint tracking and reduced motion', () => {
  it('redraws from current rects while reconciler motion is in flight', () => {
    const { layer, rects } = make();
    layer.setPersistent([
      {
        id: 'link:blk',
        category: 'blocker-link',
        from: { ref: 'blk' },
        to: { ref: 'atk' },
        accent: COMBAT_LINK.color,
      },
    ]);
    layer.advance(0);
    const before = layer.lastProgram;

    layer.trackMotion(true);
    rects.set('atk', { x: 200, y: 380, w: 66, h: 92 }); // mid-tween position
    expect(layer.advance(16)).toBe(true);
    expect(layer.lastProgram).not.toEqual(before);

    const midTween = layer.lastProgram;
    rects.set('atk', { x: 240, y: 380, w: 66, h: 92 }); // final position between ticks
    layer.trackMotion(false);
    expect(layer.needsFrame()).toBe(true);
    expect(layer.advance(32)).toBe(true);
    expect(layer.lastProgram).not.toEqual(midTween);
    expect(layer.advance(48)).toBe(false); // back to render-on-demand
  });

  it('renders static forms under reduced motion with zero per-frame work', () => {
    const { layer } = make({ reducedMotion: true });
    layer.setPersistent([
      {
        id: 'path:a1',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'blk' },
        accent: SURFACES.targeting,
      },
    ]);
    expect(layer.advance(0)).toBe(true);
    const staticFrame = layer.lastProgram;
    // No dash crawl: later frames neither draw nor differ.
    expect(layer.advance(300)).toBe(false);
    expect(layer.lastProgram).toEqual(staticFrame);
    // Tracking never engages under reduced motion (the reconciler snaps).
    layer.trackMotion(true);
    expect(layer.advance(600)).toBe(false);
  });

  it('holds a one-frame flash for a reduced-motion transient, then retires', () => {
    const { layer } = make({ reducedMotion: true });
    layer.spawn({ category: 'impact', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    expect(layer.advance(0)).toBe(true); // the static flash frame
    expect(layer.advance(100)).toBe(false); // held — zero work
    expect(layer.advance(EFFECT_TIMING.reducedHoldMs + 1)).toBe(true); // retire redraw
    expect(layer.hasLiveEffects()).toBe(false);
  });
});
