/**
 * Renderer-level tests for the combat-link overlay (issue #339): a link is present
 * for a mid-combat board, focus isolates one participant's links, and endpoints track
 * across a reconcile. Drives the real {@link SceneReconciler} over a real Pixi
 * {@link Container} (jsdom), asserting the observable drawn-link count rather than pixels.
 */
import { describe, expect, it } from 'vitest';
import { Container } from 'pixi.js';
import { normalizeGameView } from '../wire';
import { buildTableScene, defaultSceneGeometry } from './scene';
import type { GameView } from '../protocol';
import { SceneReconciler } from './sceneReconciler';

/** A creature permanent on the battlefield. */
interface Perm {
  id: string;
  controller: string;
  attacking?: boolean;
  blocking?: string;
}

/** A mid-combat view: `p1` local, holding the given permanents with combat state. */
function combatView(perms: Perm[]): GameView {
  return normalizeGameView({
    you: 'p1',
    my_hand: [],
    opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
    battlefield: perms.map((p) => ({
      id: p.id,
      controller: p.controller,
      owner: p.controller,
      attacking: p.attacking,
      blocking: p.blocking,
      card: {
        id: p.id,
        name: p.id,
        type_line: 'Creature — Beast',
        power: '2',
        toughness: '2',
      },
    })),
    phase: 'declare_blockers',
    valid_actions: [],
  });
}

/** One attacker (p2) blocked by two of the local player's creatures. */
function twoBlockersOnOneAttacker(): GameView {
  return combatView([
    { id: 'atk', controller: 'p2', attacking: true },
    { id: 'blkA', controller: 'p1', blocking: 'atk' },
    { id: 'blkB', controller: 'p1', blocking: 'atk' },
  ]);
}

describe('combat-link overlay (issue #339)', () => {
  it('draws a link for each declared blocker on a mid-combat board', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(
      buildTableScene(twoBlockersOnOneAttacker(), undefined, defaultSceneGeometry()),
    );
    expect(reconciler.drawnLinkCount()).toBe(2);
  });

  it('draws no links outside combat', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(
      buildTableScene(
        combatView([{ id: 'x', controller: 'p1' }]),
        undefined,
        defaultSceneGeometry(),
      ),
    );
    expect(reconciler.drawnLinkCount()).toBe(0);
  });

  it('isolates a focused participant’s links', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(
      buildTableScene(twoBlockersOnOneAttacker(), undefined, defaultSceneGeometry()),
    );

    // Focusing one blocker isolates its single link.
    reconciler.setIsolation('blkA');
    expect(reconciler.drawnLinkCount()).toBe(1);
    // Focusing the shared attacker shows both of its links.
    reconciler.setIsolation('atk');
    expect(reconciler.drawnLinkCount()).toBe(2);
    // A non-participant isolates nothing.
    reconciler.setIsolation('someone-else');
    expect(reconciler.drawnLinkCount()).toBe(0);
    // Clearing isolation restores every link.
    reconciler.setIsolation(null);
    expect(reconciler.drawnLinkCount()).toBe(2);
  });

  it('keeps links only for endpoints still present after a reconcile', () => {
    const reconciler = new SceneReconciler(new Container());
    reconciler.reconcile(
      buildTableScene(twoBlockersOnOneAttacker(), undefined, defaultSceneGeometry()),
    );
    expect(reconciler.drawnLinkCount()).toBe(2);

    // One blocker leaves combat: the next view carries only the other link.
    reconciler.reconcile(
      buildTableScene(
        combatView([
          { id: 'atk', controller: 'p2', attacking: true },
          { id: 'blkA', controller: 'p1', blocking: 'atk' },
        ]),
        undefined,
        defaultSceneGeometry(),
      ),
    );
    expect(reconciler.drawnLinkCount()).toBe(1);
  });
});
