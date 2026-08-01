/**
 * The relationships, as geometry: where a line between two objects starts and where it ends.
 *
 * `relations.ts` says *what* points at what. This says *where*, given a box for each object the
 * screen happens to be drawing. The split is the point: the join is about identifiers the server
 * stated and has nothing to do with pixels, and this module is about pixels and knows nothing
 * about the game. Together they are an arrow from an attacker to what it is attacking, which is
 * the fact a text trail states in a sentence under the card and which every other tabletop draws.
 *
 * **It renders `relations.ts` and nothing else.** Every edge here came from an identifier the
 * server projected. There is no rule in this file that could invent a relationship, promote one,
 * or drop one for being uninteresting — an edge is drawn when both of its ends are on the screen
 * and is not drawn when they are not, and that is the whole of the filter. An overlay is the
 * easiest place in the client for a relationship to be imagined, so the only decisions it is
 * allowed to make are about geometry.
 *
 * The line runs between the *borders* of the two boxes rather than between their centres. A line
 * that starts under a card's art and ends under another card's art has to cross both faces to be
 * seen; a line that starts where the card ends is a connection between two objects rather than a
 * scratch across them.
 *
 * Nothing here reads the DOM. Boxes come in as plain numbers, which is what makes the geometry
 * testable without a browser — the measuring is the component's job (`ui/game/RelationOverlay`).
 */
import type { Relation, RelationKind } from './relations'

/**
 * How a surface says "this element is the object with that id".
 *
 * The name lives here, with the geometry that reads it back, so a card and the sheet drawn over
 * it cannot disagree about it. The value is always an identifier the **server** stated — the same
 * one `valid_actions` and the relationship join use — which is what makes an anchored element and
 * a related object the same thing rather than two things a lookup has to reconcile.
 */
export const ANCHOR = 'data-entity'

export const anchorProps = (id: string): { [ANCHOR]: string } => ({ [ANCHOR]: id })

/** A box in the overlay's own coordinates: the origin is the frame's top-left corner. */
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

/**
 * How much of the player's attention an edge is currently owed.
 *
 * `plain` is the resting state, when the player is looking at nothing in particular. Once they
 * *are* looking at an object, every edge that touches it becomes `traced` and every other edge
 * becomes `dimmed` — a board mid-combat has a line for every attack, every block, and every
 * target on the stack, and the answer to that density is emphasis rather than deciding on the
 * player's behalf which of the server's facts were worth drawing.
 */
export type OverlayEmphasis = 'plain' | 'traced' | 'dimmed'

/** One relationship, as a segment to draw. Directed: it runs from the object that states it. */
export interface OverlayEdge {
  kind: RelationKind
  from: string
  to: string
  x1: number
  y1: number
  x2: number
  y2: number
  emphasis: OverlayEmphasis
}

/**
 * Turn stated relationships into segments, given whatever the screen is currently drawing.
 *
 * An edge survives when both of its ends are anchored *and* on the board: an object with no box
 * is one this surface is not drawing at all — a card in a pile nobody opened — and an object
 * whose box has scrolled out of the frame is one an arrow would point at through the edge of the
 * board. Neither is a fact being withheld: the trail under the card still names the other end in
 * words, and clicking that name is what reaches it.
 *
 * Traced edges come last so they paint over the rest of the board. Painting order is the only
 * ranking in this module, and it is undone the moment the pointer moves.
 */
export function overlayEdges(
  all: readonly Relation[],
  anchors: ReadonlyMap<string, Rect>,
  frame: Rect,
  options: { traced?: string } = {},
): readonly OverlayEdge[] {
  const edges: OverlayEdge[] = []

  for (const relation of all) {
    // An edge the server stated without naming its other end — an attack with no defender
    // projected — has nothing to point at, and choosing something would be this client
    // answering a question the view declined to answer.
    if (relation.to === undefined) continue
    // Guarded rather than assumed: a zero-length segment has no direction, so an arrowhead on it
    // would point somewhere arbitrary.
    if (relation.to === relation.from) continue

    const from = visible(anchors.get(relation.from), frame)
    const to = visible(anchors.get(relation.to), frame)
    if (!from || !to) continue

    const start = border(from, centre(to))
    const end = border(to, centre(from))
    edges.push({
      kind: relation.kind,
      from: relation.from,
      to: relation.to,
      x1: start.x,
      y1: start.y,
      x2: end.x,
      y2: end.y,
      emphasis:
        options.traced === undefined
          ? 'plain'
          : relation.from === options.traced || relation.to === options.traced
            ? 'traced'
            : 'dimmed',
    })
  }

  // Stable, so an edge does not change depth for any reason other than being traced.
  return [...edges].sort((a, b) => rank(a.emphasis) - rank(b.emphasis))
}

const rank = (emphasis: OverlayEmphasis): number => (emphasis === 'traced' ? 1 : 0)

/** A box, if it is one the frame actually shows any of. */
function visible(rect: Rect | undefined, frame: Rect): Rect | undefined {
  if (!rect) return undefined
  const clear =
    rect.x + rect.width <= frame.x ||
    rect.x >= frame.x + frame.width ||
    rect.y + rect.height <= frame.y ||
    rect.y >= frame.y + frame.height
  return clear ? undefined : rect
}

/**
 * The part of a box that two boxes have in common, or nothing where they do not overlap.
 *
 * What it is for is the board's scrolling regions: a permanent below the fold of its own row is
 * still laid out, still has a box, and is not on the screen. Clipping an anchor to what is
 * actually showing is what keeps an arrow from pointing confidently at a card the player cannot
 * see — and where a card is half in view, it points at the half that is.
 */
export function intersect(rect: Rect, clip: Rect): Rect | undefined {
  const x = Math.max(rect.x, clip.x)
  const y = Math.max(rect.y, clip.y)
  const right = Math.min(rect.x + rect.width, clip.x + clip.width)
  const bottom = Math.min(rect.y + rect.height, clip.y + clip.height)
  if (right <= x || bottom <= y) return undefined
  return { x, y, width: right - x, height: bottom - y }
}

interface Point {
  x: number
  y: number
}

const centre = (rect: Rect): Point => ({ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 })

/**
 * Where the ray from a box's centre towards a point leaves the box.
 *
 * The scale is taken against whichever axis runs out first, which is what makes the answer land
 * on the nearer of the four sides for any direction — the side a player would draw the line from
 * if they were joining the two cards with a pencil.
 */
function border(rect: Rect, towards: Point): Point {
  const from = centre(rect)
  const dx = towards.x - from.x
  const dy = towards.y - from.y
  // Two boxes with the same centre — a card and a wrapper around it — have no direction between
  // them, so the centre is the only answer that is not invented.
  if (dx === 0 && dy === 0) return from

  const scale = Math.min(
    dx === 0 ? Infinity : rect.width / 2 / Math.abs(dx),
    dy === 0 ? Infinity : rect.height / 2 / Math.abs(dy),
  )
  return { x: from.x + dx * scale, y: from.y + dy * scale }
}
