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
import { Graphics } from 'pixi.js';
import type { Container } from 'pixi.js';
import { buildCardDisplay, buildChipDisplay, cardVisualSignature } from '../card/cardFactory';
import type { EntityId } from '../protocol';
import type { CombatLink, RenderedCard, Rect, TableScene } from './scene';
import {
  doubledStroke,
  linkAlpha,
  positionLinks,
  selectVisibleLinks,
  type PositionedLink,
} from './combatLinks';
import { COMBAT_LINK } from '../tokens';

/** Parse a `#rrggbb` token to the numeric form Pixi wants. */
function hexColor(hex: string): number {
  return parseInt(hex.replace('#', ''), 16);
}

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

  /** The combat-link overlay (issue #339): a single Graphics layer, kept on top of the
   * cards, that draws each blocker→attacker connector. Redrawn on reconcile and — while
   * a view-diff animation is in flight — each frame, so links track their endpoints. */
  private readonly linkLayer = new Graphics();

  /** The blocker→attacker links of the latest scene (issue #339). */
  private combatLinks: CombatLink[] = [];

  /** Each present card's footprint, for locating a link endpoint's centre. */
  private readonly rects = new Map<EntityId, Rect>();

  /** The focused/selected/hovered participant whose links are isolated on a crowded
   * board, or `null` to draw every link (issue #339). Ephemeral presentation. */
  private isolatedId: EntityId | null = null;

  /**
   * @param root the container to mutate; the caller parents it under the stage.
   * @param options set `animate` to enable the animate-the-diff layer (issue #334).
   */
  constructor(root: Container, options: ReconcilerOptions = {}) {
    this.root = root;
    this.animation = resolveAnimation(options.animate);
    // The link overlay is passive: it never intercepts pointer input, so a combat link
    // can never become a hit-target or delay a live prompt (issue #339).
    this.linkLayer.eventMode = 'none';
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
    this.combatLinks = scene.combatLinks;
    this.rects.clear();

    for (const card of cards) {
      present.add(card.entityId);
      this.rects.set(card.entityId, card.rect);
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
      // The scene's card rects already carry the scaled footprints; scaling the
      // display object makes the drawn pixels match them. Applied unconditionally
      // (like position) so a reused display tracks a scale change across scenes.
      display.scale.set(scene.scale ?? 1);
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
    // the live set so an incoming card is never occluded by one on its way out. The
    // combat-link overlay sits on top of every card so links are never occluded.
    this.root.removeChildren();
    for (const exit of this.exits) this.root.addChild(exit.display);
    for (const display of ordered) this.root.addChild(display);
    // The link overlay is parented (on top) only when there are links to draw, so a
    // board with no combat carries no extra display node — a fresh mount of a
    // link-free scene is byte-identical to before this layer existed.
    if (this.combatLinks.length > 0) this.root.addChild(this.linkLayer);
    this.drawLinks();
  }

  /**
   * Isolate the combat links of a focused/selected/hovered participant (issue #339):
   * on a crowded board only that object's links draw, instead of every line at once.
   * `null` clears the isolation (all links draw). Ephemeral presentation — it only
   * redraws the overlay, never a scene or hit-target.
   */
  setIsolation(entityId: EntityId | null): void {
    if (this.isolatedId === entityId) return;
    this.isolatedId = entityId;
    this.drawLinks();
  }

  /** The current on-screen centre of a card, from wherever it renders *now* (possibly
   * mid-tween), or `undefined` if it is not on stage. Lets links track their endpoints
   * during a view-diff animation (issue #334/#339). */
  private centerOf(id: EntityId): { x: number; y: number } | undefined {
    const display = this.cache.get(id)?.display;
    const rect = this.rects.get(id);
    if (!display || !rect) return undefined;
    return { x: display.position.x + rect.w / 2, y: display.position.y + rect.h / 2 };
  }

  /** Redraw the combat-link overlay from the current card positions and isolation.
   * Passive pixels only — the layer has no hit area and never intercepts input. */
  private drawLinks(): void {
    this.linkLayer.clear();
    if (this.combatLinks.length === 0) return;
    const visible = selectVisibleLinks(this.combatLinks, this.isolatedId);
    const alpha = linkAlpha(this.combatLinks.length, this.isolatedId);
    const positioned = positionLinks(visible, (id) => this.centerOf(id));
    for (const pl of positioned) this.drawLink(pl, alpha);
  }

  /** Draw one positioned link as a doubled stroke with a node at the blocker end. */
  private drawLink(pl: PositionedLink, alpha: number): void {
    const color = hexColor(COMBAT_LINK.color);
    this.linkLayer.lineStyle({ width: COMBAT_LINK.strokeWidth, color, alpha });
    for (const [a, b] of doubledStroke(pl.from, pl.to)) {
      this.linkLayer.moveTo(a.x, a.y);
      this.linkLayer.lineTo(b.x, b.y);
    }
    // A small node at the blocker end marks the link's direction (blocker → attacker).
    this.linkLayer.lineStyle(0);
    this.linkLayer.beginFill(color, alpha);
    this.linkLayer.drawCircle(pl.from.x, pl.from.y, COMBAT_LINK.nodeRadius);
    this.linkLayer.endFill();
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
    const hadMoves = this.moves.size > 0;

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

    // Redraw the combat links from the moved positions so they track their endpoints
    // while cards ease to new spots (issue #339); only while cards are actually moving.
    if (hadMoves && this.combatLinks.length > 0) this.drawLinks();
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
    this.linkLayer.clear();
    this.root.removeChild(this.linkLayer);
    this.cache.clear();
    this.targets.clear();
    this.moves.clear();
    this.fades.clear();
    this.exits = [];
    this.rects.clear();
    this.combatLinks = [];
  }

  /** How many combat links are currently drawn — the visible set after isolation
   * filtering and endpoint resolution (issue #339). For renderer-level tests. */
  drawnLinkCount(): number {
    const visible = selectVisibleLinks(this.combatLinks, this.isolatedId);
    return positionLinks(visible, (id) => this.centerOf(id)).length;
  }
}

/** Resolve the `animate` option into concrete settings, or `null` when disabled. */
function resolveAnimation(animate: ReconcilerOptions['animate']): ReconcilerAnimation | null {
  if (!animate) return null;
  if (animate === true) return { ...DEFAULT_ANIMATION };
  return { ...DEFAULT_ANIMATION, ...animate };
}
