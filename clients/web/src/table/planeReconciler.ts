/**
 * DOM scene-plane reconciler (issue #481, ADR 0030 layer 2) — the shipped Pixi
 * reconciler pattern (`sceneReconciler.ts`, issues #58/#334) retargeted at the
 * DOM plane, keeping every invariant it proved:
 *
 * - **Reconcile by entity id.** Successive {@link StagedPlane}s (#478's output)
 *   update one DOM element per staged object in place: add new entities,
 *   re-render faces whose visual signature changed, move those that only
 *   shifted, remove the gone. Full rebuilds are reserved for
 *   reconnect/fast-forward ({@link PlaneReconciler.rebuild}).
 * - **The cache is never load-bearing.** After applying any single plane the
 *   DOM is identical to a fresh mount of that plane alone (fresh-mount
 *   equivalence, re-tested structurally).
 * - **Input is never gated.** An element's layout box (`left`/`top`) lands on
 *   its authoritative rect the moment a plane reconciles; motion is only a
 *   decaying `transform` offset (FLIP), so destination rects — and
 *   {@link PlaneReconciler.targetFor} — are addressable at 0 ms.
 * - **A newer scene retargets or discards in-flight motion**: a mid-flight
 *   reconcile re-anchors the offset from the current visual position; a
 *   departed entity's motion is dropped with it.
 * - **Reduced motion snaps**, byte-identical to the un-animated path; the
 *   collapse rides the scene tokens ({@link sceneMotionMs}).
 * - **Zero work when nothing changes** (ADR 0030): an unchanged plane touches
 *   no element ({@link PlaneReconciler.lastStats} proves it), and `advance` is
 *   inert with nothing in flight — the spike's full-re-render travel-storm
 *   number is the cautionary tale this shape exists to avoid.
 *
 * Motion follows the visual-system §8 grammar through the #480 tokens: card
 * moves and enter/leave travel run in the zone-travel class with **FLIP travel
 * ghosts** to/from the owning seat's zone piles (destination addressable at
 * 0 ms; the ghost is decorative, `pointer-events: none`); slot/focus changes
 * tween regions in the staging class (300–500 ms); simultaneous batches stagger
 * per item inside the ≤ 800 ms window (items beyond it land together); and the
 * skippability contract holds per class — no single tween exceeds 600 ms, and
 * the one composition that can (the batch window) is user-skippable via
 * {@link PlaneReconciler.skipTransitions}.
 *
 * Animation is driven by {@link PlaneReconciler.advance} with a monotonic
 * timestamp — no wall clock, no `requestAnimationFrame` — so tests drive it
 * deterministically. Card faces render through an injected
 * {@link PlaneFaceRenderer} (the CardFace-consuming default lives in
 * `planeFaceRenderer.tsx`); the reconciler itself owns only geometry and
 * lifecycle. The shipped Pixi reconciler is untouched and keeps shipping.
 */
import type { EntityId, PlayerId } from '../protocol';
import { SCENE_BATCH, SCENE_MOTION, sceneMotionMs } from '../sceneTokens';
import type { Rect } from './scene';
import type { PlaneRegion, PlaneRender, StagedPlane, SummaryTileSlot } from './plane';

/** Renders a card face into (and re-renders it within) an entity wrapper.
 * `signature` is the "same-looking card" key: equal signatures ⇒ `render` may
 * be skipped, exactly like the Pixi reconciler's visual signature. */
export interface PlaneFaceRenderer {
  /** A stable key of every visual input `render` draws for this entity. */
  signature(render: PlaneRender): string;
  /** Draw (or redraw) the face inside the wrapper element. */
  render(el: HTMLElement, render: PlaneRender): void;
}

/** Resolved animation settings (durations seed from the #480 scene tokens). */
export interface PlaneAnimation {
  /** `prefers-reduced-motion`: every transition snaps; nothing else differs. */
  reducedMotion: boolean;
  /** Card move / travel-ghost duration (zone-travel class), ms. */
  travelMs: number;
  /** Region slot / focus re-staging duration (staging class), ms. */
  stagingMs: number;
  /** Per-item batch stagger, ms. */
  staggerMs: number;
  /** Total batch window, ms — items beyond it land together. */
  windowMs: number;
}

/** Options for {@link PlaneReconciler}. Omit `animate` for the snap default. */
export interface PlaneReconcilerOptions {
  /** The face renderer every entity wrapper draws through. */
  face: PlaneFaceRenderer;
  /** Enable the motion layer; `true` for token defaults, a partial to override. */
  animate?: boolean | Partial<PlaneAnimation>;
}

/** Optional semantic hint from the GameView presentation adapter (#492). */
export interface PlaneMotionHint {
  entityId?: EntityId;
  category: string;
  /** Semantic source anchor (`hand:*`, `stack:*`, `pile:*`, seat, or entity). */
  from?: string;
  /** Semantic destination anchor (`hand:*`, `stack:*`, `pile:*`, seat, or entity). */
  to?: string;
  durationMs: number;
  delayMs: number;
}

/** What one reconcile actually did — the zero-work-when-nothing-changes gate. */
export interface ReconcileStats {
  /** Entity wrappers created. */
  created: number;
  /** Faces re-rendered because their signature changed. */
  updatedFaces: number;
  /** Entity wrappers whose rect moved. */
  moved: number;
  /** Entity wrappers removed. */
  removed: number;
  /** Region/chrome elements created, updated, or moved. */
  chrome: number;
  /** Whether draw order had to be reasserted. */
  reordered: boolean;
}

const ZERO_STATS: ReconcileStats = {
  created: 0,
  updatedFaces: 0,
  moved: 0,
  removed: 0,
  chrome: 0,
  reordered: false,
};

/** Clamp to the unit interval. */
function clamp01(t: number): number {
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

/** Ease-out cubic — the JS form of the tokens' decelerate curve. */
function easeOutCubic(t: number): number {
  const u = 1 - t;
  return 1 - u * u * u;
}

/** A decaying FLIP offset: the element's layout box is already at its final
 * rect; only this transform offset animates to zero. */
interface FlipTween {
  el: HTMLElement;
  /** Offset from the final rect at tween start (the "invert" of FLIP). */
  dx: number;
  dy: number;
  duration: number;
  /** Batch-stagger delay before the tween begins, ms. */
  delay: number;
  /** First `advance` timestamp; set lazily so `reconcile` needs no clock. */
  start?: number;
}

/** A travel ghost: a decorative clone easing between two rects, then removed. */
interface GhostTween {
  el: HTMLElement;
  from: Rect;
  to: Rect;
  /** Whether the ghost fades out as it travels (a leaving entity). */
  fadeOut: boolean;
  duration: number;
  delay: number;
  start?: number;
}

/** An entering wrapper's fade-up (opacity only; the box is already in place). */
interface EnterTween {
  el: HTMLElement;
  duration: number;
  delay: number;
  start?: number;
}

/** Every staged region, in stable plane order. */
export function planeRegions(plane: StagedPlane): PlaneRegion[] {
  return [plane.receiver, plane.farSide, ...plane.wings].filter(
    (r): r is PlaneRegion => r !== undefined,
  );
}

/** Every individually addressable render, in stable plane order (region renders
 * first, then compact-tile candidate strips). */
export function planeRenders(plane: StagedPlane): PlaneRender[] {
  return [
    ...planeRegions(plane).flatMap((r) => r.renders),
    ...plane.tiles.flatMap((t) => t.candidates),
  ];
}

/** Apply a rect to an element's layout box (the authoritative position). */
function applyRect(el: HTMLElement, rect: Rect): void {
  el.style.left = `${rect.x}px`;
  el.style.top = `${rect.y}px`;
  el.style.width = `${rect.w}px`;
  el.style.height = `${rect.h}px`;
}

/** Rect equality. */
function sameRect(a: Rect, b: Rect): boolean {
  return a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

/** One cached chrome element (region, crest, piles, tile) and its inputs. */
interface CachedChrome {
  el: HTMLElement;
  rect: Rect;
  /** Serialized non-geometry inputs, to skip attribute writes when unchanged. */
  meta: string;
}

/** One cached entity wrapper. */
interface CachedCard {
  el: HTMLElement;
  signature: string;
  rect: Rect;
  seat: PlayerId;
}

/** Reconciles successive {@link StagedPlane}s into one DOM root. */
export class PlaneReconciler {
  /** The plane element this reconciler owns; the caller positions/transforms it. */
  readonly root: HTMLElement;

  private readonly face: PlaneFaceRenderer;
  private readonly animation: PlaneAnimation | null;

  /** Layer order is structural: regions under entities under ghosts. */
  private readonly regionLayer: HTMLElement;
  private readonly entityLayer: HTMLElement;
  private readonly ghostLayer: HTMLElement;

  private readonly chrome = new Map<string, CachedChrome>();
  private readonly cards = new Map<EntityId, CachedCard>();
  private readonly targets = new Map<EntityId, Rect>();

  private readonly cardMoves = new Map<EntityId, FlipTween>();
  private readonly chromeMoves = new Map<string, FlipTween>();
  private readonly enters = new Map<EntityId, EnterTween>();
  private ghosts: GhostTween[] = [];

  /** What the latest reconcile did (see {@link ReconcileStats}). */
  lastStats: ReconcileStats = { ...ZERO_STATS };

  /** Set during {@link rebuild} so the fresh mount renders complete, no tweens. */
  private suppressMotion = false;

  constructor(root: HTMLElement, options: PlaneReconcilerOptions) {
    this.root = root;
    this.face = options.face;
    this.animation = resolvePlaneAnimation(options.animate);
    this.regionLayer = root.ownerDocument.createElement('div');
    this.regionLayer.dataset.layer = 'regions';
    this.entityLayer = root.ownerDocument.createElement('div');
    this.entityLayer.dataset.layer = 'entities';
    this.ghostLayer = root.ownerDocument.createElement('div');
    this.ghostLayer.dataset.layer = 'ghosts';
    this.ghostLayer.style.pointerEvents = 'none';
    root.append(this.regionLayer, this.entityLayer, this.ghostLayer);
  }

  /** Whether transitions tween (animation enabled and not reduced motion). */
  private get animating(): boolean {
    return this.animation !== null && !this.animation.reducedMotion && !this.suppressMotion;
  }

  /**
   * Bring the DOM in line with `plane`. Every element's layout box lands on its
   * authoritative rect before this returns; with animation, FLIP offsets and
   * travel ghosts then decay under {@link advance}. Unchanged elements are not
   * touched at all.
   *
   * `suppressMotion` snaps this one reconcile to its final layout without a full
   * teardown — the fast-forward collapse (#493) when a newer view arrives before
   * the prior transition settled: destinations apply immediately, no catch-up
   * travel is spawned, yet the cache and unchanged elements are kept.
   */
  reconcile(
    plane: StagedPlane,
    motionHints: readonly PlaneMotionHint[] = [],
    suppressMotion = false,
  ): void {
    const stats: ReconcileStats = { ...ZERO_STATS };
    const anim = this.animating && !suppressMotion ? this.animation! : null;

    this.reconcileChrome(plane, stats, anim);

    // ── Entities, by id ─────────────────────────────────────────────────────
    const renders = planeRenders(plane);
    const hints = new Map(
      motionHints
        .filter(
          (hint): hint is PlaneMotionHint & { entityId: EntityId } => hint.entityId !== undefined,
        )
        .map((hint) => [hint.entityId, hint]),
    );
    const routes = new Map<EntityId, { hint: PlaneMotionHint; from?: Rect; to?: Rect }>();
    if (hints.size > 0) {
      const futureRects = new Map<EntityId, Rect>();
      for (const render of renders) {
        for (const memberId of render.memberIds) futureRects.set(memberId, render.rect);
      }
      for (const [entityId, hint] of hints) {
        routes.set(entityId, {
          hint,
          from: hint.from ? this.resolveMotionRef(hint.from, plane, futureRects) : undefined,
          to: hint.to ? this.resolveMotionRef(hint.to, plane, futureRects) : undefined,
        });
      }
    }
    const consumedRoutes = new Set<EntityId>();
    const present = new Set<EntityId>();
    const orderedIds: EntityId[] = [];
    let batchIndex = 0;
    this.targets.clear();

    for (const render of renders) {
      present.add(render.entityId);
      orderedIds.push(render.entityId);
      this.targets.set(render.entityId, render.rect);
      const signature = this.face.signature(render);
      const cached = this.cards.get(render.entityId);
      const hint = hints.get(render.entityId);

      if (cached) {
        if (hint || cached.el.dataset.motion !== undefined) applyMotionHint(cached.el, hint);
        // Sync the wrapper's staging facts (the ladder can re-tier a card; a
        // prompt can make it a candidate). Values only — the attribute set is
        // fixed at creation, so serialization order never drifts from fresh.
        if (cached.el.dataset.seat !== render.seat) cached.el.dataset.seat = render.seat;
        if (cached.el.dataset.tier !== render.tier) cached.el.dataset.tier = render.tier;
        const candidate = String(render.candidate);
        if (cached.el.dataset.candidate !== candidate) cached.el.dataset.candidate = candidate;
        if (cached.signature !== signature) {
          this.face.render(cached.el, render);
          cached.signature = signature;
          stats.updatedFaces += 1;
        }
        if (!sameRect(cached.rect, render.rect)) {
          // FLIP: land the box on the new rect now; carry the visual offset
          // (including any in-flight offset — the retarget case) in transform.
          const offset = anim ? this.currentOffset(render.entityId, cached) : null;
          const fromX = cached.rect.x + (offset?.dx ?? 0);
          const fromY = cached.rect.y + (offset?.dy ?? 0);
          applyRect(cached.el, render.rect);
          if (anim) {
            this.startFlip(this.cardMoves, render.entityId, {
              el: cached.el,
              dx: fromX - render.rect.x,
              dy: fromY - render.rect.y,
              duration: hint?.durationMs ?? anim.travelMs,
              delay: hint?.delayMs ?? 0,
            });
          }
          cached.rect = render.rect;
          stats.moved += 1;
        }
      } else {
        const el = this.root.ownerDocument.createElement('div');
        el.style.position = 'absolute';
        el.dataset.entityId = render.entityId;
        el.dataset.seat = render.seat;
        el.dataset.tier = render.tier;
        el.dataset.candidate = String(render.candidate);
        if (hint) applyMotionHint(el, hint);
        applyRect(el, render.rect);
        this.face.render(el, render);
        this.entityLayer.appendChild(el);
        this.cards.set(render.entityId, {
          el,
          signature,
          rect: render.rect,
          seat: render.seat,
        });
        stats.created += 1;
        if (anim) {
          const delay = hint?.delayMs ?? batchDelay(batchIndex, anim);
          batchIndex += 1;
          el.style.opacity = '0';
          const duration = hint?.durationMs ?? anim.travelMs;
          this.enters.set(render.entityId, { el, duration, delay });
          const from = routes.get(render.entityId)?.from ?? this.travelHome(plane, render.seat);
          if (from) this.spawnGhost(el, from, render.rect, false, duration, delay);
          consumedRoutes.add(render.entityId);
        }
      }
    }

    // Retire entities absent from this plane. The wrapper (and with it the hit
    // box) is removed immediately — a leaving card is never addressable — and,
    // when animating, a decorative ghost travels to the seat's zone home.
    for (const [entityId, cached] of this.cards) {
      if (present.has(entityId)) continue;
      if (anim) {
        const hint = hints.get(entityId);
        const to = routes.get(entityId)?.to ?? this.travelHome(plane, cached.seat) ?? cached.rect;
        const delay = hint?.delayMs ?? batchDelay(batchIndex, anim);
        batchIndex += 1;
        this.spawnGhost(cached.el, cached.rect, to, true, hint?.durationMs ?? anim.travelMs, delay);
        consumedRoutes.add(entityId);
      }
      cached.el.remove();
      this.cards.delete(entityId);
      this.cardMoves.delete(entityId);
      this.enters.delete(entityId);
      stats.removed += 1;
    }

    // Some semantic travel never has a battlefield wrapper at either endpoint
    // (draw pile→hand, hand→stack, stack→graveyard). Render a generic passive
    // proxy so those authoritative transitions are visible rather than merely
    // recorded as intents. It has no entity id and can never receive input.
    if (anim) {
      for (const [entityId, route] of routes) {
        if (consumedRoutes.has(entityId) || !route.from || !route.to) continue;
        this.spawnMotionProxy(
          entityId,
          route.hint.category,
          route.from,
          route.to,
          route.hint.durationMs,
          route.hint.delayMs,
        );
      }
    }

    // Reassert draw order only when it actually changed (zero-work rule).
    if (!this.orderMatches(orderedIds)) {
      for (const id of orderedIds) this.entityLayer.appendChild(this.cards.get(id)!.el);
      stats.reordered = true;
    }

    this.lastStats = stats;
  }

  /**
   * Full rebuild — reserved for reconnect/fast-forward (budget: one rebuild ≤
   * 50–100 ms, never the per-change path): drop every element and mount
   * `plane` fresh, with no transitions (a reconnect renders complete).
   */
  rebuild(plane: StagedPlane): void {
    this.clear();
    this.suppressMotion = true;
    try {
      this.reconcile(plane);
    } finally {
      this.suppressMotion = false;
    }
  }

  /**
   * Advance every in-flight transition to `now` (monotonic ms). Inert when the
   * motion layer is off or nothing is in flight — the plane costs zero while
   * idle. Interactivity never waits on this: it only decays visual offsets
   * toward layouts `reconcile` already made authoritative.
   */
  advance(now: number): void {
    if (this.animation === null) return;
    this.advanceFlips(this.cardMoves, now);
    this.advanceFlips(this.chromeMoves, now);

    for (const [id, tween] of this.enters) {
      const p = progress(tween, now);
      if (p === null) continue;
      if (p >= 1) {
        tween.el.style.opacity = '';
        this.enters.delete(id);
      } else {
        tween.el.style.opacity = String(easeOutCubic(p).toFixed(3));
      }
    }

    this.ghosts = this.ghosts.filter((ghost) => {
      const p = progress(ghost, now);
      if (p === null) return true;
      if (p >= 1) {
        ghost.el.remove();
        return false;
      }
      const e = easeOutCubic(p);
      const dx = (ghost.to.x - ghost.from.x) * e;
      const dy = (ghost.to.y - ghost.from.y) * e;
      ghost.el.style.transform = `translate(${dx}px, ${dy}px)`;
      if (ghost.fadeOut) ghost.el.style.opacity = String((1 - p).toFixed(3));
      return true;
    });
  }

  /**
   * Complete every in-flight transition instantly — the skippability contract:
   * no single class exceeds 600 ms, and the one composition that can (a batch
   * inside the ≤ 800 ms window) must be user-skippable. Ends on the exact
   * layouts the latest plane made authoritative (which input never waited on).
   */
  skipTransitions(): void {
    for (const tween of this.cardMoves.values()) tween.el.style.transform = '';
    for (const tween of this.chromeMoves.values()) tween.el.style.transform = '';
    for (const tween of this.enters.values()) tween.el.style.opacity = '';
    for (const ghost of this.ghosts) ghost.el.remove();
    this.cardMoves.clear();
    this.chromeMoves.clear();
    this.enters.clear();
    this.ghosts = [];
  }

  /** Whether any transition is still in flight; always false un-animated. */
  hasPendingAnimations(): boolean {
    return (
      this.cardMoves.size > 0 ||
      this.chromeMoves.size > 0 ||
      this.enters.size > 0 ||
      this.ghosts.length > 0
    );
  }

  /** Drop cross-surface proxies from an older authoritative view immediately. */
  discardMotionProxies(): void {
    this.ghosts = this.ghosts.filter((ghost) => {
      if (ghost.el.dataset.motionProxy === undefined) return true;
      ghost.el.remove();
      return false;
    });
  }

  /** The authoritative rect of an entity from the latest reconcile — available
   * immediately, independent of any in-flight offset. */
  targetFor(entityId: EntityId): Rect | undefined {
    return this.targets.get(entityId);
  }

  /**
   * The entity's current visual rect, including an in-flight FLIP offset.
   * This is deliberately distinct from {@link targetFor}: effects follow the
   * pixels while pointer/focus routing addresses the authoritative destination.
   */
  visualFor(entityId: EntityId): Rect | undefined {
    const cached = this.cards.get(entityId);
    return cached ? visualRect(cached.rect, cached.el) : undefined;
  }

  /** Current visual rect of reconciled chrome (`crest:<seat>`, `piles:<seat>`). */
  chromeVisualFor(key: string): Rect | undefined {
    const cached = this.chrome.get(key);
    return cached ? visualRect(cached.rect, cached.el) : undefined;
  }

  /** The entity's wrapper element, if staged (never a ghost on its way out). */
  elementFor(entityId: EntityId): HTMLElement | undefined {
    return this.cards.get(entityId)?.el;
  }

  /** Drop every element and transition, emptying the layers. */
  clear(): void {
    for (const cached of this.cards.values()) cached.el.remove();
    for (const cached of this.chrome.values()) cached.el.remove();
    for (const ghost of this.ghosts) ghost.el.remove();
    this.cards.clear();
    this.chrome.clear();
    this.targets.clear();
    this.cardMoves.clear();
    this.chromeMoves.clear();
    this.enters.clear();
    this.ghosts = [];
    this.lastStats = { ...ZERO_STATS };
  }

  // ── internals ─────────────────────────────────────────────────────────────

  /** Reconcile the region slots and chrome anchors (crest, piles, tiles). */
  private reconcileChrome(
    plane: StagedPlane,
    stats: ReconcileStats,
    anim: PlaneAnimation | null,
  ): void {
    const present = new Set<string>();
    const orderedKeys: string[] = [];
    const upsert = (key: string, kind: string, rect: Rect, meta: Record<string, string>): void => {
      present.add(key);
      orderedKeys.push(key);
      const metaKey = JSON.stringify(meta);
      const cached = this.chrome.get(key);
      if (cached) {
        if (cached.meta !== metaKey) {
          // Drop keys the new meta no longer carries (a wing that became the
          // far side sheds side/rank), then write the rest — conditional keys
          // always append in `regionMeta`'s fixed sequence, so serialization
          // order stays identical to a fresh mount.
          const old = JSON.parse(cached.meta) as Record<string, string>;
          for (const name of Object.keys(old)) {
            if (!(name in meta)) delete cached.el.dataset[name];
          }
          for (const [name, value] of Object.entries(meta)) cached.el.dataset[name] = value;
          cached.meta = metaKey;
          stats.chrome += 1;
        }
        if (!sameRect(cached.rect, rect)) {
          // Slot/focus re-staging: FLIP in the staging motion class.
          const offset = anim ? this.currentOffset(key, cached, this.chromeMoves) : null;
          const fromX = cached.rect.x + (offset?.dx ?? 0);
          const fromY = cached.rect.y + (offset?.dy ?? 0);
          applyRect(cached.el, rect);
          if (anim) {
            this.startFlip(this.chromeMoves, key, {
              el: cached.el,
              dx: fromX - rect.x,
              dy: fromY - rect.y,
              duration: anim.stagingMs,
              delay: 0,
            });
          }
          cached.rect = rect;
          stats.chrome += 1;
        }
        return;
      }
      const el = this.root.ownerDocument.createElement('div');
      el.style.position = 'absolute';
      el.dataset.slot = kind;
      el.dataset.key = key;
      for (const [name, value] of Object.entries(meta)) el.dataset[name] = value;
      applyRect(el, rect);
      this.regionLayer.appendChild(el);
      this.chrome.set(key, { el, rect, meta: metaKey });
      stats.chrome += 1;
    };

    for (const region of planeRegions(plane)) {
      upsert(`region:${region.seat}`, 'region', region.rect, regionMeta(region));
      upsert(`crest:${region.seat}`, 'crest', region.crest, crestMeta(region));
      upsert(`piles:${region.seat}`, 'piles', region.piles, pilesMeta(region));
    }
    for (const tile of plane.tiles) {
      upsert(`tile:${tile.seat}`, 'tile', tile.rect, tileMeta(tile));
    }

    for (const [key, cached] of this.chrome) {
      if (present.has(key)) continue;
      cached.el.remove();
      this.chrome.delete(key);
      this.chromeMoves.delete(key);
      stats.chrome += 1;
    }

    // Reassert slot order (fresh-mount equivalence) only when it drifted —
    // e.g. a focus swap re-stages which seat mounts as the far side first.
    const children = this.regionLayer.children;
    let ordered = children.length === orderedKeys.length;
    for (let i = 0; ordered && i < orderedKeys.length; i += 1) {
      ordered = (children[i] as HTMLElement).dataset.key === orderedKeys[i];
    }
    if (!ordered) {
      for (const key of orderedKeys) this.regionLayer.appendChild(this.chrome.get(key)!.el);
      stats.reordered = true;
    }
  }

  /** The current visual offset of an element with a possibly in-flight FLIP —
   * what a retargeting reconcile continues from, so motion never jumps. */
  private currentOffset(
    key: EntityId | string,
    cached: { el: HTMLElement },
    map: Map<string, FlipTween> | Map<EntityId, FlipTween> = this.cardMoves,
  ): { dx: number; dy: number } | null {
    const tween = (map as Map<string, FlipTween>).get(key as string);
    if (!tween) return null;
    const match = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(cached.el.style.transform);
    if (!match) return { dx: tween.dx, dy: tween.dy };
    return { dx: Number(match[1]), dy: Number(match[2]) };
  }

  /** Start (or restart) a FLIP tween and apply its initial inverted offset. */
  private startFlip(
    map: Map<string, FlipTween> | Map<EntityId, FlipTween>,
    key: string,
    tween: Omit<FlipTween, 'start'>,
  ): void {
    (map as Map<string, FlipTween>).set(key, { ...tween });
    tween.el.style.transform = `translate(${tween.dx}px, ${tween.dy}px)`;
  }

  /** Decay a map of FLIP offsets toward zero. */
  private advanceFlips(map: Map<string, FlipTween> | Map<EntityId, FlipTween>, now: number): void {
    for (const [key, tween] of map as Map<string, FlipTween>) {
      const p = progress(tween, now);
      if (p === null) continue;
      if (p >= 1) {
        tween.el.style.transform = '';
        (map as Map<string, FlipTween>).delete(key);
      } else {
        const e = 1 - easeOutCubic(p);
        tween.el.style.transform = `translate(${(tween.dx * e).toFixed(2)}px, ${(tween.dy * e).toFixed(2)}px)`;
      }
    }
  }

  /** The travel home of a seat's zone changes: its region's pile cluster (a
   * compact tile for a tile seat) — where enter ghosts rise from and leave
   * ghosts return to. */
  private travelHome(plane: StagedPlane, seat: PlayerId): Rect | undefined {
    const region = planeRegions(plane).find((r) => r.seat === seat);
    if (region) return region.piles;
    return plane.tiles.find((t) => t.seat === seat)?.rect;
  }

  /** Resolve a semantic motion reference into the plane coordinate space. */
  private resolveMotionRef(
    ref: string,
    plane: StagedPlane,
    futureRects: ReadonlyMap<EntityId, Rect>,
  ): Rect | undefined {
    const future = futureRects.get(ref);
    if (future) return future;
    const card = this.cards.get(ref);
    if (card) return visualRect(card.rect, card.el);

    const [kind, id] = ref.split(':', 2);
    if (!id) return undefined;
    const region = planeRegions(plane).find((candidate) => candidate.seat === id);
    const tile = plane.tiles.find((candidate) => candidate.seat === id);
    if (kind === 'seat') return region?.crest ?? tile?.crest;
    if (kind === 'pile') return region?.piles ?? tile?.rect;
    if (kind === 'hand') {
      const home = region?.rect ?? tile?.rect;
      if (!home) return undefined;
      return {
        x: home.x + home.w / 2 - 24,
        y: Math.min(plane.height - 8, home.y + home.h),
        w: 48,
        h: 68,
      };
    }
    if (kind === 'stack') {
      return {
        x: plane.corridor.x + plane.corridor.w / 2 - 24,
        y: plane.corridor.y + plane.corridor.h / 2 - 34,
        w: 48,
        h: 68,
      };
    }
    return undefined;
  }

  /** Spawn a decorative travel ghost cloned from an entity wrapper. */
  private spawnGhost(
    el: HTMLElement,
    from: Rect,
    to: Rect,
    fadeOut: boolean,
    duration: number,
    delay: number,
  ): void {
    const ghost = el.cloneNode(true) as HTMLElement;
    ghost.dataset.ghost = 'true';
    delete ghost.dataset.entityId;
    ghost.style.pointerEvents = 'none';
    ghost.style.opacity = '1';
    ghost.style.transform = '';
    applyRect(ghost, { ...from, w: to.w, h: to.h });
    this.ghostLayer.appendChild(ghost);
    this.ghosts.push({ el: ghost, from, to, fadeOut, duration, delay });
  }

  /** Spawn a generic card-shaped travel proxy for cross-surface motion. */
  private spawnMotionProxy(
    entityId: EntityId,
    category: string,
    from: Rect,
    to: Rect,
    duration: number,
    delay: number,
  ): void {
    const proxy = this.root.ownerDocument.createElement('div');
    proxy.dataset.ghost = 'true';
    proxy.dataset.motionProxy = category;
    proxy.dataset.motionEntity = entityId;
    proxy.style.position = 'absolute';
    proxy.style.pointerEvents = 'none';
    proxy.style.opacity = '0.82';
    proxy.style.border = '2px solid var(--gold, #f2c94c)';
    proxy.style.borderRadius = '7px';
    proxy.style.background = 'var(--raised, #202938)';
    proxy.style.boxShadow = '0 8px 18px rgb(0 0 0 / 35%)';
    applyRect(proxy, { ...from, w: to.w, h: to.h });
    this.ghostLayer.appendChild(proxy);
    this.ghosts.push({ el: proxy, from, to, fadeOut: false, duration, delay });
  }

  /** Whether the entity layer's child order already matches `orderedIds`. */
  private orderMatches(orderedIds: EntityId[]): boolean {
    const children = this.entityLayer.children;
    if (children.length !== orderedIds.length) return false;
    for (let i = 0; i < orderedIds.length; i += 1) {
      if ((children[i] as HTMLElement).dataset.entityId !== orderedIds[i]) return false;
    }
    return true;
  }
}

/** Lazily anchor a tween's clock and return its progress, or `null` before its
 * first `advance`. The batch-stagger delay holds progress at zero until it
 * elapses (so staggered items land inside the window, later ones together). */
function progress(tween: { start?: number; delay: number; duration: number }, now: number): number {
  if (tween.start === undefined) tween.start = now;
  if (tween.duration <= 0) return 1;
  return clamp01((now - tween.start - tween.delay) / tween.duration);
}

/** Apply the reconciler-owned FLIP translate to an authoritative rect. */
function visualRect(rect: Rect, el: HTMLElement): Rect {
  const match = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(el.style.transform);
  if (!match) return rect;
  return { ...rect, x: rect.x + Number(match[1]), y: rect.y + Number(match[2]) };
}

/** Expose the active grammar class to CSS/card consumers; clear it on the next
 * view so a rapid update cannot leave stale semantic animation state behind. */
function applyMotionHint(el: HTMLElement, hint: PlaneMotionHint | undefined): void {
  if (!hint) {
    delete el.dataset.motion;
    el.style.removeProperty('--motion-ms');
    el.style.removeProperty('--motion-delay-ms');
    el.style.removeProperty('--motion-card');
    el.style.removeProperty('--motion-card-delay');
    return;
  }
  el.dataset.motion = hint.category;
  el.style.setProperty('--motion-ms', `${hint.durationMs}ms`);
  el.style.setProperty('--motion-delay-ms', `${hint.delayMs}ms`);
  el.style.setProperty('--motion-card', `${hint.durationMs}ms`);
  el.style.setProperty('--motion-card-delay', `${hint.delayMs}ms`);
}

/** The batch-stagger delay for the `index`-th simultaneous item: per-item
 * stagger, clamped so every item completes inside the total window (items
 * beyond the window land together at its edge). */
function batchDelay(index: number, anim: PlaneAnimation): number {
  return Math.min(index * anim.staggerMs, Math.max(0, anim.windowMs - anim.travelMs));
}

/** A region's non-geometry inputs as data attributes. */
function regionMeta(region: PlaneRegion): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    kind: region.kind,
    rung: String(region.rung),
    surface: region.surface,
    label: region.label,
    life: String(region.life),
    hand: String(region.handCount),
    focused: String(region.focused),
    eliminated: String(region.eliminated),
    attacked: String(region.attacked),
    active: String(region.active),
    priority: String(region.priority),
  };
  if (region.side !== undefined) meta.side = region.side;
  if (region.rank !== undefined) meta.rank = String(region.rank);
  if (region.digest) {
    meta.digestCreatures = String(region.digest.creatures);
    meta.digestOthers = String(region.digest.others);
    meta.digestLands = String(region.digest.lands);
  }
  return meta;
}

/**
 * A crest cluster's non-geometry inputs. The crest is staged at every count and
 * every rung, so the markers that must never degrade away ride it: the seat's
 * life/hand readout, the priority glow, and the **attacked ring** — combat
 * against any seat is drawn regardless of which board holds focus
 * (layout-model §Focus model, "off-focus activity is never silent").
 */
function crestMeta(region: PlaneRegion): Record<string, string> {
  return {
    seat: region.seat,
    life: String(region.life),
    hand: String(region.handCount),
    attacked: String(region.attacked),
    priority: String(region.priority),
  };
}

/** A seat's zone-pile counts as data attributes (the authoritative pile data a
 * draw or a battlefield→graveyard move must reconcile, slots unmoved). */
function zonesMeta(zones: PlaneRegion['zones']): Record<string, string> {
  const meta: Record<string, string> = {
    library: String(zones.library),
    graveyard: String(zones.graveyard),
    exile: String(zones.exile),
    command: String(zones.command ?? 0),
  };
  if (zones.graveyardTop) {
    meta.top = zones.graveyardTop.name;
    meta.topColor = zones.graveyardTop.colorIdentity;
  }
  return meta;
}

/** A pile cluster's non-geometry inputs as data attributes. */
function pilesMeta(region: PlaneRegion): Record<string, string> {
  return { seat: region.seat, ...zonesMeta(region.zones) };
}

/** A compact tile's non-geometry inputs as data attributes (a tile owes the
 * seat's zone counts too — they are its whole summary). */
function tileMeta(tile: SummaryTileSlot): Record<string, string> {
  return {
    seat: tile.seat,
    label: tile.label,
    life: String(tile.life),
    hand: String(tile.handCount),
    overflow: String(tile.candidateOverflow),
    eliminated: String(tile.eliminated),
    attacked: String(tile.attacked),
    active: String(tile.active),
    priority: String(tile.priority),
    ...zonesMeta(tile.zones),
  };
}

/** Resolve the `animate` option: token-seeded defaults, `null` when absent. */
function resolvePlaneAnimation(animate: PlaneReconcilerOptions['animate']): PlaneAnimation | null {
  if (!animate) return null;
  const defaults: PlaneAnimation = {
    reducedMotion: false,
    travelMs: SCENE_MOTION.zoneTravel.ms,
    stagingMs: SCENE_MOTION.staging.ms,
    staggerMs: SCENE_BATCH.staggerMs,
    windowMs: SCENE_BATCH.windowMs,
  };
  if (animate === true) return defaults;
  const resolved = { ...defaults, ...animate };
  // The reduced-motion collapse rides the tokens: zero duration ⇒ every
  // transition completes on its first advance with no intermediate state.
  if (resolved.reducedMotion) {
    resolved.travelMs = sceneMotionMs('zoneTravel', true);
    resolved.stagingMs = sceneMotionMs('staging', true);
  }
  return resolved;
}
