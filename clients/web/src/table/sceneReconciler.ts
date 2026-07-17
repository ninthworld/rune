/**
 * Pixi scene reconciler (issue #58).
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
 * §11). It is also the intended attachment point for future animate-the-diff
 * transitions; animations themselves ship separately (do not add them here).
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

  /** @param root the container to mutate; the caller parents it under the stage. */
  constructor(root: Container) {
    this.root = root;
  }

  /**
   * Bring {@link root} in line with `scene`: add new entities, rebuild those whose
   * visual inputs changed, move those that only shifted position, and remove those
   * that are gone. On return the tree is identical to a fresh mount of `scene`.
   */
  reconcile(scene: TableScene): void {
    const cards = drawOrder(scene);
    const present = new Set<EntityId>();
    const ordered: Container[] = [];

    for (const card of cards) {
      present.add(card.entityId);
      const signature = cardVisualSignature(card.data, card.tier);
      const cached = this.cache.get(card.entityId);

      let display: Container;
      if (cached && cached.signature === signature) {
        // Visual inputs unchanged — reuse the existing display object.
        display = cached.display;
      } else {
        // New entity, or its look changed: build fresh and drop any stale one.
        display = buildDisplay(card);
        if (cached) {
          this.root.removeChild(cached.display);
          cached.display.destroy({ children: true });
        }
        this.cache.set(card.entityId, { signature, display });
      }
      display.position.set(card.rect.x, card.rect.y);
      ordered.push(display);
    }

    // Retire entities absent from this scene.
    for (const [entityId, cached] of this.cache) {
      if (present.has(entityId)) continue;
      this.root.removeChild(cached.display);
      cached.display.destroy({ children: true });
      this.cache.delete(entityId);
    }

    // Reassert draw order to match a fresh mount. `removeChildren` only detaches;
    // re-adding reused objects reparents them into the scene's order.
    this.root.removeChildren();
    for (const display of ordered) this.root.addChild(display);
  }

  /** The cached display object for an entity, if one is currently on stage. */
  displayFor(entityId: EntityId): Container | undefined {
    return this.cache.get(entityId)?.display;
  }

  /** Detach and destroy every cached display, emptying the root. */
  clear(): void {
    for (const cached of this.cache.values()) {
      this.root.removeChild(cached.display);
      cached.display.destroy({ children: true });
    }
    this.cache.clear();
  }
}
