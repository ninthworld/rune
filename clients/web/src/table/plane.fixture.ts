import { normalizeGameView } from '../wire';
import type { GameView } from '../protocol';
import type { Rect } from './scene';
import {
  stagePlane,
  type PlaneRegion,
  type PlaneStagingState,
  type PlaneViewport,
  type StagedPlane,
} from './plane';

/** The desktop reference plane (presentation-budgets §Device envelope). */
export const DESKTOP: PlaneViewport = { width: 1280, height: 800 };
/** The phone-portrait reference plane (390×844). */
export const PHONE: PlaneViewport = { width: 390, height: 844 };
/** A 16:9 reference plane at the ultrawide height (surplus-width baseline). */
export const WIDE16: PlaneViewport = { width: 1920, height: 1080 };
/** A 21:9 ultrawide reference plane (presentation-budgets §Device envelope). */
export const ULTRAWIDE: PlaneViewport = { width: 2560, height: 1080 };
/** The tablet-landscape reference plane at the geometry floor (1180×820). */
export const TABLET: PlaneViewport = { width: 1180, height: 820 };

/** A minimal permanent spec for staging tests. */
export interface PlanePermSpec {
  id: string;
  controller: string;
  name?: string;
  type_line?: string;
  tapped?: boolean;
  power?: string;
  toughness?: string;
  rules_text?: string;
  attacking?: boolean;
  attacking_player?: string;
  blocking?: string;
  attached_to?: string;
}

/**
 * A normalized table view: `p1` is the receiver; opponents are `p2`, `p3`, … in
 * seat order (override with `seatOrder`). Everything else takes the staging
 * tests' defaults.
 */
export function seatTable(opts: {
  opponents?: number;
  perms?: PlanePermSpec[];
  eliminated?: string[];
  active?: string;
  seatOrder?: string[];
  you?: string;
  validActions?: GameView['valid_actions'];
}): GameView {
  const count = opts.opponents ?? 1;
  const ids = Array.from({ length: count }, (_, i) => `p${i + 2}`);
  const eliminated = new Set(opts.eliminated ?? []);
  return normalizeGameView({
    you: opts.you ?? 'p1',
    my_hand: [],
    opponents: ids.map((id) => ({
      player_id: id,
      hand_size: 3,
      life: 40,
      library_size: 60,
      ...(eliminated.has(id) ? { eliminated: true } : {}),
    })),
    battlefield: (opts.perms ?? []).map((p) => {
      const typeLine = p.type_line ?? 'Creature — Bear';
      const isCreature = /\bCreature\b/.test(typeLine);
      return {
        id: p.id,
        controller: p.controller,
        owner: p.controller,
        tapped: p.tapped,
        attacking: p.attacking,
        attacking_player: p.attacking_player,
        blocking: p.blocking,
        attached_to: p.attached_to,
        card: {
          id: p.id,
          name: p.name ?? p.id,
          type_line: typeLine,
          power: p.power ?? (isCreature ? '2' : undefined),
          toughness: p.toughness ?? (isCreature ? '2' : undefined),
          rules_text: p.rules_text,
        },
      };
    }),
    phase: 'precombat_main',
    active_player: opts.active ?? 'p1',
    seat_order: opts.seatOrder ?? ['p1', ...ids],
    valid_actions: opts.validActions ?? [],
  });
}

/** `n` identical-state bears for one controller (they may fold to ×N). */
export function bears(
  controller: string,
  n: number,
  opts: { tapped?: boolean; prefix?: string } = {},
): PlanePermSpec[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `${opts.prefix ?? controller}_bear_${i}`,
    controller,
    name: 'Bear',
    tapped: opts.tapped,
  }));
}

/**
 * `n` identical basic lands of one kind for one controller — the ×N fold's
 * canonical case (visual-system §5: "four Plains should look like a stack of
 * Plains"). They fold exactly like {@link bears}; only the row differs.
 */
export function basics(
  controller: string,
  n: number,
  kind: 'Plains' | 'Island' | 'Swamp' | 'Mountain' | 'Forest',
  opts: { tapped?: boolean; prefix?: string } = {},
): PlanePermSpec[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `${opts.prefix ?? controller}_${kind.toLowerCase()}_${i}`,
    controller,
    name: kind,
    type_line: `Basic Land — ${kind}`,
    tapped: opts.tapped,
  }));
}

/** `n` pairwise-distinct creatures for one controller (they never fold). */
export function menagerie(controller: string, n: number): PlanePermSpec[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `${controller}_beast_${i}`,
    controller,
    name: `Beast ${i}`,
    power: String(1 + (i % 9)),
  }));
}

/** Stage a view against the desktop plane unless told otherwise. */
export function stage(
  view: GameView,
  viewport: PlaneViewport = DESKTOP,
  staging?: PlaneStagingState,
): StagedPlane {
  return stagePlane(view, viewport, staging);
}

/** Every staged region: receiver, far side, wings — in that order. */
export function regionsOf(plane: StagedPlane): PlaneRegion[] {
  return [plane.receiver, plane.farSide, ...plane.wings].filter(
    (r): r is PlaneRegion => r !== undefined,
  );
}

/**
 * Every rect the plane stages — region slots, crest clusters, pile clusters,
 * render hotspots, tiles, and tile candidate hotspots — for the corridor
 * emptiness checks.
 */
export function allPlaneRects(plane: StagedPlane): Rect[] {
  const rects: Rect[] = [];
  for (const region of regionsOf(plane)) {
    rects.push(region.rect, region.crest, region.piles);
    for (const slot of region.rack.slots) rects.push(slot.hitRect);
    for (const render of region.renders) rects.push(render.hitRect);
  }
  for (const tile of plane.tiles) {
    rects.push(tile.rect, tile.crest);
    for (const render of tile.candidates) rects.push(render.hitRect);
  }
  return rects;
}
