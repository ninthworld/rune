/**
 * The seat **identity cluster** — portrait medallion, nameplate, life medallion,
 * hand pip, identity gem, and the state rail — implementing
 * `docs/design/seat-identity.md` §1, §2, §5–§9 for issue #532.
 *
 * Pure geometry and pure text fitting. Nothing here reads a `GameView`, touches
 * the DOM, or decides a rule: it is handed already-derived facts
 * ({@link SeatClusterFacts}, assembled by `table/seatIdentity.ts` straight from
 * the view) and answers where each element of the cluster sits at the rung's
 * scale unit `D`.
 *
 * Four rules from the specification drive everything below:
 *
 * - **One `D` per cluster** (§1.1). Every offset in {@link PLANE.cluster} is a
 *   multiple of the portrait's outer rim diameter, and nothing inside a cluster
 *   scales independently of it.
 * - **Every state gets a distinct shape or placement** (§6). Priority is a
 *   concentric ring, the active turn a 12-o'clock pennant, the attacked marker
 *   an inset dashed ring plus a rail chip, elimination a struck glyph *in place
 *   of* the life numeral. They occupy disjoint placements by construction, so
 *   "every state at once" (§7) needs no arbitration.
 * - **The crest carries the hand count and only the hand count**
 *   (`zone-geography.md` §4.1, resolved against seat-identity §1.2 el. 8): the
 *   library count lives on the library pile. There is no library pip.
 * - **A dormant slot renders nothing** (§11). Poison, structured counters, the
 *   connection glyph, local elimination, the AI marker, and the commander name
 *   have no protocol field today, so they are laid out and left unrendered —
 *   never a zero, never an "unknown", and never parsed out of free-form text.
 *
 * The nameplate's placement is the one documented departure from §8, and it is
 * confined to {@link resolvePlate}: see the note there.
 */
import type { PlayerId } from '../../protocol';
import type { ColorIdentity } from '../../tokens';
import type { Rect } from '../scene/types';
import type { SeatPortrait } from '../seatPortraits';
import { PLANE, hitRectFor } from './metrics';
import type { PlaneViewport } from './types';

/** The five rungs of the §1.1 ladder, as the variants §2 names them. */
export type ClusterVariant = 'local' | 'focused' | 'wing' | 'compact' | 'minimal';

/** Which way the nameplate runs from the portrait (§8, and `resolvePlate`). */
export type PlateDirection = 'left' | 'right' | 'below';

/** The §5.1 life-urgency shape channel — a threshold on a *displayed number*. */
export type LifeUrgency = 'none' | 'low' | 'zero';

/** The §5.4 commander-damage escalation shapes (thresholds from CR 903.10a). */
export type DamageEscalation = 'plain' | 'doubled' | 'notched' | 'terminal';

/** What a status-rail medallion stands for (§5.4, §5.6, §6.3, §6.7). */
export type ClusterChipKind =
  'attacked' | 'commanderDamage' | 'commanderTax' | 'autoPassed' | 'status' | 'overflow';

/** One drawn status-rail medallion. */
export interface ClusterChip {
  /** Which channel it carries. */
  kind: ClusterChipKind;
  /** The medallion's rect (Ø `0.28 D`, on the §1.2 el. 10 arc). */
  rect: Rect;
  /** The glyph drawn at the rung — no text rides a medallion (§5.6). */
  glyph: string;
  /** The value, where the chip carries one (`×3`, `+4`, `21`). */
  value?: string;
  /** The accessible label; for a status this is the server string, verbatim. */
  label: string;
  /** Escalation shape, for the commander-damage shield only (§5.4). */
  escalation?: DamageEscalation;
}

/** The nameplate and the name it fitted (§1.2 el. 4/5, §7 long name). */
export interface ClusterPlate {
  /** The plate's rect. */
  rect: Rect;
  /** Which way it runs from the portrait. */
  direction: PlateDirection;
  /** The text actually drawn — middle-ellipsised where the plate could not hold it. */
  text: string;
  /** Whether {@link text} is shorter than the seat's real label. */
  truncated: boolean;
}

/** The hand pip, in whichever of its two attachment modes the rung draws (§1.2). */
export interface ClusterPip {
  /** The pip's rect. */
  rect: Rect;
  /** `hex` is the free-standing vertical hexagon; `tab` the under-slung shield. */
  shape: 'hex' | 'tab';
  /** The hand count, straight from the view. */
  count: number;
}

/** The non-colour state channels a cluster wears (§6.8). */
export interface ClusterChannels {
  /** Holds priority — the concentric double ring (§6.1). */
  priority: boolean;
  /** Is the active player — the 12-o'clock swallow-tail pennant (§6.2). */
  active: boolean;
  /** Is under attack — the inset dashed ring (§6.3). */
  attacked: boolean;
  /** Is the manually or by-relevance focused opponent. */
  focused: boolean;
  /** Has been eliminated — stone rim and the struck rune (§6.5). */
  eliminated: boolean;
  /** Has a commander — the 5-o'clock crown mark (§4). */
  commander: boolean;
  /** The receiver's decision deadline is running — the depleting arc (§6.6). */
  deadline: boolean;
  /**
   * The seat's transport is down — the 10-o'clock broken-chain glyph (§6.4).
   * **Dormant**: no per-seat connection field exists (§11, issue #553), so this
   * is always `false` today and the glyph is never drawn.
   */
  disconnected: boolean;
}

/** Everything the cluster displays, already read off the view. */
export interface SeatClusterFacts {
  /** The seat's nameplate label, before any truncation (§7's `Seat N` applied). */
  label: string;
  /** Whether this is the receiver's own seat — the accessible sentence says so. */
  local: boolean;
  /** Life total, verbatim from the server (§5.1). */
  life: number;
  /** Visible hand count (§4 — the crest's only count). */
  handCount: number;
  /** Library count — for the accessible sentence only; the pile owns the badge. */
  libraryCount: number;
  /** The seat's colour identity, when its commander is in the command zone. */
  gem?: ColorIdentity;
  /** Whether the seat has a commander at all (crown mark). */
  commanderPresent: boolean;
  /** Commander tax, only where §5.3 lets the cluster duplicate the pile's value. */
  commanderTax?: number;
  /** The single worst incoming commander damage (§5.4), when ≥ 1. */
  commanderDamage?: { amount: number; from: PlayerId };
  /** The seat's named statuses, in the server's array order (§5.6). */
  statuses: readonly string[];
  /** How many permanents are attacking this seat (§6.3 — a filter, not combat). */
  attackedCount: number;
  /** `auto_passed` on this view (§6.7); the receiver's cluster only. */
  autoPassed: boolean;
  /** `action_deadline` is present (§6.6); the receiver's cluster only. */
  deadline: boolean;
  /** The plate this seat wears, when one resolved (§1.3). */
  portrait?: SeatPortrait;
  /** The seat's accent, from `sceneTokens`. */
  accent: string;
  /** Elimination, priority, active turn, focus, and attack, from the view. */
  eliminated: boolean;
  /** Whether this seat holds priority. */
  priority: boolean;
  /** Whether this seat is the active player. */
  active: boolean;
  /** Whether this seat is the focused opponent. */
  focused: boolean;
  /** Whether any attacker is attacking this seat. */
  attacked: boolean;
}

/** Everything {@link stageSeatCluster} needs; all of it derived by the stage. */
export interface SeatClusterRequest {
  /** The seat. */
  seat: PlayerId;
  /** Which rung of the §1.1 ladder this cluster draws at. */
  variant: ClusterVariant;
  /** Where the portrait medallion's centre wants to sit. */
  anchor: { x: number; y: number };
  /** The plane, so nothing stages off-canvas. */
  viewport: PlaneViewport;
  /** Which way §8's table points this slot's nameplate. */
  outboard: 'left' | 'right';
  /** A rect the nameplate must not cover — the seat's own zone rack. */
  keepOut?: Rect;
  /** The displayed facts. */
  facts: SeatClusterFacts;
}

/** One seat's staged identity cluster. */
export interface SeatCluster {
  /** The seat. */
  seat: PlayerId;
  /** The seat's full, untruncated display name — never a raw `p{N}` id (§7). */
  name: string;
  /** The rung it drew at. */
  variant: ClusterVariant;
  /** The scale unit `D` in px — the portrait's outer rim diameter (§1.1). */
  d: number;
  /** The portrait medallion's drawn box (`D × D`). */
  portrait: Rect;
  /** The cluster's activation rect: the medallion grown to the 44 px floor. */
  hit: Rect;
  /** The life medallion's box. */
  life: Rect;
  /** What the life medallion draws — the numeral, or §6.5's struck rune. */
  lifeGlyph: string;
  /** Glyph count in {@link lifeGlyph}, which steps the numeral's size (§5.1). */
  lifeGlyphs: number;
  /** The §5.1 urgency shape, `none` for an eliminated seat (the life is gone). */
  urgency: LifeUrgency;
  /** The nameplate, absent only at the minimal rung or where none fits. */
  plate?: ClusterPlate;
  /** The identity gem, absent while the seat's colour identity is unknowable. */
  gem?: { rect: Rect; identity: ColorIdentity };
  /** The hand pip, absent at the minimal rung and on an eliminated seat (§2). */
  pip?: ClusterPip;
  /** The status rail, capped per rung, `+N` last (§5.6, §7). */
  chips: ClusterChip[];
  /** The state channels (§6.8). */
  channels: ClusterChannels;
  /**
   * The plate's URL, when one resolved. When it is absent — a loading plate, a
   * failed one, or a build with none — the aperture keeps its token background
   * and draws **no substitute glyph** (§1.3); the seat's name stays in the
   * accessibility tree through {@link ariaLabel}.
   */
  portraitSrc?: string;
  /** The seat's accent colour. */
  accent: string;
  /** The union of everything drawn, priority bloom included. */
  bounds: Rect;
  /** The whole seat as one sentence (§9 screen readers). */
  ariaLabel: string;
}

/** Nominal `D` for a rung, before the plane's own ceiling applies. */
function nominalD(variant: ClusterVariant): number {
  return PLANE.cluster.d[variant];
}

/**
 * The rung's `D`, capped so a cluster never outgrows the plane it is staged on
 * (a phone's 390 px width would otherwise carry a 112 px medallion). Never falls
 * below the minimal rung's 48 px, which is itself above the 44 px touch floor.
 */
export function clusterD(variant: ClusterVariant, viewport: PlaneViewport): number {
  const { maxWidthFrac, maxHeightFrac, d } = PLANE.cluster;
  return Math.max(
    d.minimal,
    Math.min(
      nominalD(variant),
      Math.floor(viewport.width * maxWidthFrac),
      Math.floor(viewport.height * maxHeightFrac),
    ),
  );
}

/** A box of `w × h` in `D` units, centred at the offset `(cx, cy)` in `D` units. */
function unit(
  anchor: { x: number; y: number },
  d: number,
  cx: number,
  cy: number,
  w: number,
  h: number,
): Rect {
  return { x: anchor.x + (cx - w / 2) * d, y: anchor.y + (cy - h / 2) * d, w: w * d, h: h * d };
}

/** The union of a non-empty rect list. */
function union(rects: Rect[]): Rect {
  const x0 = Math.min(...rects.map((r) => r.x));
  const y0 = Math.min(...rects.map((r) => r.y));
  const x1 = Math.max(...rects.map((r) => r.x + r.w));
  const y1 = Math.max(...rects.map((r) => r.y + r.h));
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** Whether two rects share positive area. */
function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** Graphemes of a label, code-point-wise so an astral name never splits. */
function graphemes(text: string): string[] {
  return Array.from(text);
}

/**
 * §7's long-name rule: keep the first {@link PLANE.cluster.plate.headGraphemes}
 * and the last {@link PLANE.cluster.plate.tailGraphemes} around a middle
 * ellipsis. A plate shorter than the full-length one keeps the same 2:1 head/tail
 * proportion, so the rule degrades rather than changing kind. The whole name
 * always rides the accessible sentence.
 */
export function fitName(label: string, capacity: number): { text: string; truncated: boolean } {
  const chars = graphemes(label);
  if (capacity <= 0) return { text: '', truncated: chars.length > 0 };
  if (chars.length <= capacity) return { text: label, truncated: false };
  if (capacity === 1) return { text: '…', truncated: true };
  const { headGraphemes, tailGraphemes, maxGraphemes } = PLANE.cluster.plate;
  const budget = capacity - 1;
  const head =
    capacity >= maxGraphemes
      ? headGraphemes
      : Math.max(1, Math.round((budget * headGraphemes) / (headGraphemes + tailGraphemes)));
  const tail = Math.max(1, budget - head);
  return {
    text: `${chars.slice(0, head).join('')}…${chars.slice(-tail).join('')}`,
    truncated: true,
  };
}

/** How many graphemes a plate of `len` px holds at scale `d` (§1.2 el. 5 inset). */
function plateCapacity(len: number, d: number): number {
  const { inset, advance } = PLANE.cluster.plate;
  return Math.max(0, Math.floor(len / d / advance - (2 * inset) / advance));
}

/** The plate length, in px, that exactly holds `count` graphemes at scale `d`. */
function plateLength(count: number, d: number): number {
  const { inset, advance, minLen, maxLen } = PLANE.cluster.plate;
  return Math.max(minLen * d, Math.min(maxLen * d, (2 * inset + count * advance) * d));
}

/**
 * How far a plate laid at `cy` (in `D` from the portrait centre) may reach on
 * each side before it leaves the plane or crosses the seat's own zone rack. The
 * rack only blocks when it actually shares the plate's horizontal band — a rack
 * above or below the plate is not in its way.
 */
function plateLimits(
  request: SeatClusterRequest,
  d: number,
  cy: number,
): { left: number; right: number } {
  const { anchor, viewport, keepOut } = request;
  const band: Rect = {
    x: 0,
    y: anchor.y + (cy - PLANE.cluster.plate.h / 2) * d,
    w: viewport.width,
    h: PLANE.cluster.plate.h * d,
  };
  const blocks = keepOut !== undefined && overlaps(keepOut, band) ? keepOut : undefined;
  return {
    left: blocks !== undefined && blocks.x < anchor.x ? Math.max(4, blocks.x + blocks.w) : 4,
    right:
      blocks !== undefined && blocks.x >= anchor.x
        ? Math.min(viewport.width - 4, blocks.x)
        : viewport.width - 4,
  };
}

/** The horizontal run available on one side of the portrait, in px. */
function sideRun(request: SeatClusterRequest, d: number, side: 'left' | 'right'): number {
  const { anchor } = request;
  const { gap } = PLANE.cluster.plate;
  const limits = plateLimits(request, d, 0);
  return side === 'left' ? anchor.x - gap * d - limits.left : limits.right - (anchor.x + gap * d);
}

/**
 * Where the nameplate goes.
 *
 * §8 fixes the direction — the portrait sits nearest the table centre and the
 * plate points outboard. That holds wherever the outboard run can hold a plate.
 *
 * **Documented departure (issue #532's baseline-fidelity rule).** Issue #531
 * anchors every cluster on the seat's *zone-rack origin* (`zone-geography.md`
 * §2.2), which puts a wing's medallion within `~20 px` of the plane edge and the
 * receiver's against its own pile strip. At those anchors the outboard run is
 * shorter than the `1.05 D` minimum plate, so the baseline treatment would draw
 * a plate through the plane edge or through the seat's own library. The smallest
 * fix is to try the inboard side next and, only when neither side holds a plate,
 * to hang it centred **below** the life medallion — still horizontal, still
 * attached, still never pushing the portrait. The baseline direction returns the
 * moment the outboard run fits, which is what every focused seat does.
 */
function resolvePlate(
  request: SeatClusterRequest,
  d: number,
): { direction: PlateDirection; len: number } | undefined {
  const { minLen, maxLen, belowCy } = PLANE.cluster.plate;
  const inboard = request.outboard === 'left' ? 'right' : 'left';
  for (const direction of [request.outboard, inboard] as const) {
    const run = sideRun(request, d, direction);
    if (run >= minLen * d) return { direction, len: Math.min(run, maxLen * d) };
  }
  const { viewport } = request;
  const limits = plateLimits(request, d, belowCy);
  const len = Math.min(maxLen * d, limits.right - limits.left);
  const bottom = request.anchor.y + (belowCy + PLANE.cluster.plate.h / 2) * d;
  if (len < minLen * d || bottom > viewport.height - 4) return undefined;
  return { direction: 'below', len };
}

/** The plate's rect for a resolved direction and length. */
function plateRect(
  request: SeatClusterRequest,
  d: number,
  direction: PlateDirection,
  len: number,
): Rect {
  const { anchor } = request;
  const { h, gap, belowCy } = PLANE.cluster.plate;
  if (direction === 'below') {
    // Centred on the portrait axis where there is room on both sides, and slid
    // against whichever limit bites otherwise — the plate never crosses the rack
    // and never leaves the plane, and it never pushes the portrait either way.
    const limits = plateLimits(request, d, belowCy);
    const x = Math.max(limits.left, Math.min(anchor.x - len / 2, limits.right - len));
    return { x, y: anchor.y + (belowCy - h / 2) * d, w: len, h: h * d };
  }
  const y = anchor.y - (h / 2) * d;
  return direction === 'left'
    ? { x: anchor.x - gap * d - len, y, w: len, h: h * d }
    : { x: anchor.x + gap * d, y, w: len, h: h * d };
}

/** The §5.1 numeral, or §6.5's struck rune when the seat is out of the game. */
function lifeGlyphFor(life: number, eliminated: boolean): string {
  if (eliminated) return '⊘';
  const text = String(life);
  if (text.length <= 5) return text;
  return life < 0 ? '-99+' : '999+';
}

/** The §5.1 urgency shape. A threshold on a displayed number, never a verdict. */
function urgencyFor(life: number, eliminated: boolean): LifeUrgency {
  if (eliminated) return 'none';
  if (life <= 0) return 'zero';
  return life <= 5 ? 'low' : 'none';
}

/** The §5.4 escalation shape for one incoming commander-damage total. */
export function damageEscalation(amount: number): DamageEscalation {
  if (amount >= 21) return 'terminal';
  if (amount >= 18) return 'notched';
  return amount >= 15 ? 'doubled' : 'plain';
}

/** One rail medallion before it is placed on the arc. */
interface ChipSpec {
  kind: ClusterChipKind;
  glyph: string;
  value?: string;
  label: string;
  escalation?: DamageEscalation;
}

/**
 * The rail's contents, in the fixed order §6.3 (attacked owns slot 1) and §5.6
 * (statuses keep the server's array order — the client never ranks them) give.
 * Poison and structured counters are absent by construction: no field carries
 * them (§11, issue #544), and a number may never be parsed out of `statuses`.
 */
function chipSpecs(facts: SeatClusterFacts): ChipSpec[] {
  const specs: ChipSpec[] = [];
  if (facts.attacked) {
    specs.push({
      kind: 'attacked',
      glyph: '⚔',
      value: `×${facts.attackedCount}`,
      label: `Attacked by ${facts.attackedCount}`,
    });
  }
  if (facts.commanderDamage) {
    const { amount } = facts.commanderDamage;
    specs.push({
      kind: 'commanderDamage',
      glyph: '⛨',
      value: String(amount),
      label: `Commander damage ${amount} of 21`,
      escalation: damageEscalation(amount),
    });
  }
  if (facts.commanderTax !== undefined) {
    specs.push({
      kind: 'commanderTax',
      glyph: '⌘',
      value: `+${facts.commanderTax}`,
      label: `Commander tax ${facts.commanderTax}`,
    });
  }
  if (facts.autoPassed) {
    specs.push({ kind: 'autoPassed', glyph: '⇥', label: 'Passed for you' });
  }
  for (const status of facts.statuses) {
    specs.push({ kind: 'status', glyph: '◈', label: status });
  }
  return specs;
}

/** Place the capped rail on the §1.2 el. 10 arc, growing from the inboard shoulder. */
function placeChips(request: SeatClusterRequest, d: number, specs: ChipSpec[]): ClusterChip[] {
  const { rail, railCap } = PLANE.cluster;
  const cap = railCap[request.variant];
  const sign = request.outboard === 'left' ? 1 : -1;
  const drawn = specs.slice(0, cap);
  const hidden = specs.length - drawn.length;
  const laid: ChipSpec[] = [...drawn];
  if (hidden > 0) {
    laid.push({
      kind: 'overflow',
      glyph: '+',
      value: String(hidden),
      label: `${hidden} more: ${specs
        .slice(cap)
        .map((spec) => spec.label)
        .join(', ')}`,
    });
  }
  return laid.map((spec, index) => {
    const deg = rail.startDeg + index * rail.stepDeg;
    const theta = (deg * Math.PI) / 180;
    const cx = sign * rail.radius * Math.sin(theta);
    const cy = -rail.radius * Math.cos(theta);
    return { ...spec, rect: unit(request.anchor, d, cx, cy, rail.d, rail.d) };
  });
}

/** Push a rect fully into the staging box without resizing it. */
function onPlane(rect: Rect, viewport: PlaneViewport): Rect {
  // Clamped to the **staging box**, not to the plane (issue #534). A wing board
  // deliberately tucks partway offstage, but its crest may not follow it there:
  // the crest is the selection surface for player-targeting and attack
  // declaration and can never degrade away (`layout-model.md` §Staging). Under
  // the contextual shell the plane spans the whole viewport, so clamping to the
  // plane leaves a right-hand wing's crest sitting *under the control cluster* —
  // on canvas, and unpickable. Absent a box this is the plane, which is the
  // pre-#534 behaviour exactly.
  const box = viewport.safe ?? { x: 0, y: 0, w: viewport.width, h: viewport.height };
  return {
    ...rect,
    x: Math.max(box.x, Math.min(rect.x, box.x + box.w - rect.w)),
    y: Math.max(box.y, Math.min(rect.y, box.y + box.h - rect.h)),
  };
}

/**
 * The §9 accessible sentence: the whole seat reads without opening anything.
 * The **full** label and the **exact** life value ride here even where the plate
 * truncated and the medallion clamped (§7).
 */
function ariaFor(facts: SeatClusterFacts, chips: ClusterChip[]): string {
  const who = facts.local ? `${facts.label} (you)` : facts.label;
  const parts = [
    `${who}, ${facts.life} life, ${facts.handCount} in hand, ${facts.libraryCount} in library`,
  ];
  if (facts.eliminated) parts.push('eliminated');
  if (facts.active) parts.push('active turn');
  if (facts.priority) parts.push('has priority');
  if (facts.focused) parts.push('focused');
  if (facts.commanderPresent) parts.push('has a commander');
  for (const chip of chips) parts.push(chip.label);
  return parts.join(', ');
}

/**
 * Stage one seat's identity cluster.
 *
 * The medallion is placed on `anchor`, everything else is measured from its
 * centre in `D`, and every drawn rect is then pushed onto the plane — the
 * cluster is the player-targeting surface and can never degrade away
 * (`layout-model.md` §Staging), so no part of it may sit off-canvas.
 */
export function stageSeatCluster(request: SeatClusterRequest): SeatCluster {
  const { seat, variant, anchor, viewport, facts } = request;
  const d = clusterD(variant, viewport);
  const c = PLANE.cluster;
  const minimal = variant === 'minimal';

  const portrait = onPlane(unit(anchor, d, 0, 0, 1, 1), viewport);
  const life = minimal
    ? unit(anchor, d, c.minimalLife.cx, c.minimalLife.cy, c.minimalLife.d, c.minimalLife.d)
    : unit(anchor, d, 0, c.life.cy, c.life.d, c.life.d);

  const resolved = minimal ? undefined : resolvePlate(request, d);
  let plate: ClusterPlate | undefined;
  if (resolved) {
    const fitted = fitName(facts.label, plateCapacity(resolved.len, d));
    const len = plateLength(graphemes(fitted.text).length, d);
    plate = {
      rect: onPlane(
        plateRect(request, d, resolved.direction, Math.min(len, resolved.len)),
        viewport,
      ),
      direction: resolved.direction,
      text: fitted.text,
      truncated: fitted.truncated,
    };
  }

  // The hexagon sits opposite the plate, which is §8's arrangement wherever §8
  // draws the plate outboard, and the only non-overlapping placement wherever
  // the departure above moved it. A plate hung below leaves §8's side free.
  const plateSide = plate?.direction;
  const pipSign =
    plateSide === 'right' ? -1 : plateSide === 'left' ? 1 : request.outboard === 'left' ? -1 : 1;
  let pip: ClusterPip | undefined;
  if (!minimal && !facts.eliminated) {
    pip =
      variant === 'compact'
        ? {
            rect: onPlane(unit(anchor, d, 0, c.tab.cy, c.tab.w, c.tab.h), viewport),
            shape: 'tab',
            count: facts.handCount,
          }
        : {
            rect: onPlane(
              unit(anchor, d, pipSign * c.pip.cx, c.life.cy, c.pip.w, c.pip.h),
              viewport,
            ),
            shape: 'hex',
            count: facts.handCount,
          };
  }

  let gem: SeatCluster['gem'];
  if (facts.gem !== undefined && plate !== undefined) {
    const rect =
      plate.direction === 'below'
        ? {
            x: plate.rect.x + 2,
            y: plate.rect.y + (plate.rect.h - c.gem.s * d) / 2,
            w: c.gem.s * d,
            h: c.gem.s * d,
          }
        : unit(anchor, d, (plate.direction === 'left' ? -1 : 1) * c.gem.cx, 0, c.gem.s, c.gem.s);
    gem = { rect: onPlane(rect, viewport), identity: facts.gem };
  }

  const chips = facts.eliminated
    ? []
    : placeChips(request, d, chipSpecs(facts)).map((chip) => ({
        ...chip,
        rect: onPlane(chip.rect, viewport),
      }));

  const glow = unit(anchor, d, 0, 0, 2 * c.priorityOuter, 2 * c.priorityOuter);
  const bounds = union([
    glow,
    portrait,
    life,
    ...(plate ? [plate.rect] : []),
    ...(pip ? [pip.rect] : []),
    ...chips.map((chip) => chip.rect),
  ]);

  return {
    seat,
    name: facts.local ? `${facts.label} (you)` : facts.label,
    variant,
    d,
    portrait,
    hit: onPlane(hitRectFor(portrait), viewport),
    life: onPlane(life, viewport),
    lifeGlyph: lifeGlyphFor(facts.life, facts.eliminated),
    lifeGlyphs: graphemes(lifeGlyphFor(facts.life, facts.eliminated)).length,
    urgency: urgencyFor(facts.life, facts.eliminated),
    plate,
    gem,
    pip,
    chips,
    channels: {
      priority: facts.priority,
      active: facts.active,
      attacked: facts.attacked,
      focused: facts.focused,
      eliminated: facts.eliminated,
      commander: facts.commanderPresent,
      deadline: facts.deadline,
      // Dormant until a per-seat connection field exists (§11, issue #553).
      disconnected: false,
    },
    portraitSrc: facts.portrait?.src,
    accent: facts.accent,
    bounds,
    ariaLabel: ariaFor(facts, chips),
  };
}
