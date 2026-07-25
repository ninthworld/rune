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
import { SCENE_HUES, SCENE_NEUTRALS, SCENE_RELATIONSHIP, SCENE_SEAT_ACCENTS } from '../sceneTokens';
import {
  EFFECT_TIMING,
  EffectsLayer,
  PARTICLE_CAP,
  TRANSIENT_CAP,
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

describe('EffectsLayer off-focus crest ping (issue #501)', () => {
  /** A layer whose one anchor is an off-focus seat's crest cluster. */
  function crest(opts: { reducedMotion?: boolean; quality?: EffectQuality } = {}) {
    return make({
      ...opts,
      rects: new Map<string, Rect>([['seat:p3', { x: 300, y: 40, w: 52, h: 52 }]]),
    });
  }

  it('draws a quiet rune ping: a ring plus its shape-channel spokes', () => {
    const { layer } = crest();
    layer.spawn({
      category: 'off-focus-ping',
      target: { ref: 'seat:p3' },
      accent: SCENE_SEAT_ACCENTS[2]!,
    });
    layer.advance(0);

    const rings = layer.lastProgram.filter((op) => op.op === 'circle');
    const spokes = layer.lastProgram.filter((op) => op.op === 'segment');
    expect(rings).toHaveLength(1);
    expect(rings[0]!.op === 'circle' && rings[0]!.fill).toBe(false);
    // Never color-only (visual-system §7): the mark carries its own shape.
    expect(spokes.length).toBeGreaterThan(1);
    for (const op of layer.lastProgram) {
      expect(op.category).toBe('off-focus-ping');
      expect(op.color).toBe(SCENE_SEAT_ACCENTS[2]);
    }
    // Its own row of the motion grammar: ≤300 ms, inside the batch window.
    expect(EFFECT_TIMING.offFocusPingMs).toBeLessThanOrEqual(300);
    expect(layer.advance(EFFECT_TIMING.offFocusPingMs + 1)).toBe(true);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('holds the static ping badge for at least a second under reduced motion', () => {
    const { layer } = crest({ reducedMotion: true });
    layer.spawn({
      category: 'off-focus-ping',
      target: { ref: 'seat:p3' },
      accent: SCENE_SEAT_ACCENTS[2]!,
    });
    expect(EFFECT_TIMING.offFocusHoldMs).toBeGreaterThanOrEqual(1000);

    expect(layer.advance(0)).toBe(true);
    const badge = layer.lastProgram;
    // A badge, not a pulse: the filled center distinguishes it, and it costs
    // nothing per frame across the whole hold.
    expect(badge.some((op) => op.op === 'circle' && op.fill)).toBe(true);
    expect(layer.advance(999)).toBe(false);
    expect(layer.lastProgram).toEqual(badge);
    expect(layer.advance(EFFECT_TIMING.offFocusHoldMs + 1)).toBe(true);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('spawns no particles at Lite or minimal density (pulse-only vocabulary)', () => {
    for (const options of [{ quality: 'lite' as const }, { quality: 'high' as const }]) {
      const { layer } = crest(options);
      layer.spawn({
        category: 'off-focus-ping',
        target: { ref: 'seat:p3' },
        accent: SCENE_SEAT_ACCENTS[2]!,
      });
      layer.advance(0);
      expect(layer.stats.liveParticles).toBe(0);
      expect(layer.lastProgram.filter((op) => op.op === 'circle' && op.fill)).toHaveLength(0);
    }
  });
});

/**
 * The zero-idle contract at the layer level, restated for the relationship
 * grammar (`stack-and-relationships.md` §8.4, implementation note IN1). Before
 * this, every non-`blocker-link` persistent effect was treated as animating, so
 * a declared attacker or anything sitting on the stack would have spun the
 * ticker forever. Only the two genuinely moving states may do that.
 */
describe('EffectsLayer zero idle cost with the relationship grammar (IN1)', () => {
  const persistent = (over: Partial<import('./effects').PersistentEffect>) => ({
    id: 'r',
    category: 'targeting-path' as const,
    from: { ref: 'atk' },
    to: { ref: 'blk' },
    accent: SURFACES.targeting,
    ...over,
  });

  it('costs nothing per frame once a CONFIRMED attack path has drawn', () => {
    const { layer } = make();
    layer.setPersistent([
      persistent({ category: 'attack-path', state: 'confirmed', endpoint: 'player' }),
    ]);
    expect(layer.advance(0)).toBe(true);
    expect(layer.lastProgram.length).toBeGreaterThan(3);
    expect(layer.hasLiveEffects()).toBe(true);
    // Live but static: the mount's ticker may stop immediately.
    expect(layer.needsFrame()).toBe(false);
    expect(layer.advance(16)).toBe(false);
    expect(layer.advance(900)).toBe(false);
  });

  it('costs nothing per frame for calmed, endpoint-only, or attachment relationships', () => {
    for (const effect of [
      persistent({ state: 'calmed' }),
      persistent({ state: 'endpoint-only' }),
      persistent({ category: 'attachment-bracket', accent: SCENE_NEUTRALS.text }),
      persistent({ category: 'source-tether', accent: SCENE_NEUTRALS.text }),
      persistent({ state: 'provisional' }),
    ]) {
      const { layer } = make();
      layer.setPersistent([effect]);
      expect(layer.advance(0)).toBe(true);
      expect(layer.needsFrame()).toBe(false);
      expect(layer.advance(16)).toBe(false);
    }
  });

  it('still runs a frame for the two states that genuinely move', () => {
    for (const state of ['pending', 'resolving'] as const) {
      const { layer } = make();
      layer.setPersistent([persistent({ state })]);
      expect(layer.advance(0)).toBe(true);
      expect(layer.needsFrame()).toBe(true);
      expect(layer.advance(16)).toBe(true);
    }
  });

  it('runs the resolving retraction on its own clock, then retires itself', () => {
    const { layer } = make();
    layer.setPersistent([persistent({ state: 'resolving' })]);
    layer.advance(1000);
    const strokes = (): number => layer.lastProgram.filter((op) => op.part === 'path').length;
    const started = strokes();
    expect(started).toBeGreaterThan(0);
    layer.advance(1000 + EFFECT_TIMING.resolveRetractMs / 2);
    expect(strokes()).toBeLessThan(started);
    // §6.2 is a bounded moment, and the layer owns its whole lifetime: a
    // departing relationship must not leave caps standing on the board once the
    // retraction is over, and it must not keep the ticker alive either.
    layer.advance(1000 + EFFECT_TIMING.resolveRetractMs + 1);
    expect(layer.lastProgram).toHaveLength(0);
    expect(layer.hasLiveEffects()).toBe(false);
    expect(layer.needsFrame()).toBe(false);
    // §8.4 — and it stays at zero from there, with no further view.
    expect(layer.advance(1000 + EFFECT_TIMING.resolveRetractMs + 200)).toBe(false);
    // §7.1 — the retraction is inside the ≤600 ms resolution cap.
    expect(EFFECT_TIMING.resolveRetractMs).toBeLessThanOrEqual(600);
  });

  it('drops a retraction the instant a newer view supersedes it (interruptible)', () => {
    // The §7.1 note: the composed sequence is not individually skippable, but it
    // "remains interruptible by a newer authoritative view". Retractions must
    // never queue behind one another or hold the truth back.
    const { layer } = make();
    layer.setPersistent([persistent({ id: 'a', state: 'resolving' })]);
    layer.advance(0);
    expect(layer.lastProgram.length).toBeGreaterThan(0);
    // A newer view mid-retraction: the whole persistent set is replaced, so the
    // half-played retraction is simply gone rather than finishing first.
    layer.setPersistent([persistent({ id: 'b', state: 'confirmed' })]);
    layer.advance(EFFECT_TIMING.resolveRetractMs / 2);
    expect(layer.lastProgram.every((op) => op.part !== undefined)).toBe(true);
    expect(layer.needsFrame()).toBe(false);
    // And the abandoned clock does not survive to shorten a later retraction of
    // the same relationship (a recast, a copy).
    layer.setPersistent([persistent({ id: 'a', state: 'resolving' })]);
    const at = EFFECT_TIMING.resolveRetractMs;
    layer.advance(at);
    const full = layer.lastProgram.filter((op) => op.part === 'path').length;
    layer.advance(at + EFFECT_TIMING.resolveRetractMs / 2);
    expect(layer.lastProgram.filter((op) => op.part === 'path').length).toBeLessThan(full);
    expect(full).toBeGreaterThan(0);
  });

  it('retires a retraction in the same frame under reduced motion (F6)', () => {
    // §7.2's row: "Resolution happened" is carried by the applied state, the log
    // entry, and the 200 ms static ring — never by a retraction nobody asked to
    // watch. The path is absent in the same frame the state applies.
    const { layer } = make({ reducedMotion: true });
    layer.setPersistent([persistent({ state: 'resolving' })]);
    layer.advance(0);
    expect(layer.lastProgram).toHaveLength(0);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('retracts an elbow kind too — the tether of a resolving stack entry', () => {
    // The commonest §6.2 case in a real match is an ability's source tether
    // departing, and the elbow kinds are otherwise static by contract. A
    // static-first animation test would leave that retraction with a clock
    // nothing advanced, i.e. frozen at frame one.
    const { layer } = make();
    const tether = persistent({
      id: 'tether:ability',
      category: 'source-tether',
      accent: SCENE_NEUTRALS.text,
      state: 'resolving',
    });
    layer.setPersistent([tether]);
    layer.advance(0);
    const strokes = (): number => layer.lastProgram.filter((op) => op.part === 'path').length;
    const started = strokes();
    expect(started).toBeGreaterThan(0);
    expect(layer.needsFrame()).toBe(true);
    layer.advance(EFFECT_TIMING.resolveRetractMs / 2);
    expect(strokes()).toBeLessThan(started);
    // The destination terminal holds to the end — the retraction converges on
    // the thing the tether pointed at (§6.2 step 1).
    expect(layer.lastProgram.filter((op) => op.part === 'terminal')).toHaveLength(1);
    layer.advance(EFFECT_TIMING.resolveRetractMs + 1);
    expect(layer.lastProgram).toHaveLength(0);
    expect(layer.needsFrame()).toBe(false);
  });
});

/**
 * §10.3 / implementation note IN2 — an unresolvable endpoint has THREE outcomes,
 * not two. The distinction comes from the caller: an endpoint that is gone
 * retires, and an endpoint the caller says is merely occluded clamps to its
 * container edge and grows an indicator.
 */
describe('EffectsLayer occluded endpoints (§10.3, IN2)', () => {
  it('clamps an occluded endpoint to its container edge and draws an indicator', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'path:occluded',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'behind-the-sheet' },
        accent: SURFACES.targeting,
        state: 'confirmed',
        edge: { x: 600, y: 0, w: 200, h: 400 },
      },
    ]);
    expect(layer.advance(0)).toBe(true);
    // The relationship is NOT lost: it terminates visibly at the rail's edge.
    expect(layer.lastProgram.filter((op) => op.part === 'edge')).toHaveLength(2);
    expect(layer.lastProgram.some((op) => op.part === 'path')).toBe(true);
    expect(layer.hasLiveEffects()).toBe(true);
  });

  it('clamps a CLIPPED endpoint whose rect resolved outside its container', () => {
    // The second §10.3 shape: the endpoint is drawn, but off the viewport (a
    // deep stack marches its slots off the top). Both rects resolve, so the
    // container is what says which end left the screen.
    const rects = new Map<string, Rect>([
      ['atk', { x: 100, y: 400, w: 66, h: 92 }],
      ['far', { x: 300, y: -900, w: 48, h: 68 }],
    ]);
    const { layer } = make({ rects });
    layer.setPersistent([
      {
        id: 'path:clipped',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'far' },
        accent: SURFACES.targeting,
        state: 'confirmed',
        edge: { x: 0, y: 0, w: 1280, h: 720 },
      },
    ]);
    expect(layer.advance(0)).toBe(true);
    expect(layer.lastProgram.filter((op) => op.part === 'edge')).toHaveLength(2);
    // The path terminates on the viewport edge, not at the off-screen rect.
    for (const op of layer.lastProgram) {
      if (op.op === 'segment') expect(op.to.y).toBeGreaterThanOrEqual(-1);
    }
  });

  it('leaves a relationship alone when both endpoints are inside the container', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'path:onscreen',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'blk' },
        accent: SURFACES.targeting,
        state: 'confirmed',
        edge: { x: 0, y: 0, w: 1280, h: 720 },
      },
    ]);
    layer.advance(0);
    expect(layer.lastProgram.filter((op) => op.part === 'edge')).toHaveLength(0);
    expect(layer.lastProgram.some((op) => op.part === 'cap')).toBe(true);
  });

  it('still RETIRES an endpoint that is simply gone (the carried behaviour)', () => {
    const { layer } = make();
    layer.setPersistent([
      {
        id: 'path:gone',
        category: 'targeting-path',
        from: { ref: 'atk' },
        to: { ref: 'gone' },
        accent: SURFACES.targeting,
        state: 'confirmed',
      },
    ]);
    layer.advance(0);
    expect(layer.lastProgram).toHaveLength(0);
    expect(layer.hasLiveEffects()).toBe(false);
  });
});

/**
 * The §6.3 terminal forms. The fizzle rule is normative and load-bearing: a
 * countered spell's terminal lands on the stack object, and the released target
 * gets an **opening** ring with no burst — "nothing happened to me" has to be a
 * visible event rather than the absence of one (decision D14).
 */
describe('EffectsLayer terminal forms (§6.3)', () => {
  it('gives the counter/fizzle release an opening ring and NO burst', () => {
    const { layer } = make({ quality: 'high', density: 'full' });
    layer.spawn({ category: 'counter', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    layer.advance(0);
    const early = layer.lastProgram[0]!;
    expect(layer.lastProgram).toHaveLength(1);
    expect(early.op === 'circle' && early.fill).toBe(false);
    expect(layer.stats.liveParticles).toBe(0);
    // It starts AT the reticle it is releasing — this is the target's own ring
    // opening, not a new ring blooming from nothing.
    expect(early.op === 'circle' && early.r).toBe(SCENE_RELATIONSHIP.reticleRadius);

    layer.advance(EFFECT_TIMING.resolutionMs * 0.8);
    const late = layer.lastProgram[0]!;
    expect(late.op === 'circle' && early.op === 'circle' && late.r > early.r).toBe(true);
    expect(late.alpha).toBeLessThan(early.alpha);
    // And it *opens* — a few px — rather than bursting outward the way an
    // impact ring does. "Nothing happened to me" is a quiet, distinct event.
    layer.advance(EFFECT_TIMING.resolutionMs - 1);
    const end = layer.lastProgram[0]!;
    expect(end.op === 'circle' && early.op === 'circle' && end.r - early.r).toBeLessThan(10);

    const impact = make({ quality: 'lite' }).layer;
    impact.spawn({ category: 'impact', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    impact.advance(0);
    impact.advance(EFFECT_TIMING.impactMs - 1);
    const burst = impact.lastProgram[0]!;
    expect(burst.op === 'circle' && end.op === 'circle' && burst.r).toBeGreaterThan(
      end.op === 'circle' ? end.r : 0,
    );
  });

  it('keeps the impact burst distinct: many filled particles, growing ring', () => {
    const { layer } = make({ quality: 'high', density: 'full' });
    layer.spawn({ category: 'damage', target: { ref: 'atk' }, accent: SCENE_HUES.red.value });
    layer.advance(0);
    expect(layer.stats.liveParticles).toBeGreaterThan(0);
    expect(layer.lastProgram.filter((op) => op.op === 'circle' && op.fill).length).toBeGreaterThan(
      1,
    );
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

describe('EffectsLayer authoritative transient batches', () => {
  it('replaces an in-flight batch when a newer view arrives', () => {
    const { layer } = make();
    layer.replaceTransients([
      { category: 'damage', target: { ref: 'atk' }, accent: SCENE_HUES.red.value },
      { category: 'death', target: { ref: 'blk' }, accent: SCENE_HUES.red.value },
    ]);
    layer.advance(0);
    expect(layer.hasLiveEffects()).toBe(true);

    layer.replaceTransients([
      { category: 'healing', target: { ref: 'atk' }, accent: SCENE_HUES.green.value },
    ]);
    layer.advance(16);
    expect(new Set(layer.lastProgram.map((op) => op.category))).toEqual(new Set(['healing']));
  });

  it('caps a dense batch to the total presentation window', () => {
    const { layer } = make();
    layer.replaceTransients(
      Array.from({ length: 100 }, () => ({
        category: 'battlefield-entry' as const,
        target: { ref: 'atk' as const },
        accent: SCENE_HUES.green.value,
      })),
    );
    layer.advance(0);
    expect(layer.stats.liveTransients).toBe(TRANSIENT_CAP.high);
    expect(layer.hasLiveEffects()).toBe(true);
    layer.advance(799);
    expect(layer.hasLiveEffects()).toBe(true);
    layer.advance(801);
    expect(layer.hasLiveEffects()).toBe(false);
  });

  it('caps rings and flashes as well as particles at every quality', () => {
    const { layer } = make({ quality: 'lite' });
    layer.replaceTransients(
      Array.from({ length: 100 }, () => ({
        category: 'flow' as const,
        target: { ref: 'atk' as const },
        accent: SCENE_HUES.gold.value,
      })),
    );
    expect(layer.stats.liveParticles).toBe(0);
    expect(layer.stats.liveTransients).toBe(TRANSIENT_CAP.lite);
    layer.advance(0);
    expect(layer.lastProgram).toHaveLength(TRANSIENT_CAP.lite);
  });

  it('snaps the whole batch under reduced motion with no staggered animation', () => {
    const { layer } = make({ reducedMotion: true });
    layer.replaceTransients(
      Array.from({ length: 8 }, () => ({
        category: 'damage' as const,
        target: { ref: 'atk' as const },
        accent: SCENE_HUES.red.value,
      })),
    );
    expect(layer.advance(0)).toBe(true);
    const staticFrame = layer.lastProgram;
    expect(layer.advance(100)).toBe(false);
    expect(layer.lastProgram).toEqual(staticFrame);
    expect(layer.advance(EFFECT_TIMING.reducedHoldMs + 1)).toBe(true);
    expect(layer.hasLiveEffects()).toBe(false);
  });
});
