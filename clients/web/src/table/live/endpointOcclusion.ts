/**
 * The §10.3 staging adapter: **occluded is not vanished**
 * (`docs/design/stack-and-relationships.md` §10.3, implementation note IN2).
 *
 * `EffectsLayer.buildProgram` gives an undrawable endpoint three outcomes, but
 * only the caller can tell which one applies — the layer sees a rect source and
 * nothing else. This module is that caller. It answers one question per
 * endpoint, from the authoritative view and the staged plane alone:
 *
 * > Does the thing this relationship points at still **exist in the view**?
 *
 * - **Yes, and the plane published no rect for it** — it exists but is not
 *   drawn: scrolled out of the stack rail, behind the compact sheet, or on a
 *   surface this rung of the ladder does not stage. That is *occluded*. The
 *   relationship keeps its declared container and the layer terminates the path
 *   on that container's edge with an indicator, so it is never silently lost.
 * - **Yes, and its rect lies wholly outside the plane** — drawn, but off the
 *   viewport. That is *clipped*, and it takes the same treatment against the
 *   viewport container.
 * - **No** — the object left the view entirely (it resolved, died, was exiled,
 *   or the seat was eliminated). That is *vanished*, and it keeps retiring
 *   exactly as before. This is the direction that must not be got wrong:
 *   attaching a container to a departed endpoint would leave paths pointing at
 *   nothing for the rest of the match.
 *
 * The membership test is over the view's **own server-supplied lists** and
 * nothing else — `battlefield`, `stack`, `seat_order`. It parses no text,
 * infers no kind, and computes no legality (invariant I1); "still exists" is a
 * fact the server stated, not one the client derived.
 *
 * A **resolving** relationship (§6.2) is exempt: its endpoints have by
 * definition just left the view, and its anchors are already frozen onto the
 * rects they occupied. Testing it for occlusion would classify every retraction
 * as vanished and cancel it before its first frame.
 */
import type { GameView } from '../../protocol';
import type { PersistentEffect } from '../effects';
import type { StagedPlane } from '../plane';
import type { Rect } from '../scene';

/** Whether two rects share any area — "is this rect on screen at all". */
function intersects(a: Rect, b: Rect): boolean {
  return a.x <= b.x + b.w && a.x + a.w >= b.x && a.y <= b.y + b.h && a.y + a.h >= b.y;
}

/**
 * Whether `ref` names something the **current view still contains**.
 *
 * Relationship anchors are only ever one of three shapes, and each maps to one
 * server-supplied list. An unrecognised shape answers `false`, so an endpoint
 * this module does not understand keeps the carried retire-on-unresolvable
 * behaviour rather than acquiring an indicator it cannot justify.
 */
export function endpointPresentInView(view: GameView, ref: string): boolean {
  if (ref.startsWith('seat:')) {
    const seat = ref.slice('seat:'.length);
    return (
      view.seat_order.includes(seat) ||
      view.you === seat ||
      view.opponents.some((opponent) => opponent.player_id === seat)
    );
  }
  if (ref.startsWith('stack:')) {
    const id = ref.slice('stack:'.length);
    return view.stack.some((item) => item.id === id);
  }
  if (ref.includes(':')) return false;
  return view.battlefield.some((permanent) => permanent.id === ref);
}

/**
 * The container an occluded endpoint's indicator lands on (§10.3: "the nearest
 * container edge — rail edge, sheet rail, or viewport edge").
 *
 * A stack endpoint belongs to the stage, which is staged in the centre
 * corridor, so a stack entry the rail did not draw gets its chevron on the
 * corridor's edge — pointing at where the stage is, which is where activating
 * it would scroll to. Everything else belongs to the board as a whole and
 * clamps to the plane's own bounds.
 *
 * The corridor is intersected with nothing and clamped to nothing on purpose:
 * `stagePlane` guarantees it lies inside the plane, so both containers are
 * always on screen and the indicator is always reachable.
 */
export function occlusionContainer(ref: string, plane: StagedPlane): Rect {
  const viewport: Rect = { x: 0, y: 0, w: plane.width, h: plane.height };
  return ref.startsWith('stack:') ? plane.corridor : viewport;
}

/** What the staging adapter needs to classify an endpoint. */
export interface OcclusionContext {
  /** The authoritative view — the only source of "does this still exist". */
  view: GameView;
  /** The staged plane, for the container rects and the viewport bounds. */
  plane: StagedPlane;
  /** The live rect source, i.e. the same anchors the effects layer reads. */
  rectFor: (ref: string) => Rect | undefined;
}

/**
 * Attach the §10.3 container to every relationship with an endpoint that is
 * **occluded or clipped rather than gone**, and leave every other relationship
 * untouched.
 *
 * At most one container is attached per relationship, because a path has one
 * terminus to redirect; when both ends are undrawable there is no on-screen
 * anchor for an indicator to point away from, so the relationship retires — the
 * carried behaviour, and the honest one.
 */
export function attachOccludedEndpoints(
  effects: readonly PersistentEffect[],
  { view, plane, rectFor }: OcclusionContext,
): PersistentEffect[] {
  const viewport: Rect = { x: 0, y: 0, w: plane.width, h: plane.height };
  const containerFor = (anchor: PersistentEffect['from']): Rect | undefined => {
    if ('rect' in anchor) return undefined;
    const rect = rectFor(anchor.ref);
    if (rect === undefined) {
      // Undrawn. Occluded only if the server still lists it; otherwise gone.
      return endpointPresentInView(view, anchor.ref)
        ? occlusionContainer(anchor.ref, plane)
        : undefined;
    }
    // Drawn, but off the plane: clipped. The viewport is the container whose
    // edge the chevron rides, and the other endpoint is inside it by
    // construction, which is what makes the clipped end unambiguous downstream.
    return intersects(rect, viewport) ? undefined : viewport;
  };
  return effects.map((effect) => {
    if (effect.edge !== undefined || effect.state === 'resolving') return effect;
    const container = containerFor(effect.to) ?? containerFor(effect.from);
    return container === undefined ? effect : { ...effect, edge: container };
  });
}
