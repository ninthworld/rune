import { Container, Graphics } from 'pixi.js';
import { SCENE_BATCH, SCENE_RELATIONSHIP } from '../../sceneTokens';
import type { Rect } from '../scene';
import {
  anchorCenter,
  anchorRect,
  burstParticles,
  clampToRect,
  rectCenter,
  type Point,
} from './geometry';
import {
  RELATIONSHIP_DASH,
  edgeIndicatorOps,
  fanGroups,
  relationshipAnimates,
  relationshipOps,
  relationshipState,
  type ResolvedRelationship,
} from './relationships';
import {
  DENSITY_SCALE,
  PARTICLE_CAP,
  TRANSIENT_CAP,
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
  /** The off-focus crest ping (visual-system §8, "Off-focus activity": ≤300 ms). */
  offFocusPingMs: 300,
  /** Its reduced-motion form: the static ping badge, held ≥1 s (same row). */
  offFocusHoldMs: 1000,
  /** Dash-crawl period of a pending path (full dash+gap cycle). */
  dashPeriodMs: 900,
  dashLen: RELATIONSHIP_DASH.len,
  dashGap: RELATIONSHIP_DASH.gap,
  /**
   * The §6.2 resolution retraction (storyboard F6): the path retracts from the
   * source toward the destination over 300 ms, inside the ≤ 600 ms resolution
   * cap. Under reduced motion the path is simply absent instead.
   */
  resolveRetractMs: 300,
} as const;

/** Base particle count of a magnitude-1 impact burst at full density. */
const BURST_BASE = 24;

/**
 * The off-focus rune ping's geometry: a ring plus evenly spaced radial spokes.
 * The spokes are the cue's **shape channel** — no state is color-only at any
 * quality level (visual-system §7) — and they are strokes, never particles, so
 * the ping stays inside Lite's pulse-and-flash vocabulary.
 */
const PING = { radius: 15, spokes: 4, spokeInner: 0.55, spokeOuter: 1.2, badgeRadius: 4 } as const;

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
  /** Simultaneous-batch delay, inside the ≤800 ms total window. */
  delay: number;
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
  readonly stats = { draws: 0, liveParticles: 0, liveTransients: 0 };

  /** Called when the layer gains work — the mount restarts its ticker on this. */
  wake?: () => void;

  private readonly graphics = new Graphics();
  private readonly options: EffectsLayerOptions;
  private persistent: PersistentEffect[] = [];
  private persistentKey = '[]';
  private transients: LiveTransient[] = [];
  /**
   * When each `resolving` path first drew — the only clock a persistent effect
   * owns. The retraction of §6.2 decorates a state change the authoritative view
   * has *already* applied (I6), so it is stamped on first sight rather than
   * scheduled, and it never gates input.
   */
  private resolveStarts = new Map<string, number>();
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
    this.spawnOne(invocation, 0);
  }

  /**
   * Replace the transient sequence for a newer authoritative view. This is the
   * fast-forward/interrupt contract: presentation never buffers old views.
   * Simultaneous invocations stagger inside the normative ≤800 ms window;
   * overflow items share the last delay and land together.
   */
  replaceTransients(invocations: readonly TransientInvocation[]): void {
    this.transients = [];
    this.stats.liveParticles = 0;
    this.stats.liveTransients = 0;
    const capped = invocations.slice(0, TRANSIENT_CAP[this.options.quality]);
    for (let index = 0; index < capped.length; index += 1) {
      const invocation = capped[index]!;
      const duration = this.durationFor(invocation);
      const delay =
        this.options.reducedMotion || this.options.quality === 'lite'
          ? 0
          : Math.min(index * SCENE_BATCH.staggerMs, Math.max(0, SCENE_BATCH.windowMs - duration));
      this.spawnOne(invocation, delay);
    }
    // Even an empty replacement must erase pixels from an interrupted batch.
    this.dirty = true;
    this.wake?.();
  }

  private durationFor(invocation: TransientInvocation): number {
    const { reducedMotion } = this.options;
    // The off-focus ping keeps its own row of the motion grammar: a ≤300 ms
    // pulse, and a static badge held ≥1 s instead under reduced motion — the
    // one reduced-motion form that outlives the generic flash.
    if (invocation.category === 'off-focus-ping') {
      return reducedMotion ? EFFECT_TIMING.offFocusHoldMs : EFFECT_TIMING.offFocusPingMs;
    }
    return reducedMotion
      ? EFFECT_TIMING.reducedHoldMs
      : invocation.category === 'impact' || invocation.category === 'damage'
        ? EFFECT_TIMING.impactMs
        : EFFECT_TIMING.resolutionMs;
  }

  private spawnOne(invocation: TransientInvocation, delay: number): void {
    const { reducedMotion, quality, density } = this.options;
    if (this.transients.length >= TRANSIENT_CAP[quality]) return;
    const duration = this.durationFor(invocation);
    // Particle budget: Lite and reduced motion render pulses/flashes only;
    // density scales the count; the pooled cap for the quality level is never
    // exceeded across live bursts.
    const wanted =
      quality === 'lite' ||
      reducedMotion ||
      (invocation.category !== 'impact' &&
        invocation.category !== 'damage' &&
        invocation.category !== 'death' &&
        invocation.category !== 'battlefield-entry')
        ? 0
        : Math.round(BURST_BASE * (invocation.magnitude ?? 1) * DENSITY_SCALE[density]);
    const free = PARTICLE_CAP[quality] - this.stats.liveParticles;
    const particleCount = Math.max(0, Math.min(wanted, free));
    this.stats.liveParticles += particleCount;
    this.transients.push({ invocation, particleCount, duration, delay });
    this.stats.liveTransients += 1;
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
    // Drop retraction clocks whose effect is gone, so a relationship that
    // resolves twice (a copy, a recast) restarts rather than resuming.
    const ids = new Set(effects.map((effect) => effect.id));
    for (const id of [...this.resolveStarts.keys()]) {
      if (!ids.has(id)) this.resolveStarts.delete(id);
    }
    this.dirty = true;
    this.wake?.();
  }

  /**
   * The §6.2 retraction's progress, stamped on the frame the path first drew.
   *
   * Under reduced motion it is complete on sight: F6's reduced form is "path
   * removed in the same frame the state applies", and the fact is carried by
   * the applied state, the log entry, and the ≤200 ms static impact ring
   * instead (§7.2). That makes the retraction a *presentation* clock only —
   * nothing waits on it, and skipping it costs no information.
   */
  private resolveProgress(id: string, now: number): number {
    if (this.options.reducedMotion) return 1;
    const start = this.resolveStarts.get(id);
    if (start === undefined) {
      this.resolveStarts.set(id, now);
      return 0;
    }
    return Math.min(1, (now - start) / EFFECT_TIMING.resolveRetractMs);
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
      const remaining = Math.max(0, transient.delay + transient.duration - (now - start));
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
      if (now - transient.start >= transient.delay + transient.duration) expired = true;
    }
    if (expired) {
      this.transients = this.transients.filter((t) => {
        const done = t.start !== undefined && now - t.start >= t.delay + t.duration;
        if (done) this.stats.liveParticles -= t.particleCount;
        if (done) this.stats.liveTransients -= 1;
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

  /**
   * Whether a per-frame animation is live (never under reduced motion).
   *
   * The zero-idle contract (`stack-and-relationships.md` §8.4, implementation
   * note IN1) turns on this predicate: only a **pending** path (which
   * dash-crawls) and a **resolving** path (which retracts) need another frame.
   * Confirmed and calmed solid paths, endpoint-only caps, blocker links, and
   * attachment brackets are static geometry — treating them as animating would
   * spin the ticker forever whenever anything sat on the stack or in combat.
   */
  private isAnimating(): boolean {
    if (this.tracking) return true;
    if (this.options.reducedMotion) return false;
    if (this.transients.length > 0) return true;
    return this.persistent.some((effect) => relationshipAnimates(effect));
  }

  /**
   * Which endpoint of a relationship is **clipped** out of its declared §10.3
   * container, if either — the second shape of an undrawable endpoint, and the
   * one where both rects still resolve.
   *
   * The container the caller declares in `edge` is, by contract, one that holds
   * the endpoint that is still visible (the staging adapter uses the viewport
   * or the rail for exactly that reason), so "does not intersect `edge`" picks
   * out the clipped end unambiguously. With both ends outside there is no
   * on-screen anchor to point the indicator away from, so neither is reported
   * and the relationship retires as before.
   */
  private offContainer(effect: PersistentEffect, from: Rect, to: Rect): 'from' | 'to' | undefined {
    const edge = effect.edge;
    if (edge === undefined) return undefined;
    const inside = (rect: Rect): boolean =>
      rect.x <= edge.x + edge.w &&
      rect.x + rect.w >= edge.x &&
      rect.y <= edge.y + edge.h &&
      rect.y + rect.h >= edge.y;
    const fromInside = inside(from);
    const toInside = inside(to);
    if (fromInside === toInside) return undefined;
    return fromInside ? 'to' : 'from';
  }

  /** Build the draw program for time `now` — pure data, snapshot-testable. */
  private buildProgram(now: number): DrawOp[] {
    const { rects, reducedMotion } = this.options;
    const ops: DrawOp[] = [];
    const retained: PersistentEffect[] = [];

    const phase = reducedMotion
      ? 0
      : ((now % EFFECT_TIMING.dashPeriodMs) / EFFECT_TIMING.dashPeriodMs) *
        (EFFECT_TIMING.dashLen + EFFECT_TIMING.dashGap);

    // Pass 1 — resolve both endpoints of every relationship. An endpoint the
    // layer cannot draw at has THREE outcomes (implementation note IN2), not
    // two: clamped to its declared container edge when the caller stated the
    // endpoint exists but is occluded (§10.3), and otherwise RETIRED — never a
    // stale line, and never an idle leak, since a dropped-but-live path would
    // keep "animating" an empty program forever. A retired effect returns only
    // through a new authoritative setPersistent.
    //
    // "Cannot draw at" covers both §10.3 shapes: an endpoint with no rect at
    // all (undrawn — scrolled out of the stack rail, behind the compact sheet)
    // and an endpoint whose rect lies wholly outside the declared container
    // (clipped — off the viewport). The second is why `edge` is read even when
    // both rects resolve.
    interface Live {
      effect: PersistentEffect;
      from: Rect;
      to: Rect;
      /** The §6.2 retraction's progress, when this relationship is resolving. */
      progress?: number;
      /** The clamp point of an occluded endpoint, when §10.3 applied. */
      indicator?: Point;
      /** The resolvable endpoint's center — the indicator's tangent origin. */
      indicatorFrom?: Point;
    }
    const live: Live[] = [];
    for (const effect of this.persistent) {
      // A `resolving` relationship is a **self-retiring presentation intent**
      // (§6.2): it decorates a state change the authoritative view has already
      // applied (I6), so the layer — not the caller and not any client state —
      // owns its bounded lifetime. Once the retraction completes it is dropped
      // in this very frame, which is what keeps §8.4's zero-idle contract true
      // (nothing is left marking the layer as animating) and what stops a
      // departed relationship from leaving caps behind forever.
      let progress: number | undefined;
      if (relationshipState(effect) === 'resolving') {
        progress = this.resolveProgress(effect.id, now);
        if (progress >= 1) {
          this.resolveStarts.delete(effect.id);
          continue;
        }
      }
      const fromRect = anchorRect(effect.from, rects);
      const toRect = anchorRect(effect.to, rects);
      const withProgress = progress === undefined ? {} : { progress };
      if (fromRect && toRect && !this.offContainer(effect, fromRect, toRect)) {
        live.push({ effect, from: fromRect, to: toRect, ...withProgress });
        retained.push(effect);
        continue;
      }
      // §10.3 — the third outcome. `edge` is the container the CALLER declared
      // this endpoint still lives in; without one there is nothing to clamp to
      // and the relationship retires, which is the carried behaviour.
      const outside = fromRect && toRect ? this.offContainer(effect, fromRect, toRect) : undefined;
      const known =
        outside === 'from' ? toRect : outside === 'to' ? fromRect : (fromRect ?? toRect);
      if (effect.edge === undefined || known === undefined) continue;
      const anchor = rectCenter(known);
      const clamp = clampToRect(effect.edge, anchor);
      const stub: Rect = { x: clamp.x - 1, y: clamp.y - 1, w: 2, h: 2 };
      live.push({
        effect,
        from: outside === 'from' ? stub : (fromRect ?? stub),
        to: outside === 'to' ? stub : (toRect ?? stub),
        ...withProgress,
        indicator: clamp,
        indicatorFrom: anchor,
      });
      retained.push(effect);
    }
    if (retained.length !== this.persistent.length) {
      this.persistent = retained;
      this.persistentKey = JSON.stringify(retained);
    }

    // Pass 2 — the §4.3 R5 fan: several destinations from one source leave as
    // one trunk and split at a shared node, rather than as a starburst.
    const resolved: ResolvedRelationship[] = live.map((entry) => ({
      effect: entry.effect,
      source: rectCenter(entry.from),
      destination: rectCenter(entry.to),
    }));
    const fans = fanGroups(resolved);

    // Pass 3 — the draw program itself.
    for (const entry of live) {
      const { effect } = entry;
      const fan = fans.get(effect.id);
      const ctx = {
        phase,
        reducedMotion,
        ...(fan === undefined ? {} : { fan }),
        ...(entry.progress === undefined ? {} : { progress: entry.progress }),
      };
      ops.push(...relationshipOps(effect, entry.from, entry.to, ctx));
      if (entry.indicator !== undefined && entry.indicatorFrom !== undefined) {
        const other = entry.indicatorFrom;
        const angle = Math.atan2(entry.indicator.y - other.y, entry.indicator.x - other.x);
        ops.push(
          ...edgeIndicatorOps(
            effect.category,
            entry.indicator,
            angle,
            effect.accent,
            SCENE_RELATIONSHIP.alpha.confirmed,
          ),
        );
      }
    }

    for (const transient of this.transients) {
      const center = anchorCenter(transient.invocation.target, rects);
      if (!center) continue;
      const t = reducedMotion
        ? 1
        : transient.start === undefined
          ? 0
          : Math.max(
              0,
              Math.min(1, (now - transient.start - transient.delay) / transient.duration),
            );
      if (
        !reducedMotion &&
        transient.start !== undefined &&
        now - transient.start < transient.delay
      ) {
        continue;
      }
      this.transientOps(ops, transient, center, reducedMotion ? 0.35 : t);
    }
    return ops;
  }

  /** The ops of one live transient at eased progress `t`. */
  private transientOps(ops: DrawOp[], transient: LiveTransient, center: Point, t: number): void {
    const { invocation } = transient;
    if (invocation.category === 'off-focus-ping') {
      pingOps(ops, invocation, center, t, this.options.reducedMotion);
      return;
    }
    const magnitude = invocation.magnitude ?? 1;
    const ease = 1 - (1 - t) * (1 - t);
    const alpha = Math.max(0.05, 1 - t);
    if (invocation.category === 'counter') {
      // The §6.3 fizzle rule, decision D14 — the **release** form. A countered
      // or fizzled spell's terminal lands on the STACK OBJECT, never on its
      // target, and the released target's reticle *opens* (14 → 20) and fades
      // with no burst at all. "Nothing happened to me" must be a visible event
      // rather than merely the absence of one, so this is deliberately a
      // different shape from every impact ring — small, opening, particle-free.
      ops.push({
        op: 'circle',
        category: invocation.category,
        x: center.x,
        y: center.y,
        r: SCENE_RELATIONSHIP.reticleRadius + ease * 6,
        color: invocation.accent,
        alpha,
        fill: false,
      });
      return;
    }
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
    this.resolveStarts.clear();
    this.tracking = false;
    this.stats.liveParticles = 0;
    this.stats.liveTransients = 0;
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

/**
 * The off-focus crest ping (visual-system §8, "Off-focus activity"): a quiet
 * rune mark at the acting seat's crest — a ring that breathes outward once,
 * plus {@link PING}`.spokes` radial strokes that give the cue a shape channel
 * of its own (§7: never color-only). Under reduced motion it is a **static
 * badge**: the same mark at rest with a filled center, drawn once and held for
 * {@link EFFECT_TIMING.offFocusHoldMs}. It never spawns particles, so Lite and
 * `minimal` density render it unchanged.
 */
function pingOps(
  ops: DrawOp[],
  invocation: TransientInvocation,
  center: Point,
  t: number,
  reducedMotion: boolean,
): void {
  const ease = 1 - (1 - t) * (1 - t);
  const radius = reducedMotion ? PING.radius : PING.radius * (0.55 + 0.45 * ease);
  const alpha = reducedMotion ? 0.9 : Math.max(0.1, 1 - t);
  const { category, accent } = invocation;
  ops.push({
    op: 'circle',
    category,
    x: center.x,
    y: center.y,
    r: radius,
    color: accent,
    alpha,
    fill: false,
  });
  for (let spoke = 0; spoke < PING.spokes; spoke += 1) {
    const angle = (spoke * 2 * Math.PI) / PING.spokes + Math.PI / 4;
    const at = (scale: number): Point => ({
      x: center.x + Math.cos(angle) * radius * scale,
      y: center.y + Math.sin(angle) * radius * scale,
    });
    ops.push({
      ...segment(category, at(PING.spokeInner), at(PING.spokeOuter), accent, 2),
      alpha,
    });
  }
  if (reducedMotion) {
    // The badge's held center: a filled mark that reads at a glance without
    // any animation, distinct from the hollow pulsing ring.
    ops.push({
      op: 'circle',
      category,
      x: center.x,
      y: center.y,
      r: PING.badgeRadius,
      color: accent,
      alpha,
      fill: true,
    });
  }
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
    } else if (op.op === 'rect') {
      // The R9 square terminal — the shape that separates "attached" from every
      // circular target cap.
      if (op.fill) {
        graphics.lineStyle(0);
        graphics.beginFill(hexColor(op.color), op.alpha);
        graphics.drawRect(op.x, op.y, op.w, op.h);
        graphics.endFill();
      } else {
        graphics.lineStyle({ width: 1, color: hexColor(op.color), alpha: op.alpha });
        graphics.drawRect(op.x, op.y, op.w, op.h);
      }
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
