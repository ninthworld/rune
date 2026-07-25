/**
 * The bridge between the plane's staged slots and the environment's focal-safe
 * zones (`docs/design/environment-system.md` §2.1, §3.3) — issue #530.
 *
 * §2.1 derives the focal-safe geometry by overlaying every slot rect the plane
 * can occupy at 2–6 seats, clipping to the canvas, and taking the union. This
 * module performs that derivation **against the live `carveSlots`** rather than
 * against a transcription of it, so a layout change that widens the occupied
 * union fails the environment's tests instead of quietly invalidating the art.
 *
 * The boundary §3.3 draws is the reason this file only reads:
 *
 * - Layout may move a seat anywhere inside Zone A ∪ Zone B without consulting
 *   the art. That is what the containment check here proves.
 * - Layout may **not** place drawn content inside Zone C without amending §2.2.
 * - A layout change that widens the occupied union changes the zones, in the
 *   same PR — which is the failure this module is designed to produce.
 *
 * Nothing here mutates plane geometry; `carveSlots` is imported and called, and
 * its output is normalised to canvas fractions. Plane geometry belongs to
 * issue #531.
 */
import type { PlayerId } from '../../protocol';
import type { Rect } from '../scene/types';
import { carveSlots, type PlaneViewport, type StagedPlane } from '../plane';
import type { EnvBias } from './quality';
import { clipToCanvas, type FractionRect } from './zones';

/** Seat counts the one environment must serve unchanged (§3.1, panels 1–5). */
export const ENV_SEAT_COUNTS = [2, 3, 4, 5, 6] as const;

/** A seat count the environment composition is proven at. */
export type EnvSeatCount = (typeof ENV_SEAT_COUNTS)[number];

/**
 * The landscape viewports the crop table of §4.2 names, as the geometries the
 * occupancy union is proven across. Phone portrait is deliberately absent: §4.5
 * makes portrait a **recomposition** rather than a crop, so it is governed by
 * the portrait branch of the quality plan, not by these zones.
 */
export const ENV_REFERENCE_VIEWPORTS: readonly { label: string; viewport: PlaneViewport }[] = [
  { label: 'ultrawide 21:9', viewport: { width: 2560, height: 1080 } },
  { label: 'desktop 16:9', viewport: { width: 1680, height: 945 } },
  { label: 'desktop 16:10', viewport: { width: 1680, height: 1050 } },
  { label: 'desktop 3:2', viewport: { width: 1500, height: 1000 } },
  { label: 'tablet landscape floor 1180×820', viewport: { width: 1180, height: 820 } },
  { label: 'tablet 4:3', viewport: { width: 1280, height: 960 } },
];

/** Synthetic seat ids for a pure geometry pass — `carveSlots` only orders them. */
function seatsFor(count: EnvSeatCount): { far: PlayerId; peripherals: PlayerId[] } {
  const opponents = Array.from({ length: count - 1 }, (_, i) => `p${i + 1}`);
  return { far: opponents[0]!, peripherals: opponents.slice(1) };
}

/** A plane rect expressed as a fraction of the canvas it was carved for. */
export function toFractionRect(rect: Rect, viewport: PlaneViewport): FractionRect {
  return {
    x: rect.x / viewport.width,
    y: rect.y / viewport.height,
    w: rect.w / viewport.width,
    h: rect.h / viewport.height,
  };
}

/**
 * Every slot rect `carveSlots` produces for one seat count and viewport, in
 * canvas fractions and clipped to the canvas exactly as §2.1 specifies. Wings
 * bleed 28 % of their width offstage, so the clip is load-bearing rather than
 * defensive.
 *
 * The receiver, the far side, the wings, and the corridor are included; crest
 * and pile clusters are not, because `carveSlots` does not produce them — they
 * are attached by `stagePlane`. See the note in `environment.test.ts`.
 */
export function planeOccupancy(count: EnvSeatCount, viewport: PlaneViewport): FractionRect[] {
  const { far, peripherals } = seatsFor(count);
  const slots = carveSlots(viewport, true, far, peripherals);
  const rects: Rect[] = [];
  if (slots.receiver) rects.push(slots.receiver);
  if (slots.far) rects.push(slots.far.rect);
  for (const wing of slots.wings) rects.push(wing.rect);
  rects.push(slots.corridor);
  return rects.map((rect) => clipToCanvas(toFractionRect(rect, viewport)));
}

/**
 * The parallax bias for a staged plane (§1.1): the environment leans a little
 * toward whatever the staging tween just focused.
 *
 * This is the *only* permitted parallax driver. ADR 0030 has no free camera, so
 * the excursion may not follow pointer position, device orientation, or scroll —
 * it follows the plane delta, and nothing else. The magnitude is the plan's
 * `E` ladder (12 px desktop, halved at Standard, zero at Lite and under reduced
 * motion), which sits far below the 44 px hit floor: the environment can never
 * move something a player is aiming at.
 *
 * A duel has no focused seat and a receiver-less (spectator) plane has no
 * receiver, so both return a centred bias rather than a special case.
 */
export function environmentBias(plane: StagedPlane): EnvBias {
  const focus = plane.focusSeat;
  if (focus === undefined) return { x: 0, y: 0 };
  const region = [plane.farSide, ...plane.wings].find((entry) => entry?.seat === focus);
  if (region === undefined || plane.width <= 0 || plane.height <= 0) return { x: 0, y: 0 };
  const cx = (region.rect.x + region.rect.w / 2) / plane.width;
  const cy = (region.rect.y + region.rect.h / 2) / plane.height;
  const clamp = (value: number): number => Math.max(-1, Math.min(1, value));
  return { x: clamp((cx - 0.5) * 2), y: clamp((cy - 0.5) * 2) };
}
