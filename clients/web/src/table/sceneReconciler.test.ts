import { describe, expect, it } from 'vitest';
import { Container, Text } from 'pixi.js';
import type { CardDisplayData } from '../card/cardFactory';
import { normalizeGameView } from '../wire';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import { buildTableScene, defaultSceneGeometry } from './scene';
import type { RenderedCard, TableScene } from './scene';
import { SceneReconciler } from './sceneReconciler';

/**
 * A structural snapshot of a display tree: node type, transform, and (for text)
 * content + fill, recursively. Two trees with equal snapshots render identically,
 * so this is how the tests assert the reconcile cache is never load-bearing —
 * an incrementally-reconciled tree must match a fresh mount byte for byte.
 */
interface Snap {
  type: string;
  x: number;
  y: number;
  rotation: number;
  alpha: number;
  pivotX: number;
  pivotY: number;
  text?: string;
  fill?: unknown;
  children: Snap[];
}

function snapshot(node: Container): Snap {
  const snap: Snap = {
    type: node.constructor.name,
    x: node.position.x,
    y: node.position.y,
    rotation: node.rotation,
    alpha: node.alpha,
    pivotX: node.pivot.x,
    pivotY: node.pivot.y,
    children: node.children.map((child) => snapshot(child as Container)),
  };
  if (node instanceof Text) {
    snap.text = node.text;
    snap.fill = node.style.fill;
  }
  return snap;
}

const bears: CardDisplayData = {
  name: 'Grizzly Bears',
  typeLine: 'Creature — Bear',
  colorIdentity: 'G',
  manaCost: '{1}{G}',
  power: '2',
  toughness: '2',
};

const elves: CardDisplayData = {
  name: 'Llanowar Elves',
  typeLine: 'Creature — Elf Druid',
  colorIdentity: 'G',
  manaCost: '{G}',
  power: '1',
  toughness: '1',
};

/** A single battlefield card at a position, with everything else defaulted. */
function card(
  entityId: string,
  data: CardDisplayData,
  x: number,
  y: number,
  tier: RenderedCard['tier'] = 'field',
): RenderedCard {
  return {
    entityId,
    zone: 'battlefield',
    tier,
    name: data.name,
    data,
    rect: { x, y, w: 84, h: 118 },
    memberIds: [entityId],
    stackCount: 1,
    actions: [],
    targetable: false,
    chosen: false,
  };
}

/** A one-band scene (plus optional hand) for driving the reconciler directly. */
function scene(cards: RenderedCard[], hand: RenderedCard[] = []): TableScene {
  return {
    width: 600,
    height: 400,
    bands: [
      {
        playerId: 'p1',
        isLocal: true,
        cards,
        rows: [],
        rect: { x: 0, y: 0, w: 600, h: 160 },
        headerRect: { x: 0, y: 0, w: 600, h: 32 },
        label: 'p1 (you)',
        isEmpty: cards.length === 0,
        zones: { library: 0, graveyard: 0, exile: 0 },
        accent: '#3E9C9C',
        pileRect: { x: 520, y: 40, w: 68, h: 120 },
        densityRung: 0,
        summary: false,
      },
    ],
    hand,
    handRegion: { rect: { x: 0, y: 160, w: 600, h: 140 }, label: 'Your hand' },
    localPlayerId: 'p1',
    combatLinks: [],
    attackTargets: [],
  };
}

/** Reconcile a scene into a brand-new reconciler and return its root. */
function freshMount(s: TableScene): Container {
  const reconciler = new SceneReconciler(new Container());
  reconciler.reconcile(s);
  return reconciler.root;
}

describe('SceneReconciler add/update/move/remove', () => {
  it('adds a display object per scene entity, in draw order', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0), card('b', elves, 100, 0)]));

    expect(reconciler.root.children).toHaveLength(2);
    expect(reconciler.displayFor('a')).toBeDefined();
    expect(reconciler.displayFor('b')).toBeDefined();
    // Draw order matches flatten order (band cards, then hand).
    expect(reconciler.root.children[0]).toBe(reconciler.displayFor('a'));
    expect(reconciler.root.children[1]).toBe(reconciler.displayFor('b'));
  });

  it('reuses the same container for a position-only change (move, not rebuild)', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0)]));
    const before = reconciler.displayFor('a');

    reconciler.reconcile(scene([card('a', bears, 240, 60)]));
    const after = reconciler.displayFor('a');

    expect(after).toBe(before); // same instance: no ~10-node rebuild
    expect(after?.position.x).toBe(240);
    expect(after?.position.y).toBe(60);
  });

  it('rebuilds the container when a visual input changes', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0)]));
    const before = reconciler.displayFor('a');

    // Power/toughness is a visual input, so the display must be reconstructed.
    reconciler.reconcile(scene([card('a', { ...bears, power: '3', toughness: '3' }, 0, 0)]));
    const after = reconciler.displayFor('a');

    expect(after).not.toBe(before);
    const labels: string[] = [];
    const walk = (n: Container): void => {
      for (const child of n.children) {
        if (child instanceof Text) labels.push(child.text);
        walk(child as Container);
      }
    };
    walk(reconciler.root);
    expect(labels).toContain('3/3');
    expect(labels).not.toContain('2/2');
  });

  it('removes entities that leave the scene and keeps the rest', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0), card('b', elves, 100, 0)]));
    const keptA = reconciler.displayFor('a');

    reconciler.reconcile(scene([card('a', bears, 0, 0)]));

    expect(reconciler.root.children).toHaveLength(1);
    expect(reconciler.displayFor('b')).toBeUndefined();
    expect(reconciler.displayFor('a')).toBe(keptA); // survivor reused, not rebuilt
  });

  it('reasserts draw order when a new entity is inserted before an existing one', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('b', elves, 100, 0)]));
    reconciler.reconcile(scene([card('a', bears, 0, 0), card('b', elves, 100, 0)]));

    expect(reconciler.root.children[0]).toBe(reconciler.displayFor('a'));
    expect(reconciler.root.children[1]).toBe(reconciler.displayFor('b'));
  });

  it('clear() empties the root and drops the cache', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0)]));
    reconciler.clear();

    expect(reconciler.root.children).toHaveLength(0);
    expect(reconciler.displayFor('a')).toBeUndefined();
  });
});

describe('SceneReconciler determinism invariant (fresh-mount equivalence)', () => {
  it('matches a fresh mount after a position-only move', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0)]));
    const target = scene([card('a', bears, 240, 60)]);
    reconciler.reconcile(target);

    expect(snapshot(reconciler.root)).toEqual(snapshot(freshMount(target)));
  });

  it('matches a fresh mount after a visual change (no residue of the old look)', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0)]));
    const target = scene([card('a', { ...bears, tapped: true, power: '5', toughness: '5' }, 0, 0)]);
    reconciler.reconcile(target);

    expect(snapshot(reconciler.root)).toEqual(snapshot(freshMount(target)));
  });

  it('matches a fresh mount after an add + remove + reorder', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(scene([card('a', bears, 0, 0), card('b', elves, 100, 0)]));
    const target = scene([card('c', elves, 0, 0), card('a', bears, 100, 0)]);
    reconciler.reconcile(target);

    expect(snapshot(reconciler.root)).toEqual(snapshot(freshMount(target)));
  });

  it('stays fresh-mount-identical across a sequence of real GameView scenes', () => {
    // Distinct frames exercising add, move, tap toggle, counter change, hand, and
    // wholesale replacement. At every step the incrementally-reconciled tree must
    // equal a fresh mount of that same scene — the reconcile cache proves inert.
    const frames = [
      SAMPLE_GAME_VIEW,
      normalizeGameView({
        you: 'p1',
        my_hand: [{ id: 'c1', name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' }],
        opponents: [{ player_id: 'p2', hand_size: 7, life: 20, library_size: 53 }],
        battlefield: [
          {
            id: 'perm_xyz',
            controller: 'p1',
            owner: 'p1',
            card: {
              id: 'perm_xyz',
              name: 'Grizzly Bears',
              type_line: 'Creature — Bear',
              mana_cost: '{1}{G}',
              power: '2',
              toughness: '2',
            },
          },
        ],
        phase: 'declare_attackers',
        valid_actions: [],
      }),
      normalizeGameView({
        you: 'p1',
        my_hand: [],
        opponents: [{ player_id: 'p2', hand_size: 4, life: 12, library_size: 40 }],
        battlefield: [
          {
            id: 'perm_new',
            controller: 'p2',
            owner: 'p2',
            card: { id: 'perm_new', name: 'Island', type_line: 'Basic Land — Island' },
          },
        ],
        phase: 'end',
        valid_actions: [],
      }),
    ];

    const reconciler = new SceneReconciler(new Container());
    for (const view of frames) {
      const s = buildTableScene(view, undefined, defaultSceneGeometry());
      reconciler.reconcile(s);
      expect(snapshot(reconciler.root)).toEqual(snapshot(freshMount(s)));
    }
  });
});

describe('SceneReconciler animate-the-diff (issue #334)', () => {
  it('eases a moved card to its new spot without snapping, and settles exactly there', () => {
    const r = new SceneReconciler(new Container(), { animate: true });
    r.reconcile(scene([card('a', bears, 0, 0)]));
    r.reconcile(scene([card('a', bears, 240, 60)]));
    const a = r.displayFor('a')!;

    // The authoritative target is known immediately (a hit-test never waits on the
    // tween), while the visual is still at its old spot.
    expect(r.targetFor('a')).toEqual({ x: 240, y: 60 });
    expect([a.position.x, a.position.y]).toEqual([0, 0]);

    r.advance(0); // start the clock
    r.advance(90); // halfway
    expect(a.position.x).toBeGreaterThan(0);
    expect(a.position.x).toBeLessThan(240);

    r.advance(180); // done
    expect([a.position.x, a.position.y]).toEqual([240, 60]);
    expect(r.hasPendingAnimations()).toBe(false);
  });

  it('reduced motion snaps to the final layout with no tween or fade', () => {
    const r = new SceneReconciler(new Container(), { animate: { reducedMotion: true } });
    r.reconcile(scene([card('a', bears, 0, 0)]));
    r.reconcile(scene([card('a', bears, 240, 60)]));
    const a = r.displayFor('a')!;

    expect([a.position.x, a.position.y]).toEqual([240, 60]);
    expect(a.alpha).toBe(1);
    expect(r.hasPendingAnimations()).toBe(false);
    // advance is inert under reduced motion.
    r.advance(1000);
    expect([a.position.x, a.position.y]).toEqual([240, 60]);
  });

  it('fades an entering card up from zero, addressable at its final spot at once', () => {
    const r = new SceneReconciler(new Container(), { animate: true });
    r.reconcile(scene([card('a', bears, 10, 20)]));
    const a = r.displayFor('a')!;

    // Immediately addressable at the final position even though the pixels fade in.
    expect(r.targetFor('a')).toEqual({ x: 10, y: 20 });
    expect([a.position.x, a.position.y]).toEqual([10, 20]);
    expect(a.alpha).toBe(0);

    r.advance(0);
    r.advance(180);
    expect(a.alpha).toBe(1);
  });

  it('lets a leaving card fade out — not addressable mid-exit — then destroys it', () => {
    const r = new SceneReconciler(new Container(), { animate: true });
    r.reconcile(scene([card('a', bears, 0, 0), card('b', elves, 100, 0)]));
    r.advance(0);
    r.advance(180); // settle the entrance fades

    r.reconcile(scene([card('a', bears, 0, 0)])); // b leaves
    // b is dropped from the cache the instant it leaves, so it is no longer a
    // hit-target, even while its visual lingers fading out.
    expect(r.displayFor('b')).toBeUndefined();
    expect(r.targetFor('b')).toBeUndefined();
    expect(r.root.children).toHaveLength(2);
    expect(r.hasPendingAnimations()).toBe(true);

    r.advance(1000);
    r.advance(1180); // fade complete
    expect(r.root.children).toHaveLength(1);
    expect(r.hasPendingAnimations()).toBe(false);
  });

  it('matches a fresh mount once every transition settles (final layout unchanged)', () => {
    const r = new SceneReconciler(new Container(), { animate: true });
    r.reconcile(scene([card('a', bears, 0, 0)]));
    const target = scene([card('a', bears, 240, 60)]);
    r.reconcile(target);
    r.advance(0);
    r.advance(180);

    expect(snapshot(r.root)).toEqual(snapshot(freshMount(target)));
  });

  it('keeps an entering card immediately addressable while an outgoing one still fades', () => {
    const r = new SceneReconciler(new Container(), { animate: true });
    r.reconcile(scene([card('a', bears, 0, 0)]));
    r.advance(0);
    r.advance(180);

    // a leaves and c enters in the same diff.
    r.reconcile(scene([card('c', elves, 0, 0)]));
    // The entering card is live at its final spot at once; the leaving one is gone
    // from the addressable set even though its pixels are still on screen.
    expect(r.displayFor('c')).toBeDefined();
    expect(r.targetFor('c')).toEqual({ x: 0, y: 0 });
    expect(r.displayFor('a')).toBeUndefined();
  });
});
