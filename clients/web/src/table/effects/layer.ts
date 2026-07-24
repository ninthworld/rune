import { Container, Graphics } from 'pixi.js';
import { COMBAT_LINK } from '../../tokens';
import type { Rect } from '../scene';
import { doubledStroke } from '../combatLinks';
import {
  anchorCenter,
  arrowHead,
  burstParticles,
  dashSegments,
  pathCurve,
  rectCenter,
  type Point,
} from './geometry';
import {
  DENSITY_SCALE,
  PARTICLE_CAP,
  type DrawOp,
  type EffectDensity,
  type EffectQuality,
  type PersistentEffect,
  type TransientInvocation,
} from './types';

/**
 * Effect timing, inside the visual-system §8 / budget caps (resolution and
 * impact effects ≤ 600 ms; the unit test pins this). `reducedHoldMs` is the
 * reduced-motion form: one static frame held briefly, zero per-frame work.
 */
export const EFFECT_TIMING = {
  impactMs: 450,
  resolutionMs: 500,
  reducedHoldMs: 200,
  /** Dash-crawl period of a pending path (full dash+gap cycle). */
  dashPeriodMs: 900,
  dashLen: 12,
  dashGap: 9,
} as const;

/** Base particle count of a magnitude-1 impact burst at full density. */
const BURST_BASE = 24;

/** Options for {@link EffectsLayer}. */
export interface EffectsLayerOptions {
  /** Quality level — sizes the particle pool (never degrades the scene). */
  quality: EffectQuality;
  /** The effect-density control, independent of quality. */
  density: EffectDensity;
  /** Reduced motion: static dash, static pulses, one-frame flash forms. */
  reducedMotion: boolean;
  /**
   * The rect source live anchors resolve through at draw time — the reconciler
   * side supplies *current* rects, so endpoints track in-flight motion (the
   * carried combat-link behavior); under reduced motion the reconciler snaps
   * and everything renders static at final positions.
   */
  rects: (ref: string) => Rect | undefined;
}

/** One live transient effect and its anchored clock. */
interface LiveTransient {
  invocation: TransientInvocation;
  /** Particles this burst holds from the pool (0 at Lite / minimal density). */
  particleCount: number;
  duration: number;
  start?: number;
}

/**
 * The passive WebGL effects layer (issue #482, ADR 0030 layer 3): paths,
 * bursts, glows — never cards, never a hit target. Pixi's refocused role.
 *
 * **Zero idle cost by construction** (the spike's decisive finding): the layer
 * draws only when something changed (`dirty`), something is animating, or a
 * transient expired — {@link advance} is a cheap no-op otherwise, and
 * {@link EffectsLayer.stats}.draws proves it. Rendering builds a deterministic
 * **draw program** (the ADR 0011 structural-snapshot unit, GPU-free to assert)
 * and strokes it into one pooled {@link Graphics} — sustained effects never
 * touch a full-viewport 2D canvas (the measured ~8–9 fps disqualification).
 *
 * Effects only decorate state the view already applied: nothing
 * gameplay-visible depends on one having played, and the layer computes no
 * legality — every invocation is a category plus parameters the caller
 * already has (source/target anchors, accent, magnitude), never bespoke per
 * card.
 */
export class EffectsLayer {
  /** The layer's display root: one Graphics, `eventMode: 'none'` — passive. */
  readonly root: Container;

  /** Observable behavior for the budget tests: total draws and live particles. */
  readonly stats = { draws: 0, liveParticles: 0 };

  /** Called when the layer gains work — the mount restarts its ticker on this. */
  wake?: () => void;

  private readonly graphics = new Graphics();
  private readonly options: EffectsLayerOptions;
  private persistent: PersistentEffect[] = [];
  private persistentKey = '[]';
  private transients: LiveTransient[] = [];
  private tracking = false;
  private dirty = false;
  /** The last built draw program — the structural snapshot tests assert. */
  lastProgram: DrawOp[] = [];

  constructor(options: EffectsLayerOptions) {
    this.options = options;
    this.root = new Container();
    // Passive by contract: never a hit target, never intercepts input.
    this.root.eventMode = 'none';
    this.graphics.eventMode = 'none';
    this.root.addChild(this.graphics);
  }

  /** Spawn one transient effect (impact burst, resolution pulse). */
  spawn(invocation: TransientInvocation): void {
    const { reducedMotion, quality, density } = this.options;
    const duration = reducedMotion
      ? EFFECT_TIMING.reducedHoldMs
      : invocation.category === 'impact'
        ? EFFECT_TIMING.impactMs
        : EFFECT_TIMING.resolutionMs;
    // Particle budget: Lite and reduced motion render pulses/flashes only;
    // density scales the count; the pooled cap for the quality level is never
    // exceeded across live bursts.
    const wanted =
      quality === 'lite' || reducedMotion || invocation.category !== 'impact'
        ? 0
        : Math.round(BURST_BASE * (invocation.magnitude ?? 1) * DENSITY_SCALE[density]);
    const free = PARTICLE_CAP[quality] - this.stats.liveParticles;
    const particleCount = Math.max(0, Math.min(wanted, free));
    this.stats.liveParticles += particleCount;
    this.transients.push({ invocation, particleCount, duration });
    this.dirty = true;
    this.wake?.();
  }

  /**
   * Declare the current persistent set (pending targeting/attack paths and
   * blocker links). Reconciled by value: an unchanged set costs nothing.
   */
  setPersistent(effects: PersistentEffect[]): void {
    const key = JSON.stringify(effects);
    if (key === this.persistentKey) return;
    this.persistent = effects;
    this.persistentKey = key;
    this.dirty = true;
    this.wake?.();
  }

  /**
   * Endpoint tracking (carried from the shipped combat-link overlay): while
   * reconciler motion is in flight the caller turns tracking on and the layer
   * redraws each advance from the *current* rects; off again, it goes back to
   * render-on-demand. Reduced motion never tracks — the reconciler snaps.
   */
  trackMotion(moving: boolean): void {
    const track = moving && !this.options.reducedMotion;
    if (track === this.tracking) return;
    this.tracking = track;
    // Redraw on both edges. In particular, the final reconciler rect may land
    // between animation frames, so turning tracking off must commit it once.
    this.dirty = true;
    this.wake?.();
  }

  /** Whether anything is visible/live (persistent effects included, even when
   * their geometry is static and costs no further frames). */
  hasLiveEffects(): boolean {
    return this.transients.length > 0 || this.persistent.length > 0 || this.tracking;
  }

  /**
   * Whether another animation frame is needed — distinct from
   * {@link hasLiveEffects}: a drawn static blocker link or reduced-motion path
   * is live but needs nothing, so the mount's ticker may stop immediately.
   * True while something is undrawn (`dirty`) or a per-frame animation (dash
   * crawl, tracking, or a full-motion transient) is running. Static transient
   * expiry is scheduled separately through {@link nextWakeIn}.
   */
  needsFrame(): boolean {
    return this.dirty || this.isAnimating();
  }

  /**
   * Milliseconds until the next transient expires. The mount uses this only
   * when {@link needsFrame} is false, letting a reduced-motion flash sleep
   * between its single visible frame and its retirement frame.
   */
  nextWakeIn(now: number): number | undefined {
    let delay: number | undefined;
    for (const transient of this.transients) {
      const start = transient.start ?? now;
      const remaining = Math.max(0, transient.duration - (now - start));
      delay = delay === undefined ? remaining : Math.min(delay, remaining);
    }
    return delay;
  }

  /**
   * Advance to `now` (monotonic ms) and draw **only if needed**: something
   * changed, a transient expired, endpoints are tracking, or a live animation
   * (dash crawl, burst, pulse) needs its next frame. Returns whether a draw
   * happened — false is the idle path and costs nothing.
   */
  advance(now: number): boolean {
    let expired = false;
    for (const transient of this.transients) {
      if (transient.start === undefined) transient.start = now;
      if (now - transient.start >= transient.duration) expired = true;
    }
    if (expired) {
      this.transients = this.transients.filter((t) => {
        const done = t.start !== undefined && now - t.start >= t.duration;
        if (done) this.stats.liveParticles -= t.particleCount;
        return !done;
      });
    }
    if (!this.dirty && !expired && !this.isAnimating()) return false;
    this.lastProgram = this.buildProgram(now);
    drawProgram(this.graphics, this.lastProgram);
    this.stats.draws += 1;
    this.dirty = false;
    return true;
  }

  /** Whether a per-frame animation is live (never under reduced motion). */
  private isAnimating(): boolean {
    if (this.tracking) return true;
    if (this.options.reducedMotion) return false;
    if (this.transients.length > 0) return true;
    // A pending path dash-crawls; a blocker link alone is static.
    return this.persistent.some((e) => e.category !== 'blocker-link');
  }

  /** Build the draw program for time `now` — pure data, snapshot-testable. */
  private buildProgram(now: number): DrawOp[] {
    const { rects, reducedMotion } = this.options;
    const ops: DrawOp[] = [];
    const retained: PersistentEffect[] = [];

    for (const effect of this.persistent) {
      const from = anchorCenter(effect.from, rects);
      const to = anchorCenter(effect.to, rects);
      // An unresolvable endpoint RETIRES the effect — never a stale line, and
      // never an idle leak (a dropped-but-live path would otherwise keep
      // "animating" an empty program forever). It comes back only through a
      // new authoritative setPersistent.
      if (!from || !to) continue;
      retained.push(effect);
      if (effect.category === 'blocker-link') {
        for (const [a, b] of doubledStroke(from, to)) {
          ops.push(segment(effect.category, a, b, effect.accent, COMBAT_LINK.strokeWidth));
        }
        ops.push({
          op: 'circle',
          category: effect.category,
          x: from.x,
          y: from.y,
          r: COMBAT_LINK.nodeRadius,
          color: effect.accent,
          alpha: COMBAT_LINK.alpha,
          fill: true,
        });
        continue;
      }
      // Targeting / attack path: lifted bezier, dash-crawling while pending
      // (static dashes under reduced motion), arrowhead at the terminus.
      const curve = pathCurve(from, to);
      const phase = reducedMotion
        ? 0
        : ((now % EFFECT_TIMING.dashPeriodMs) / EFFECT_TIMING.dashPeriodMs) *
          (EFFECT_TIMING.dashLen + EFFECT_TIMING.dashGap);
      for (const [a, b] of dashSegments(
        curve,
        EFFECT_TIMING.dashLen,
        EFFECT_TIMING.dashGap,
        phase,
      )) {
        ops.push(segment(effect.category, a, b, effect.accent, 3));
      }
      for (const [a, b] of arrowHead(curve)) {
        ops.push(segment(effect.category, a, b, effect.accent, 3));
      }
    }
    if (retained.length !== this.persistent.length) {
      this.persistent = retained;
      this.persistentKey = JSON.stringify(retained);
    }

    for (const transient of this.transients) {
      const center = anchorCenter(transient.invocation.target, rects);
      if (!center) continue;
      const t = reducedMotion
        ? 1
        : transient.start === undefined
          ? 0
          : Math.min(1, (now - transient.start) / transient.duration);
      this.transientOps(ops, transient, center, reducedMotion ? 0.35 : t);
    }
    return ops;
  }

  /** The ops of one live transient at eased progress `t`. */
  private transientOps(ops: DrawOp[], transient: LiveTransient, center: Point, t: number): void {
    const { invocation } = transient;
    const magnitude = invocation.magnitude ?? 1;
    const ease = 1 - (1 - t) * (1 - t);
    const alpha = Math.max(0.05, 1 - t);
    // The category's default pulse ring — also the whole Lite/reduced-motion form.
    ops.push({
      op: 'circle',
      category: invocation.category,
      x: center.x,
      y: center.y,
      r: 10 + ease * 26 * magnitude,
      color: invocation.accent,
      alpha,
      fill: false,
    });
    for (const particle of burstParticles(transient.particleCount)) {
      const distance = particle.speed * ease * magnitude;
      ops.push({
        op: 'circle',
        category: invocation.category,
        x: center.x + Math.cos(particle.angle) * distance,
        y: center.y + Math.sin(particle.angle) * distance,
        r: particle.size * (1 - t * 0.6),
        color: invocation.accent,
        alpha,
        fill: true,
      });
    }
  }

  /** Drop every live effect and render the cleared surface once. */
  clear(): void {
    this.transients = [];
    this.persistent = [];
    this.persistentKey = '[]';
    this.tracking = false;
    this.stats.liveParticles = 0;
    this.lastProgram = [];
    this.graphics.clear();
    // Pixi is render-on-demand: clearing Graphics alone does not update the
    // canvas, so wake one empty draw to remove any pixels already presented.
    this.dirty = true;
    this.wake?.();
  }
}

/** A stroked segment op. */
function segment(category: string, from: Point, to: Point, color: string, width: number): DrawOp {
  return { op: 'segment', category, from, to, color, width, alpha: 0.9 };
}

/** `'#RRGGBB'` token to the numeric color Pixi expects. */
function hexColor(hex: string): number {
  return parseInt(hex.replace('#', ''), 16);
}

/**
 * The render-on-demand tick the effects mount drives its Pixi ticker with,
 * extracted so the stop policy is testable without a GL context: advance the
 * layer, render only when it drew, and **stop the ticker the moment no further
 * frame is needed** — a drawn static blocker link or reduced-motion form is
 * live but costs nothing (`needsFrame`, not `hasLiveEffects`, is the stop
 * gate). Static transient expiry gets one scheduled wake instead of polling
 * every Pixi frame. Every layer mutation calls `wake`, so stopping never
 * strands work.
 */
export function createEffectsTicker(
  layer: EffectsLayer,
  host: { render(): void; scheduleWake(delayMs: number): void; stop(): void },
): (now: number) => void {
  return (now: number) => {
    const drew = layer.advance(now);
    if (drew) host.render();
    if (!layer.needsFrame()) {
      const delay = layer.nextWakeIn(now);
      if (delay !== undefined) host.scheduleWake(delay);
      host.stop();
    }
  };
}

/**
 * Execute a draw program into one pooled Graphics (clear + stroke). This is
 * the entire GPU seam: everything above it is pure data.
 */
export function drawProgram(graphics: Graphics, program: DrawOp[]): void {
  graphics.clear();
  for (const op of program) {
    if (op.op === 'segment') {
      graphics.lineStyle({ width: op.width, color: hexColor(op.color), alpha: op.alpha });
      graphics.moveTo(op.from.x, op.from.y);
      graphics.lineTo(op.to.x, op.to.y);
    } else if (op.fill) {
      graphics.lineStyle(0);
      graphics.beginFill(hexColor(op.color), op.alpha);
      graphics.drawCircle(op.x, op.y, op.r);
      graphics.endFill();
    } else {
      graphics.lineStyle({ width: 2, color: hexColor(op.color), alpha: op.alpha });
      graphics.drawCircle(op.x, op.y, op.r);
    }
  }
}

/** Re-export for consumers building anchors from rects. */
export { rectCenter };
