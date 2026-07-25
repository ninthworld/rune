/**
 * Semantic interaction layer for the staged 2.5D plane.
 *
 * Controls are positioned from the latest {@link StagedPlane}, never from the
 * reconciler's animated transform. The authoritative destination is therefore
 * focusable and hit-testable as soon as a view arrives, even while the painted
 * card is still travelling toward it.
 */
import { useEffect, useMemo, useState } from 'react';
import type { EntityId, GameView, PlayerId, ValidAction } from '../../protocol';
import type { PlaneRegion, PlaneRender, RackZone, StagedPlane, SummaryTileSlot } from '../plane';
import { digestExpansionRects } from '../plane';
import { planeRegions } from '../planeReconciler';
import { declarationFor } from '../scene/action-helpers';
import type { Rect } from '../scene';
import styles from './live-plane.module.css';

export interface LivePlaneInteractionProps {
  selectedId: EntityId | null;
  picking: boolean;
  multiSelect: boolean;
  candidates: EntityId[];
  chosen: EntityId[];
  playerCandidates: PlayerId[];
  dropBoard?: boolean;
  dropCandidates?: EntityId[];
  onActivateEntity: (id: EntityId) => void;
  onPickEntity: (id: EntityId) => void;
  onPickPlayer: (id: PlayerId) => void;
  onPreviewTarget?: (id: EntityId | PlayerId | null) => void;
  onInspect: (id: EntityId) => void;
  onOpenZone: (id: PlayerId, zone: 'graveyard' | 'exile') => void;
  onFocusSeat: (id: PlayerId) => void;
  onFocusGeometry?: (geometry: Map<string, Rect>, plane: { width: number; height: number }) => void;
}

interface Props {
  view: GameView;
  plane: StagedPlane;
  interaction: LivePlaneInteractionProps;
}

function box(rect: Rect): React.CSSProperties {
  return {
    left: rect.x,
    top: rect.y,
    width: rect.w,
    height: rect.h,
  };
}

function actionsFor(render: PlaneRender, actions: ValidAction[]): ValidAction[] {
  return actions.filter((action) => action.subject?.some((id) => render.memberIds.includes(id)));
}

function declarationForRender(
  render: PlaneRender,
  actions: ValidAction[],
): { id: EntityId; action: ValidAction } | undefined {
  for (const id of render.memberIds) {
    const declaration = declarationFor(id, actions);
    if (declaration) return { id, action: declaration };
  }
  return undefined;
}

function interactionId(render: PlaneRender, candidates: Set<EntityId>): EntityId {
  return render.memberIds.find((id) => candidates.has(id)) ?? render.entityId;
}

/**
 * The public zones a rack offers as browse targets, in rack order.
 *
 * The library is deliberately absent: activating it never browses
 * (zone-geography §I2 / §6.2) and no wire action names it, so offering one would
 * be a client-invented affordance. The command slot is likewise absent — its
 * popover needs commander data this view does not carry (§12 gap G3).
 */
const BROWSABLE = ['graveyard', 'exile'] as const;

/**
 * A zone's hotspot on the seat's rack. Every slot already carries a ≥ 44 px
 * `hitRect` (zone-geography §2.3), so this is a lookup — never a re-derivation
 * of the geometry.
 *
 * A **digest** rack resolves every zone key to the one button (§7), which is
 * right for anchors and wrong for targets: two controls sharing one rect overlap
 * exactly, and only the later one is reachable by pointer or touch. Digest racks
 * therefore never come through here — see {@link browsableRects}.
 */
function zoneHit(region: PlaneRegion, zone: RackZone): Rect {
  return region.rack.slots.find((slot) => slot.zone === zone)?.hitRect ?? region.piles;
}

/** A zone's count, for the accessible name (§8.1 — the count is on the pile). */
function zoneCount(region: PlaneRegion, zone: RackZone): number {
  return region.rack.slots.find((slot) => slot.zone === zone)?.count ?? 0;
}

/**
 * Where each browsable zone's control sits. A drawn rack puts them on their own
 * slots' hit rects (§8.1); a digest rack has no separable slots, so its controls
 * live on the §6.2 expansion rects the button opens — which is what keeps the
 * graveyard reachable by pointer and touch rather than keyboard-only.
 */
function browsableRects(region: PlaneRegion, plane: PlaneSize): Rect[] {
  if (region.rack.variant !== 'digest') return BROWSABLE.map((zone) => zoneHit(region, zone));
  return digestExpansionRects(region.rack, BROWSABLE.length, plane);
}

/** The plane's logical size — all the expansion geometry needs. */
interface PlaneSize {
  width: number;
  height: number;
}

/**
 * The digest rack button's accessible name (§8.1): the seat, what activating it
 * does, and every count, so the rack still announces the state it collapsed.
 */
function digestLabel(region: PlaneRegion): string {
  const parts = region.rack.slots.map((slot) => `${slot.zone} ${slot.count}`);
  return `Open ${region.label} zones: ${parts.join(', ')}`;
}

/**
 * The seat's browse controls.
 *
 * A drawn rack gives each public zone its own hotspot, exactly as §8.1 says. A
 * **digest** rack cannot: all four zone keys resolve to the one button, so a
 * per-zone control per zone would stack two identical rects and hand pointer and
 * touch to whichever painted last — leaving the graveyard keyboard-only, which
 * fails the universal-path contract. §6.2 answers that with the expansion: one
 * ≥ 44 px button that opens the seat's zones as separate, non-overlapping
 * targets on the §6.2 expansion rects. Both forms route to the same
 * `onOpenZone`; no new action, no new wire message.
 */
function ZoneControls({
  region,
  plane,
  interaction,
  expanded,
  onToggleExpand,
}: {
  region: PlaneRegion;
  plane: PlaneSize;
  interaction: LivePlaneInteractionProps;
  expanded: boolean;
  onToggleExpand: (seat: PlayerId | null) => void;
}) {
  const digest = region.rack.variant === 'digest';
  const rects = browsableRects(region, plane);
  const choices = BROWSABLE.map((zone, index) => (
    <button
      key={zone}
      type="button"
      className={digest ? styles.zoneChoice : styles.zoneControl}
      style={box(rects[index])}
      data-testid={`table-${zone}-${region.seat}`}
      data-focus-key={`${zone}:${region.seat}`}
      data-count={zoneCount(region, zone)}
      aria-label={`Browse ${region.label} ${zone}, ${zoneCount(region, zone)} cards`}
      onClick={() => {
        onToggleExpand(null);
        interaction.onOpenZone(region.seat, zone);
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') onToggleExpand(null);
      }}
    >
      {digest ? zone : null}
    </button>
  ));
  if (!digest) return <>{choices}</>;
  return (
    <>
      <button
        type="button"
        className={styles.rackControl}
        style={box(region.rack.bounds)}
        data-testid={`rack-digest-${region.seat}`}
        data-focus-key={`rack:${region.seat}`}
        aria-haspopup="true"
        aria-expanded={expanded}
        aria-label={digestLabel(region)}
        onClick={() => onToggleExpand(expanded ? null : region.seat)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') onToggleExpand(null);
        }}
      />
      {/* Expansion is presentation state, dropped on the next view (§6.2 / I4). */}
      {expanded && choices}
    </>
  );
}

function RegionControls({
  region,
  plane,
  view,
  interaction,
  candidateSet,
  playerCandidateSet,
  expanded,
  onToggleExpand,
}: {
  region: PlaneRegion;
  plane: PlaneSize;
  view: GameView;
  interaction: LivePlaneInteractionProps;
  candidateSet: Set<EntityId>;
  playerCandidateSet: Set<PlayerId>;
  expanded: boolean;
  onToggleExpand: (seat: PlayerId | null) => void;
}) {
  const playerTarget = playerCandidateSet.has(region.seat);
  return (
    <div
      className={styles.controlRegion}
      data-focus-region={`plane-seat-${region.seat}`}
      data-drop-receiver={interaction.dropBoard && region.seat === view.you ? 'true' : undefined}
    >
      <button
        type="button"
        className={playerTarget ? styles.targetControl : styles.crestControl}
        style={box(region.crest)}
        data-testid={playerTarget ? `target-player-${region.seat}` : `focus-seat-${region.seat}`}
        data-focus-key={`crest:${region.seat}`}
        aria-label={
          playerTarget ? `Target player ${region.label}` : `Focus ${region.label} battlefield`
        }
        onClick={() =>
          playerTarget
            ? interaction.onPickPlayer(region.seat)
            : interaction.onFocusSeat(region.seat)
        }
        onPointerEnter={() => {
          if (playerTarget) interaction.onPreviewTarget?.(region.seat);
        }}
        onPointerLeave={() => {
          if (playerTarget) interaction.onPreviewTarget?.(null);
        }}
        onFocus={() => {
          if (playerTarget) interaction.onPreviewTarget?.(region.seat);
        }}
        onBlur={() => {
          if (playerTarget) interaction.onPreviewTarget?.(null);
        }}
      />
      <ZoneControls
        region={region}
        plane={plane}
        interaction={interaction}
        expanded={expanded}
        onToggleExpand={onToggleExpand}
      />
      {region.renders.map((render) => {
        const actions = actionsFor(render, view.valid_actions);
        const declaration = declarationForRender(render, view.valid_actions);
        const actionable = actions.length > 0 || declaration !== undefined;
        const actionMember = actions
          .flatMap((action) => action.subject ?? [])
          .find((id) => render.memberIds.includes(id));
        const id =
          render.memberIds.find((memberId) => candidateSet.has(memberId)) ??
          actionMember ??
          declaration?.id ??
          render.entityId;
        const targetable = interaction.picking && candidateSet.has(id);
        if (targetable) {
          return (
            <button
              key={render.entityId}
              type="button"
              className={styles.targetControl}
              style={box(render.hitRect)}
              data-testid={`target-${id}`}
              data-focus-key={`entity:${id}`}
              data-entity={id}
              aria-label={`${interaction.multiSelect ? 'Toggle' : 'Target'} ${render.name}`}
              aria-pressed={interaction.multiSelect ? interaction.chosen.includes(id) : undefined}
              onClick={() => interaction.onPickEntity(id)}
              onPointerEnter={() => interaction.onPreviewTarget?.(id)}
              onPointerLeave={() => interaction.onPreviewTarget?.(null)}
              onFocus={() => interaction.onPreviewTarget?.(id)}
              onBlur={() => interaction.onPreviewTarget?.(null)}
              onContextMenu={(event) => {
                event.preventDefault();
                interaction.onInspect(id);
              }}
            />
          );
        }
        if (!interaction.picking && actionable) {
          const hint =
            actions.length > 0
              ? actions.map((action) => action.label).join(', ')
              : (declaration?.action.label ?? '');
          return (
            <button
              key={render.entityId}
              type="button"
              className={styles.entityControl}
              style={box(render.hitRect)}
              data-testid={`entity-${id}`}
              data-focus-key={`entity:${id}`}
              data-entity={id}
              data-actionable="true"
              aria-label={`${render.name} — playable: ${hint}`}
              aria-pressed={render.memberIds.includes(interaction.selectedId ?? '')}
              onClick={() => interaction.onActivateEntity(id)}
              onContextMenu={(event) => {
                event.preventDefault();
                interaction.onInspect(id);
              }}
            />
          );
        }
        return (
          <button
            key={render.entityId}
            type="button"
            className={styles.inspectControl}
            style={box(render.hitRect)}
            data-testid={`inspect-surface-${id}`}
            data-focus-key={`entity:${id}`}
            data-entity={id}
            aria-label={`Inspect ${render.name}`}
            onClick={() => interaction.onInspect(id)}
          />
        );
      })}
    </div>
  );
}

function TileControls({
  tile,
  interaction,
  candidateSet,
  playerCandidateSet,
}: {
  tile: SummaryTileSlot;
  interaction: LivePlaneInteractionProps;
  candidateSet: Set<EntityId>;
  playerCandidateSet: Set<PlayerId>;
}) {
  const playerTarget = playerCandidateSet.has(tile.seat);
  return (
    <div className={styles.controlRegion} data-focus-region={`plane-tile-${tile.seat}`}>
      <button
        type="button"
        className={playerTarget ? styles.targetControl : styles.tileControl}
        style={box(tile.rect)}
        data-testid={playerTarget ? `target-player-${tile.seat}` : `focus-seat-${tile.seat}`}
        data-focus-key={`tile:${tile.seat}`}
        data-candidate-overflow={tile.candidateOverflow || undefined}
        aria-label={
          playerTarget
            ? `Target player ${tile.label}`
            : tile.candidateOverflow > 0
              ? `Focus ${tile.label} battlefield to choose ${tile.candidateOverflow} more candidates`
              : `Focus ${tile.label} battlefield`
        }
        onClick={() =>
          playerTarget ? interaction.onPickPlayer(tile.seat) : interaction.onFocusSeat(tile.seat)
        }
        onPointerEnter={() => {
          if (playerTarget) interaction.onPreviewTarget?.(tile.seat);
        }}
        onPointerLeave={() => {
          if (playerTarget) interaction.onPreviewTarget?.(null);
        }}
        onFocus={() => {
          if (playerTarget) interaction.onPreviewTarget?.(tile.seat);
        }}
        onBlur={() => {
          if (playerTarget) interaction.onPreviewTarget?.(null);
        }}
      />
      {tile.candidates.map((render) => {
        const id = interactionId(render, candidateSet);
        return (
          <button
            key={render.entityId}
            type="button"
            className={styles.targetControl}
            style={box(render.hitRect)}
            data-testid={`target-${id}`}
            data-focus-key={`entity:${id}`}
            data-entity={id}
            aria-label={`${interaction.multiSelect ? 'Toggle' : 'Target'} ${render.name}`}
            aria-pressed={interaction.multiSelect ? interaction.chosen.includes(id) : undefined}
            onClick={() => interaction.onPickEntity(id)}
            onPointerEnter={() => interaction.onPreviewTarget?.(id)}
            onPointerLeave={() => interaction.onPreviewTarget?.(null)}
            onFocus={() => interaction.onPreviewTarget?.(id)}
            onBlur={() => interaction.onPreviewTarget?.(null)}
            onContextMenu={(event) => {
              event.preventDefault();
              interaction.onInspect(id);
            }}
          />
        );
      })}
    </div>
  );
}

/** Render semantic buttons over every staged interaction surface. */
export function LivePlaneControls({ view, plane, interaction }: Props) {
  const candidateSet = useMemo(() => new Set(interaction.candidates), [interaction.candidates]);
  const playerCandidateSet = useMemo(
    () => new Set(interaction.playerCandidates),
    [interaction.playerCandidates],
  );
  const dropCandidateSet = useMemo(
    () => new Set(interaction.dropCandidates ?? []),
    [interaction.dropCandidates],
  );
  // Which seat's digest rack is expanded (§6.2). At most one, and a fresh
  // GameView drops it — expansion is presentation state, never load-bearing.
  const [expandedRack, setExpandedRack] = useState<PlayerId | null>(null);
  useEffect(() => setExpandedRack(null), [view]);

  useEffect(() => {
    const geometry = new Map<string, Rect>();
    for (const region of planeRegions(plane)) {
      geometry.set(`crest:${region.seat}`, region.crest);
      // Spatial focus navigation reads the rect a control actually occupies, so
      // a digest seat publishes its button plus the expansion rects its zone
      // controls open onto — not one rect wearing three keys.
      const rects = browsableRects(region, plane);
      BROWSABLE.forEach((zone, index) => geometry.set(`${zone}:${region.seat}`, rects[index]));
      if (region.rack.variant === 'digest') geometry.set(`rack:${region.seat}`, region.rack.bounds);
      for (const render of region.renders) {
        for (const id of render.memberIds) geometry.set(`entity:${id}`, render.hitRect);
      }
    }
    for (const tile of plane.tiles) {
      geometry.set(`tile:${tile.seat}`, tile.rect);
      for (const render of tile.candidates) {
        for (const id of render.memberIds) geometry.set(`entity:${id}`, render.hitRect);
      }
    }
    interaction.onFocusGeometry?.(geometry, { width: plane.width, height: plane.height });
  }, [interaction, plane]);

  return (
    <div className={styles.controls} data-testid="live-plane-controls">
      {interaction.dropBoard && plane.receiver && (
        <div
          className={styles.dropBoard}
          style={box(plane.receiver.rect)}
          data-testid="drop-board"
          data-drop-receiver="true"
          aria-hidden="true"
        />
      )}
      {planeRegions(plane)
        .flatMap((region) => region.renders)
        .concat(plane.tiles.flatMap((tile) => tile.candidates))
        .filter((render) => render.memberIds.some((id) => dropCandidateSet.has(id)))
        .map((render) => (
          <div
            key={`drop-${render.entityId}`}
            className={styles.dropTarget}
            style={box(render.hitRect)}
            data-testid={`drop-target-${interactionId(render, dropCandidateSet)}`}
            aria-hidden="true"
          />
        ))}
      {planeRegions(plane).map((region) => (
        <RegionControls
          key={region.seat}
          region={region}
          plane={plane}
          view={view}
          interaction={interaction}
          candidateSet={candidateSet}
          playerCandidateSet={playerCandidateSet}
          expanded={expandedRack === region.seat}
          onToggleExpand={setExpandedRack}
        />
      ))}
      {plane.tiles.map((tile) => (
        <TileControls
          key={tile.seat}
          tile={tile}
          interaction={interaction}
          candidateSet={candidateSet}
          playerCandidateSet={playerCandidateSet}
        />
      ))}
    </div>
  );
}
