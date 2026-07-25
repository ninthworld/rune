/**
 * §10.3 / implementation note IN2 — the staging adapter that gives
 * `PersistentEffect.edge` a production caller.
 *
 * The single question these gates hold the adapter to is the one that decides
 * between an edge indicator and retirement: **does the endpoint still exist in
 * the authoritative view?** Getting it wrong in the "occluded" direction leaves
 * paths pointing at nothing forever, so the vanished cases are asserted as
 * hard as the occluded ones.
 */
import { describe, expect, it } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView, Permanent } from '../../protocol';
import { SCENE_HUES, SCENE_NEUTRALS } from '../../sceneTokens';
import type { PersistentEffect } from '../effects';
import { stagePlane, type StagedPlane } from '../plane';
import type { Rect } from '../scene';
import {
  attachOccludedEndpoints,
  endpointPresentInView,
  occlusionContainer,
} from './endpointOcclusion';

const VIEWPORT = { width: 1440, height: 900 };

function permanent(id: string, controller = 'p1'): Permanent {
  return {
    id,
    controller,
    owner: controller,
    card: { id, name: `Card ${id}`, type_line: 'Creature', power: '1', toughness: '1' },
  };
}

function view(): GameView {
  return structuredClone(SAMPLE_GAME_VIEW);
}

function plane(of: GameView): StagedPlane {
  return stagePlane(of, VIEWPORT);
}

const path = (over: Partial<PersistentEffect> = {}): PersistentEffect => ({
  id: 'target:a',
  category: 'targeting-path',
  from: { ref: 'attacker' },
  to: { ref: 'victim' },
  accent: SCENE_HUES.orange.value,
  state: 'confirmed',
  ...over,
});

/** A rect source that knows about `attacker` and nothing else by default. */
function rects(extra: Record<string, Rect> = {}): (ref: string) => Rect | undefined {
  const map = new Map<string, Rect>([
    ['attacker', { x: 200, y: 500, w: 66, h: 92 }],
    ...Object.entries(extra),
  ]);
  return (ref) => map.get(ref);
}

describe('endpointPresentInView — membership in the server’s own lists', () => {
  it('reads each anchor shape against the list that owns it', () => {
    const current = view();
    current.battlefield = [permanent('on-board')];
    current.stack = [{ id: 'spell', controller: 'p1', description: 'A spell.' }];
    expect(endpointPresentInView(current, 'on-board')).toBe(true);
    expect(endpointPresentInView(current, 'stack:spell')).toBe(true);
    expect(endpointPresentInView(current, `seat:${current.you}`)).toBe(true);
    expect(endpointPresentInView(current, 'seat:p2')).toBe(true);
  });

  it('answers false for anything the view no longer lists', () => {
    const current = view();
    current.battlefield = [];
    current.stack = [];
    expect(endpointPresentInView(current, 'on-board')).toBe(false);
    expect(endpointPresentInView(current, 'stack:spell')).toBe(false);
    expect(endpointPresentInView(current, 'seat:nobody')).toBe(false);
    // An anchor shape this module does not model keeps the carried behaviour.
    expect(endpointPresentInView(current, 'pile:p1')).toBe(false);
  });
});

describe('attachOccludedEndpoints — occluded and clipped, never vanished', () => {
  it('attaches a container to an endpoint the view lists but the plane did not draw', () => {
    const current = view();
    current.battlefield = [permanent('attacker'), permanent('victim', 'p2')];
    const [staged] = attachOccludedEndpoints([path()], {
      view: current,
      plane: plane(current),
      rectFor: rects(),
    });
    // Still on the battlefield, simply not staged at this rung: the path
    // terminates on the plane's edge with an indicator rather than vanishing.
    expect(staged!.edge).toEqual({ x: 0, y: 0, w: VIEWPORT.width, h: VIEWPORT.height });
  });

  it('sends a scrolled-out stack entry’s indicator to the stage’s own rail', () => {
    const current = view();
    current.battlefield = [permanent('attacker')];
    current.stack = [{ id: 'deep', controller: 'p1', description: 'Buried.' }];
    const staged = plane(current);
    const [effect] = attachOccludedEndpoints([path({ to: { ref: 'stack:deep' } })], {
      view: current,
      plane: staged,
      rectFor: rects(),
    });
    expect(effect!.edge).toEqual(staged.corridor);
    expect(occlusionContainer('stack:deep', staged)).toEqual(staged.corridor);
  });

  it('attaches the viewport to an endpoint drawn OFF the plane (clipped)', () => {
    const current = view();
    current.battlefield = [permanent('attacker'), permanent('victim', 'p2')];
    const [staged] = attachOccludedEndpoints([path()], {
      view: current,
      plane: plane(current),
      rectFor: rects({ victim: { x: 400, y: -800, w: 66, h: 92 } }),
    });
    expect(staged!.edge).toEqual({ x: 0, y: 0, w: VIEWPORT.width, h: VIEWPORT.height });
  });

  it('leaves a VANISHED endpoint bare, so it keeps retiring', () => {
    // The direction that must not be got wrong. `victim` is on no list in this
    // view — it resolved, died, or was exiled — so declaring a container would
    // leave a path pointing at nothing for the rest of the match.
    const current = view();
    current.battlefield = [permanent('attacker')];
    current.stack = [];
    const [staged] = attachOccludedEndpoints([path()], {
      view: current,
      plane: plane(current),
      rectFor: rects(),
    });
    expect(staged!.edge).toBeUndefined();
    expect(staged).toEqual(path());
  });

  it('touches nothing when both endpoints are drawn on the plane', () => {
    const current = view();
    current.battlefield = [permanent('attacker'), permanent('victim', 'p2')];
    const effects = [path()];
    const staged = attachOccludedEndpoints(effects, {
      view: current,
      plane: plane(current),
      rectFor: rects({ victim: { x: 700, y: 200, w: 66, h: 92 } }),
    });
    expect(staged[0]).toEqual(effects[0]);
  });

  it('retires rather than indicates when BOTH endpoints are undrawable', () => {
    // With nothing on screen there is no anchor for the indicator's tangent to
    // point away from, so the honest outcome is the carried one.
    const current = view();
    current.battlefield = [permanent('attacker'), permanent('victim', 'p2')];
    const [staged] = attachOccludedEndpoints([path()], {
      view: current,
      plane: plane(current),
      rectFor: () => undefined,
    });
    // A container is still declared (one endpoint of the pair may come back on
    // the next view); the LAYER is what retires it while neither rect resolves.
    expect(staged!.edge).toBeDefined();
  });

  it('exempts a resolving relationship — its endpoints have already left', () => {
    // A §6.2 retraction is frozen onto the rects its endpoints last occupied.
    // Classifying it here would call every retraction "vanished" and cancel it
    // before its first frame.
    const current = view();
    current.battlefield = [];
    current.stack = [];
    const tether = path({
      id: 'tether:ability',
      category: 'source-tether',
      accent: SCENE_NEUTRALS.text,
      state: 'resolving',
      from: { rect: { x: 10, y: 20, w: 48, h: 68 } },
      to: { ref: 'source' },
    });
    const [staged] = attachOccludedEndpoints([tether], {
      view: current,
      plane: plane(current),
      rectFor: rects(),
    });
    expect(staged).toEqual(tether);
  });

  it('never overrides a container the caller already declared', () => {
    const current = view();
    current.battlefield = [permanent('attacker'), permanent('victim', 'p2')];
    const declared = { x: 5, y: 5, w: 50, h: 50 };
    const [staged] = attachOccludedEndpoints([path({ edge: declared })], {
      view: current,
      plane: plane(current),
      rectFor: rects(),
    });
    expect(staged!.edge).toBe(declared);
  });
});
