/**
 * The **chrome meta builders** for the DOM scene plane: every non-geometry input
 * a region slot, an identity cluster, a zone pile, a digest rack button, or a
 * compact summary tile publishes as a `data-*` attribute or a CSS custom
 * property, so the stylesheet selects on state rather than the reconciler
 * deciding paint.
 *
 * Split out of `planeReconciler.ts` when the seat identity cluster (issue #532)
 * pushed it past the file-size ceiling of `docs/coding-standards.md`. Pure code
 * motion: these are the same functions, in the same order, and the reconciler
 * is their only caller.
 *
 * Two rules to keep in mind when adding to them:
 *
 * - **Key order is fixed.** `upsert` writes the meta object's keys in iteration
 *   order and drops keys the new meta no longer carries, so an optional key must
 *   always append at the same point — otherwise a reconciled element serialises
 *   differently from a fresh mount and fresh-mount equivalence fails.
 * - **A count has exactly one home** (`zone-geography.md` §4/I5): the library,
 *   graveyard, exile, and command counts belong to their piles, and the hand
 *   count to the identity cluster. Nothing here may draw one twice.
 */
import type { PlayerId } from '../protocol';
import type {
  ClusterChip,
  HandFanSlot,
  PlaneRegion,
  RackSlot,
  SeatCluster,
  SummaryTileSlot,
} from './plane';

/** A region's non-geometry inputs as data attributes. */
export function regionMeta(region: PlaneRegion): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    kind: region.kind,
    rung: String(region.rung),
    surface: region.surface,
    label: region.label,
    life: String(region.life),
    hand: String(region.handCount),
    focused: String(region.focused),
    eliminated: String(region.eliminated),
    attacked: String(region.attacked),
    active: String(region.active),
    priority: String(region.priority),
  };
  if (region.side !== undefined) meta.side = region.side;
  if (region.rank !== undefined) meta.rank = String(region.rank);
  if (region.digest) {
    meta.digestCreatures = String(region.digest.creatures);
    meta.digestOthers = String(region.digest.others);
    meta.digestLands = String(region.digest.lands);
  }
  return meta;
}

/**
 * The cluster's two non-enumerable inputs, as custom properties: the seat's
 * accent (a token value, not a state) and the portrait plate's URL (`attr()`
 * cannot produce a `url()`). `--cluster-d` carries the rung's scale unit so
 * every painted proportion in the stylesheet is a fraction of `D`, exactly as
 * `seat-identity.md` §1.1 requires — one scale unit per cluster, nothing inside
 * it scaling independently.
 */
export function clusterVars(cluster: SeatCluster): Record<string, string> {
  const vars: Record<string, string> = {
    '--cluster-d': `${cluster.d}px`,
    '--seat-accent': cluster.accent,
  };
  if (cluster.portraitSrc !== undefined) vars['--portrait-src'] = `url("${cluster.portraitSrc}")`;
  return vars;
}

/**
 * The portrait medallion's non-geometry inputs. The crest is staged at every
 * count and every rung, so the markers that must never degrade away ride it: the
 * seat's life/hand readout, the priority ring, the active-turn pennant, and the
 * **attacked ring** — combat against any seat is drawn regardless of which board
 * holds focus (layout-model §Focus model, "off-focus activity is never silent").
 *
 * `monogram` is the procedural rune the aperture draws when no plate resolved;
 * `portrait` says which of the two the medallion is showing, so the fallback is
 * a state the DOM declares rather than a CSS accident.
 */
export function crestMeta(region: PlaneRegion): Record<string, string> {
  const cluster = region.cluster;
  return {
    seat: region.seat,
    life: String(region.life),
    hand: String(region.handCount),
    attacked: String(region.attacked),
    priority: String(region.priority),
    variant: cluster.variant,
    active: String(cluster.channels.active),
    focused: String(cluster.channels.focused),
    eliminated: String(cluster.channels.eliminated),
    commander: String(cluster.channels.commander),
    deadline: String(cluster.channels.deadline),
    // Dormant until a per-seat connection field lands (§11, issue #553).
    disconnected: String(cluster.channels.disconnected),
    portrait: String(cluster.portraitSrc !== undefined),
    monogram: cluster.monogram,
  };
}

/**
 * One back in an opponent's face-down hand fan (issue #533).
 *
 * **This is the whole published surface of a hidden card, and it is two
 * values.** The seat (which fan it belongs to) and the slot's own index. No
 * card, no id, no name, no colour, no zone — and, deliberately, **no count**:
 * the hand count has exactly one home, the identity cluster's pip
 * (`zone-geography.md` §4/I5), so the fan may not draw it either. The slot's
 * rotation rides as `--fan-angle` because a degree value is not an enum, and it
 * is a function of `(index, count)` alone (`table/handFan.ts`).
 */
export function handFanSlotMeta(seat: PlayerId, slot: HandFanSlot): Record<string, string> {
  return { seat, index: String(slot.index) };
}

/** The nameplate's inputs: the fitted text and which way the plate runs (§7/§8). */
export function plateMeta(region: PlaneRegion): Record<string, string> {
  const plate = region.cluster.plate!;
  return {
    seat: region.seat,
    name: plate.text,
    direction: plate.direction,
    truncated: String(plate.truncated),
    variant: region.cluster.variant,
    eliminated: String(region.cluster.channels.eliminated),
  };
}

/**
 * The life medallion's inputs. `glyph` is what the medallion draws — the numeral
 * verbatim, or §6.5's struck rune on an eliminated seat, where the *number* is
 * removed rather than shown as `0` (a live seat may legitimately sit at `0` for
 * an instant). `glyphs` steps the numeral's size and `urgency` is §5.1's shape
 * channel: a threshold on a displayed number, never a declaration of loss.
 */
export function lifeMeta(region: PlaneRegion): Record<string, string> {
  const cluster = region.cluster;
  return {
    seat: region.seat,
    glyph: cluster.lifeGlyph,
    glyphs: String(cluster.lifeGlyphs),
    urgency: cluster.urgency,
    eliminated: String(cluster.channels.eliminated),
  };
}

/** One status-rail medallion's inputs (§5.4 escalation shapes included). */
export function chipMeta(seat: PlayerId, chip: ClusterChip): Record<string, string> {
  const meta: Record<string, string> = {
    seat,
    kind: chip.kind,
    glyph: chip.glyph,
    label: chip.label,
  };
  if (chip.value !== undefined) meta.value = chip.value;
  if (chip.escalation !== undefined) meta.escalation = chip.escalation;
  return meta;
}

/** A seat's zone-pile counts as data attributes (the authoritative pile data a
 * draw or a battlefield→graveyard move must reconcile, slots unmoved). */
export function zonesMeta(zones: PlaneRegion['zones']): Record<string, string> {
  const meta: Record<string, string> = {
    library: String(zones.library),
    graveyard: String(zones.graveyard),
    exile: String(zones.exile),
    command: String(zones.command ?? 0),
  };
  if (zones.graveyardTop) {
    meta.top = zones.graveyardTop.name;
    meta.topColor = zones.graveyardTop.colorIdentity;
  }
  return meta;
}

/**
 * One drawn zone slot's non-geometry inputs (`zone-geography.md` §3): which zone
 * it is — the channel the pile's silhouette and material read from, so no two
 * piles look like the same object — its own count, and the seat's eliminated
 * treatment. The count is the pile's **only** home (§4/I5); nothing else in the
 * scene may draw it.
 */
export function zoneSlotMeta(region: PlaneRegion, slot: RackSlot): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    zone: slot.zone,
    count: String(slot.count),
    variant: region.rack.variant,
    eliminated: String(region.eliminated),
  };
  // Hidden stays hidden (§I2): the library never publishes a top card. Only the
  // graveyard's is public in the view, and only there does one reach the DOM.
  if (slot.zone === 'graveyard' && region.zones.graveyardTop) {
    meta.top = region.zones.graveyardTop.name;
    meta.topColor = region.zones.graveyardTop.colorIdentity;
  }
  return meta;
}

/** The digest rack button's inputs: every zone's count on one ≥ 44 px target. */
export function rackMeta(region: PlaneRegion, slots: readonly RackSlot[]): Record<string, string> {
  const meta: Record<string, string> = {
    seat: region.seat,
    variant: 'digest',
    eliminated: String(region.eliminated),
    zones: slots.map((slot) => slot.zone).join(' '),
  };
  for (const slot of slots) meta[slot.zone] = String(slot.count);
  return meta;
}

/** A compact tile's non-geometry inputs as data attributes (a tile owes the
 * seat's zone counts too — they are its whole summary). */
export function tileMeta(tile: SummaryTileSlot): Record<string, string> {
  return {
    seat: tile.seat,
    label: tile.label,
    life: String(tile.life),
    hand: String(tile.handCount),
    overflow: String(tile.candidateOverflow),
    eliminated: String(tile.eliminated),
    attacked: String(tile.attacked),
    active: String(tile.active),
    priority: String(tile.priority),
    ...zonesMeta(tile.zones),
  };
}
