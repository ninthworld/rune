import { describe, expect, it } from 'vitest';
import { Container, Text } from 'pixi.js';
import type { CardDisplayData } from '../card/cardFactory';
import { normalizeGameView } from '../wire';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';
import { buildTableScene } from './scene';
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
        label: 'p1 (you)',
        isEmpty: cards.length === 0,
        zones: { library: 0, graveyard: 0, exile: 0 },
      },
    ],
    hand,
    handRegion: { rect: { x: 0, y: 160, w: 600, h: 140 }, label: 'Your hand' },
    localPlayerId: 'p1',
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
      const s = buildTableScene(view);
      reconciler.reconcile(s);
      expect(snapshot(reconciler.root)).toEqual(snapshot(freshMount(s)));
    }
  });
});
