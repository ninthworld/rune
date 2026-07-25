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
 * - **×N piles are objects, not renders.** A permanent joining a fold travels
 *   into the pile standing for it and one leaving a fold rises out of it
 *   (never a teleport to the zone piles), and when a fold's representative
 *   departs the pile's wrapper is re-keyed onto the next member — no removal
 *   and re-entrance flashing over each other in the same rect.
 * - **A newer scene retargets or discards in-flight motion**: a mid-flight
 *   reconcile re-anchors the offset from the current visual position; a
 *   departed entity's motion is dropped with it.
 * - **Reduced motion snaps**, byte-identical to the un-animated path; the
 *   collapse rides the scene tokens (see `planeMotion.ts`).
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
 * lifecycle, with the tween shapes, easing, and batch-stagger clamp it drives
 * living in `planeMotion.ts`. The shipped Pixi reconciler is untouched and
 * keeps shipping.
 */
import type { EntityId, PlayerId } from '../protocol';
import type { Rect } from './scene';
import type { PlaneRegion, PlaneRender, RackSlot, StagedPlane, SummaryTileSlot } from './plane';
import {
  applyMotionHint,
  applyRect,
  batchDelay,
  easeOutCubic,
  progress,
  resolvePlaneAnimation,
  sameRect,
  visualRect,
  type EnterTween,
  type FlipTween,
  type GhostTween,
  type PlaneAnimation,
  type PlaneMotionHint,
} from './planeMotion';

export type { PlaneAnimation, PlaneMotionHint } from './planeMotion';

/** Renders a card face into (and re-renders it within) an entity wrapper.
 * `signature` is the "same-looking card" key: equal signatures ⇒ `render` may
 * be skipped, exactly like the Pixi reconciler's visual signature. */
export interface PlaneFaceRenderer {
  /** A stable key of every visual input `render` draws for this entity. */
  signature(render: PlaneRender): string;
  /** Draw (or redraw) the face inside the wrapper element. */
  render(el: HTMLElement, render: PlaneRender): void;
}

/** Options for {@link PlaneReconciler}. Omit `animate` for the snap default. */
export interface PlaneReconcilerOptions {
  /** The face renderer every entity wrapper draws through. */
  face: PlaneFaceRenderer;
  /** Enable the motion layer; `true` for token defaults, a partial to override. */
  animate?: boolean | Partial<PlaneAnimation>;
}

/** What one reconcile actually did — the zero-work-when-nothing-changes gate. */
export interface ReconcileStats {
  /** Entity wrappers created. */
  created: number;
  /** Wrappers re-keyed onto a new representative of the same ×N pile. */
  adopted: number;
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
  adopted: 0,
  updatedFaces: 0,
  moved: 0,
  removed: 0,
  chrome: 0,
  reordered: false,
};

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

/** One cached chrome element (region, crest, zone slot, tile) and its inputs. */
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

  /**
   * Who stood for whom in the previous plane: every folded member mapped to its
   * ×N representative (the representative itself is not an entry). This is what
   * makes pile membership *travel* — a permanent joining a fold flies to the
   * pile it disappears into, one leaving a fold flies out of it, and a
   * representative that departs hands its wrapper to the next member instead of
   * flashing a removal and a fresh entrance in the same rect.
   */
  private membership = new Map<EntityId, EntityId>();

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

    // The incoming plane's fold membership, resolved before any element is
    // touched: which ids it stages, and which permanent each folded member is
    // now represented by. Pile arrivals and departures read off this.
    const nextIds = new Set<EntityId>();
    const nextMembership = new Map<EntityId, EntityId>();
    // Where a departed representative's pile stood, kept for the siblings that
    // unfold in the same view (only the first of them inherits the wrapper).
    const vacated = new Map<EntityId, Rect>();
    for (const render of renders) {
      nextIds.add(render.entityId);
      for (const memberId of render.memberIds) {
        if (memberId !== render.entityId) nextMembership.set(memberId, render.entityId);
      }
    }

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
        continue;
      }

      // A fold whose representative departed continues as its next member: the
      // pile is the same physical object, one card lighter, so its wrapper is
      // re-keyed rather than destroyed and rebuilt in the same rect (which
      // would flash a leave ghost and an entrance fade over each other).
      const previousRep = this.membership.get(render.entityId);
      const inherited =
        previousRep !== undefined && !nextIds.has(previousRep)
          ? this.cards.get(previousRep)
          : undefined;
      if (inherited && previousRep !== undefined) {
        const el = inherited.el;
        el.dataset.entityId = render.entityId;
        if (el.dataset.seat !== render.seat) el.dataset.seat = render.seat;
        if (el.dataset.tier !== render.tier) el.dataset.tier = render.tier;
        const candidate = String(render.candidate);
        if (el.dataset.candidate !== candidate) el.dataset.candidate = candidate;
        if (hint || el.dataset.motion !== undefined) applyMotionHint(el, hint);
        if (inherited.signature !== signature) {
          this.face.render(el, render);
          stats.updatedFaces += 1;
        }
        // Carry any in-flight motion across the swap, so neither the pile's
        // travel nor its entrance restarts under the new key.
        const offset = anim ? this.currentOffset(previousRep, inherited) : null;
        const fromX = inherited.rect.x + (offset?.dx ?? 0);
        const fromY = inherited.rect.y + (offset?.dy ?? 0);
        const enter = this.enters.get(previousRep);
        vacated.set(previousRep, inherited.rect);
        this.cardMoves.delete(previousRep);
        this.enters.delete(previousRep);
        this.cards.delete(previousRep);
        if (enter) this.enters.set(render.entityId, enter);
        this.cards.set(render.entityId, { el, signature, rect: render.rect, seat: render.seat });
        applyRect(el, render.rect);
        if (anim && (fromX !== render.rect.x || fromY !== render.rect.y)) {
          this.startFlip(this.cardMoves, render.entityId, {
            el,
            dx: fromX - render.rect.x,
            dy: fromY - render.rect.y,
            duration: hint?.durationMs ?? anim.travelMs,
            delay: hint?.delayMs ?? 0,
          });
        }
        stats.adopted += 1;
        continue;
      }

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
        // Leaving a fold is travel, not a teleport: a member that unfolds rises
        // out of the pile that was standing for it, not out of the zone piles.
        const leftPile =
          previousRep === undefined
            ? undefined
            : (this.cards.get(previousRep)?.rect ?? vacated.get(previousRep));
        const from =
          routes.get(render.entityId)?.from ?? leftPile ?? this.travelHome(plane, render.seat);
        if (from) this.spawnGhost(el, from, render.rect, false, duration, delay);
        consumedRoutes.add(render.entityId);
      }
    }

    // Retire entities absent from this plane. The wrapper (and with it the hit
    // box) is removed immediately — a leaving card is never addressable — and,
    // when animating, a decorative ghost travels to the seat's zone home (or,
    // for a permanent that only folded away, into the pile now standing for it).
    for (const [entityId, cached] of this.cards) {
      if (present.has(entityId)) continue;
      if (anim) {
        const hint = hints.get(entityId);
        const joinedPile = nextMembership.get(entityId);
        const pileRect = joinedPile === undefined ? undefined : this.targets.get(joinedPile);
        const to =
          routes.get(entityId)?.to ??
          pileRect ??
          this.travelHome(plane, cached.seat) ??
          cached.rect;
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

    this.membership = nextMembership;
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

  /** Current visual rect of reconciled chrome (`crest:<seat>`, `zone:<seat>:<zone>`). */
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
    this.membership.clear();
    this.cardMoves.clear();
    this.chromeMoves.clear();
    this.enters.clear();
    this.ghosts = [];
    this.lastStats = { ...ZERO_STATS };
  }

  // ── internals ─────────────────────────────────────────────────────────────

  /** Reconcile the region slots and chrome anchors (crest, zone rack, tiles). */
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
      // The zone rack: one element per zone anchor, in the fixed §1 order, each
      // at the rect the `zone:<seat>:<zone>` anchor resolves to. A digest rack
      // stages a single button, so it upserts one key and every zone key
      // resolves to it (zone-geography §6.1, §7).
      if (region.rack.variant === 'digest') {
        upsert(
          `rack:${region.seat}`,
          'rack',
          region.rack.bounds,
          rackMeta(region, region.rack.slots),
        );
      } else {
        for (const slot of region.rack.slots) {
          upsert(`zone:${region.seat}:${slot.zone}`, 'zone', slot.rect, zoneSlotMeta(region, slot));
        }
      }
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

    const [kind, id, zone] = ref.split(':', 3);
    if (!id) return undefined;
    const region = planeRegions(plane).find((candidate) => candidate.seat === id);
    const tile = plane.tiles.find((candidate) => candidate.seat === id);
    if (kind === 'seat') return region?.crest ?? tile?.crest;
    if (kind === 'pile') return region?.piles ?? tile?.rect;
    // `zone:<seat>:<zone>` — the §7 resolution order of `zone-geography.md`:
    // exact zone key → the rack union → the seat crest, which is staged at every
    // rung and can never degrade away. A motion is retargeted, never retired.
    if (kind === 'zone') {
      const slot = region?.rack.slots.find((entry) => entry.zone === zone);
      return slot?.hitRect ?? region?.piles ?? tile?.rect ?? region?.crest ?? tile?.crest;
    }
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
    // The applied rect is where the ghost *starts* (its travel is a transform),
    // so the destination is published for inspection: it is what distinguishes
    // travel into a pile from travel to the seat's zone home.
    ghost.dataset.travelTo = `${to.x},${to.y}`;
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

/**
 * One drawn zone slot's non-geometry inputs (`zone-geography.md` §3): which zone
 * it is — the channel the pile's silhouette and material read from, so no two
 * piles look like the same object — its own count, and the seat's eliminated
 * treatment. The count is the pile's **only** home (§4/I5); nothing else in the
 * scene may draw it.
 */
function zoneSlotMeta(region: PlaneRegion, slot: RackSlot): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    zone: slot.zone,
    count: String(slot.count),
    variant: region.rack.variant,
    eliminated: String(region.eliminated),
  };
  // Hidden stays hidden (§I2): the library never publishes a top card. Only the
  // graveyard's is public in the view, and only there does one reach the DOM.
  if (slot.zone === 'graveyard' && region.zones.graveyardTop) {
    meta.top = region.zones.graveyardTop.name;
    meta.topColor = region.zones.graveyardTop.colorIdentity;
  }
  return meta;
}

/** The digest rack button's inputs: every zone's count on one ≥ 44 px target. */
function rackMeta(region: PlaneRegion, slots: readonly RackSlot[]): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    variant: 'digest',
    eliminated: String(region.eliminated),
    zones: slots.map((slot) => slot.zone).join(' '),
  };
  for (const slot of slots) meta[slot.zone] = String(slot.count);
  return meta;
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
