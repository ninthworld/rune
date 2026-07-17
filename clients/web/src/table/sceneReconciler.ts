/**
 * Pixi scene reconciler (issue #58) with an opt-in animate-the-diff layer (issue #334).
 *
 * Keeps the battlefield's Pixi display tree in sync with a stream of
 * {@link TableScene}s **by entity id** instead of tearing it down and rebuilding
 * it on every frame (the old `stage.removeChildren()` + full rebuild). Each card's
 * display object is reused across scenes whenever its visual inputs are unchanged
 * (see {@link cardVisualSignature}); a position-only change moves the existing
 * container rather than reconstructing its ~10 Graphics/Text nodes.
 *
 * The cache is a pure optimization and **never load-bearing state**: after
 * applying any single scene the tree is identical to what a freshly constructed
 * reconciler would produce from that scene alone — the reconnect/replay
 * determinism invariant (clients/web/AGENTS.md, docs/design/ui-requirements.md
 * §11).
 *
 * ## Animate-the-diff (issue #334)
 *
 * Optionally, the reconciler animates the *visual* difference between two scenes:
 * a card that migrates rows/positions eases from its old spot to its new one, an
 * entering card fades up, and a leaving card fades out before it is destroyed. This
 * is layered strictly on top and interpolates between two **authoritative** scenes —
 * it derives nothing and never changes a final layout.
 *
 * Three hard rules keep it from ever gating input (ui-requirements §Performance):
 * 1. **Hit-targets are the scene's job, not ours.** Interactivity lives in the DOM
 *    overlay keyed off `TableScene` rects; a card is addressable the instant its
 *    scene arrives, regardless of where its Pixi visual is mid-tween. This layer
 *    only moves pixels.
 * 2. **Opt-in and reduced-motion-safe.** With no `animate` option (or with
 *    `reducedMotion`), `reconcile` snaps to the final layout exactly as before — no
 *    tween, no alpha, no layout or state difference — so the determinism invariant
 *    and every existing test hold unchanged.
 * 3. **The authoritative target is known immediately.** {@link SceneReconciler.targetFor}
 *    returns a card's final position the moment its scene is reconciled, even while
 *    its visual is still easing there.
 *
 * Animation is driven by {@link SceneReconciler.advance}, called once per frame by
 * the host (the Pixi ticker) with a monotonic timestamp; tests drive it directly
 * with controlled timestamps, so no wall clock or `requestAnimationFrame` is needed.
 *
 * No game logic: the reconciler only reuses/positions the display objects the
 * scene already describes. Legality, cost, and effect are never computed here.
 */
import type { Container } from 'pixi.js';
import { buildCardDisplay, buildChipDisplay, cardVisualSignature } from '../card/cardFactory';
import type { EntityId } from '../protocol';
import type { RenderedCard, TableScene } from './scene';

/** Build a card's display object, dispatching a land chip to the chip renderer. */
function buildDisplay(card: RenderedCard): Container {
  return card.tier === 'chip'
    ? buildChipDisplay(card.data)
    : buildCardDisplay(card.data, card.tier);
}

/** A cached card: the display object plus the signature it was built from. */
interface CachedCard {
  /** The visual-input signature the display currently reflects. */
  signature: string;
  /** The reusable display object parented under the reconciler root. */
  display: Container;
}

/** Resolved animation settings for the animate-the-diff layer (issue #334). */
export interface ReconcilerAnimation {
  /** Milliseconds a move / fade transition runs. */
  duration: number;
  /**
   * Whether reduced motion is requested (`prefers-reduced-motion`). When true, every
   * transition applies instantly: `reconcile` snaps to the final layout with no tween
   * and `advance` is inert — no state or layout differs from the un-animated path.
   */
  reducedMotion: boolean;
}

/** Options for {@link SceneReconciler}. Omit `animate` for the un-animated default. */
export interface ReconcilerOptions {
  /**
   * Enable the animate-the-diff layer (issue #334). `true` uses the defaults; pass a
   * partial to override `duration` / `reducedMotion`. Absent ⇒ no animation (snap),
   * preserving the exact pre-#334 behavior and the determinism invariant.
   */
  animate?: boolean | Partial<ReconcilerAnimation>;
}

const DEFAULT_ANIMATION: ReconcilerAnimation = { duration: 180, reducedMotion: false };

/** Clamp to the unit interval. */
function clamp01(t: number): number {
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

/** Ease-out cubic — a quick start that settles gently, the standard UI decel curve. */
function easeOutCubic(t: number): number {
  const u = 1 - t;
  return 1 - u * u * u;
}

/** Linear interpolation. */
function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** An in-flight position tween for a moved card (issue #334). */
interface MoveTween {
  fromX: number;
  fromY: number;
  toX: number;
  toY: number;
  /** Timestamp of the first `advance` that drove it; set lazily so `reconcile`
   * needs no clock. */
  start?: number;
}

/** An in-flight alpha tween (fade in for entering, fade out for leaving). */
interface FadeTween {
  display: Container;
  from: number;
  to: number;
  start?: number;
}

/**
 * Flatten a scene into the exact draw order a fresh mount uses: every band's
 * cards top-to-bottom, then the hand. Child order is part of the rendered result
 * (z-order), so the reconciler reproduces this order precisely.
 */
function drawOrder(scene: TableScene): RenderedCard[] {
  const cards: RenderedCard[] = [];
  for (const band of scene.bands) for (const card of band.cards) cards.push(card);
  for (const card of scene.hand) cards.push(card);
  return cards;
}

/** Reconciles successive {@link TableScene}s into a single Pixi {@link Container}. */
export class SceneReconciler {
  /** The container this reconciler owns and mutates in place. */
  readonly root: Container;

  /** Display objects currently on stage, keyed by scene entity id. */
  private readonly cache = new Map<EntityId, CachedCard>();

  /** The authoritative final position of each present entity — known immediately at
   * reconcile time, independent of any in-flight visual tween (issue #334). */
  private readonly targets = new Map<EntityId, { x: number; y: number }>();

  /** Animation settings, or `null` when the layer is disabled (snap on reconcile). */
  private readonly animation: ReconcilerAnimation | null;

  /** In-flight position tweens for moved cards, keyed by entity id (issue #334). */
  private readonly moves = new Map<EntityId, MoveTween>();

  /** In-flight fade-in tweens for entering cards, keyed by entity id (issue #334). */
  private readonly fades = new Map<EntityId, FadeTween>();

  /** Leaving cards fading out; destroyed when their fade completes (issue #334). */
  private exits: FadeTween[] = [];

  /**
   * @param root the container to mutate; the caller parents it under the stage.
   * @param options set `animate` to enable the animate-the-diff layer (issue #334).
   */
  constructor(root: Container, options: ReconcilerOptions = {}) {
    this.root = root;
    this.animation = resolveAnimation(options.animate);
  }

  /** Whether transitions should be tweened (enabled and not reduced-motion). */
  private get animating(): boolean {
    return this.animation !== null && !this.animation.reducedMotion;
  }

  /**
   * Bring {@link root} in line with `scene`: add new entities, rebuild those whose
   * visual inputs changed, move those that only shifted position, and remove those
   * that are gone. Without animation, on return the tree is identical to a fresh
   * mount of `scene`. With animation, every card's **authoritative** target is set
   * immediately (see {@link targetFor}); only its Pixi visual eases there over the
   * next frames, driven by {@link advance}.
   */
  reconcile(scene: TableScene): void {
    const cards = drawOrder(scene);
    const present = new Set<EntityId>();
    const ordered: Container[] = [];
    const animating = this.animating;

    for (const card of cards) {
      present.add(card.entityId);
      const signature = cardVisualSignature(card.data, card.tier);
      const cached = this.cache.get(card.entityId);

      let display: Container;
      if (cached && cached.signature === signature) {
        // Visual inputs unchanged — reuse the existing display object.
        display = cached.display;
        const moved = display.position.x !== card.rect.x || display.position.y !== card.rect.y;
        if (animating && moved) {
          // Ease from wherever it currently renders (possibly mid-tween) to the new
          // spot; do not snap. The authoritative target is recorded below regardless.
          this.moves.set(card.entityId, {
            fromX: display.position.x,
            fromY: display.position.y,
            toX: card.rect.x,
            toY: card.rect.y,
          });
        } else {
          display.position.set(card.rect.x, card.rect.y);
          this.moves.delete(card.entityId);
        }
      } else {
        // New entity, or its look changed: build fresh and retire any stale one.
        display = buildDisplay(card);
        if (cached) this.retire(cached.display);
        this.cache.set(card.entityId, { signature, display });
        // Enter at the final position — always addressable there immediately — and,
        // when animating, fade the pixels up so the arrival reads as motion.
        display.position.set(card.rect.x, card.rect.y);
        this.moves.delete(card.entityId);
        if (animating) {
          display.alpha = 0;
          this.fades.set(card.entityId, { display, from: 0, to: 1 });
        }
      }
      this.targets.set(card.entityId, { x: card.rect.x, y: card.rect.y });
      ordered.push(display);
    }

    // Retire entities absent from this scene (fade + destroy when animating).
    for (const [entityId, cached] of this.cache) {
      if (present.has(entityId)) continue;
      this.retire(cached.display);
      this.cache.delete(entityId);
      this.targets.delete(entityId);
      this.moves.delete(entityId);
      this.fades.delete(entityId);
    }

    // Reassert draw order to match a fresh mount. Leaving cards mid-fade sit beneath
    // the live set so an incoming card is never occluded by one on its way out.
    this.root.removeChildren();
    for (const exit of this.exits) this.root.addChild(exit.display);
    for (const display of ordered) this.root.addChild(display);
  }

  /**
   * Retire a display object: fade it out then destroy it when animating, or remove
   * and destroy it immediately otherwise. A retired object is dropped from the cache
   * by the caller, so it is no longer addressable — hit-targets never point at a card
   * on its way out (issue #334).
   */
  private retire(display: Container): void {
    if (this.animating) {
      this.exits.push({ display, from: display.alpha, to: 0 });
    } else {
      this.root.removeChild(display);
      display.destroy({ children: true });
    }
  }

  /**
   * Advance every in-flight transition to time `now` (a monotonic millisecond
   * timestamp). Called once per frame by the host ticker; a no-op when animation is
   * disabled or nothing is in flight. Interactivity never waits on this — it only
   * moves pixels toward layouts the scene already made authoritative.
   */
  advance(now: number): void {
    if (this.animation === null) return;
    const { duration } = this.animation;

    for (const [id, tween] of this.moves) {
      const display = this.cache.get(id)?.display;
      if (!display) {
        this.moves.delete(id);
        continue;
      }
      if (tween.start === undefined) tween.start = now;
      const p = clamp01((now - tween.start) / duration);
      const e = easeOutCubic(p);
      display.position.set(lerp(tween.fromX, tween.toX, e), lerp(tween.fromY, tween.toY, e));
      if (p >= 1) this.moves.delete(id);
    }

    for (const [id, tween] of this.fades) {
      if (tween.start === undefined) tween.start = now;
      const p = clamp01((now - tween.start) / duration);
      tween.display.alpha = lerp(tween.from, tween.to, easeOutCubic(p));
      if (p >= 1) {
        tween.display.alpha = tween.to;
        this.fades.delete(id);
      }
    }

    this.exits = this.exits.filter((tween) => {
      if (tween.start === undefined) tween.start = now;
      const p = clamp01((now - tween.start) / duration);
      tween.display.alpha = lerp(tween.from, tween.to, easeOutCubic(p));
      if (p >= 1) {
        this.root.removeChild(tween.display);
        tween.display.destroy({ children: true });
        return false;
      }
      return true;
    });
  }

  /** Whether any transition is still in flight (issue #334); false when un-animated. */
  hasPendingAnimations(): boolean {
    return this.moves.size > 0 || this.fades.size > 0 || this.exits.length > 0;
  }

  /** The cached display object for an entity, if one is currently on stage. A card on
   * its way out is not cached, so this never returns a leaving object. */
  displayFor(entityId: EntityId): Container | undefined {
    return this.cache.get(entityId)?.display;
  }

  /**
   * The **authoritative** final position of an entity from the latest reconcile —
   * available immediately, even while its visual is still easing there (issue #334).
   * A hit-test that used the reconciler (the DOM overlay uses scene rects directly)
   * would read this, so interactivity is never gated on an animation settling.
   */
  targetFor(entityId: EntityId): { x: number; y: number } | undefined {
    return this.targets.get(entityId);
  }

  /** Detach and destroy every cached and in-flight display, emptying the root. */
  clear(): void {
    for (const cached of this.cache.values()) {
      this.root.removeChild(cached.display);
      cached.display.destroy({ children: true });
    }
    for (const exit of this.exits) {
      this.root.removeChild(exit.display);
      exit.display.destroy({ children: true });
    }
    this.cache.clear();
    this.targets.clear();
    this.moves.clear();
    this.fades.clear();
    this.exits = [];
  }
}

/** Resolve the `animate` option into concrete settings, or `null` when disabled. */
function resolveAnimation(animate: ReconcilerOptions['animate']): ReconcilerAnimation | null {
  if (!animate) return null;
  if (animate === true) return { ...DEFAULT_ANIMATION };
  return { ...DEFAULT_ANIMATION, ...animate };
}
