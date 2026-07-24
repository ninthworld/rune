/**
 * Semantic interaction layer for the staged 2.5D plane.
 *
 * Controls are positioned from the latest {@link StagedPlane}, never from the
 * reconciler's animated transform. The authoritative destination is therefore
 * focusable and hit-testable as soon as a view arrives, even while the painted
 * card is still travelling toward it.
 */
import { useEffect, useMemo } from 'react';
import type { EntityId, GameView, PlayerId, ValidAction } from '../../protocol';
import type { PlaneRegion, PlaneRender, StagedPlane, SummaryTileSlot } from '../plane';
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

function RegionControls({
  region,
  view,
  interaction,
  candidateSet,
  playerCandidateSet,
}: {
  region: PlaneRegion;
  view: GameView;
  interaction: LivePlaneInteractionProps;
  candidateSet: Set<EntityId>;
  playerCandidateSet: Set<PlayerId>;
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
      <button
        type="button"
        className={styles.zoneControl}
        style={box({
          x: region.piles.x,
          y: region.piles.y,
          w: Math.max(44, region.piles.w),
          h: 44,
        })}
        data-testid={`table-graveyard-${region.seat}`}
        data-focus-key={`graveyard:${region.seat}`}
        aria-label={`Browse ${region.label} graveyard`}
        onClick={() => interaction.onOpenZone(region.seat, 'graveyard')}
      />
      <button
        type="button"
        className={styles.zoneControl}
        style={box({
          x: region.piles.x,
          y: region.piles.y + Math.max(18, region.piles.h - 28),
          w: Math.max(44, region.piles.w),
          h: 44,
        })}
        data-testid={`table-exile-${region.seat}`}
        data-focus-key={`exile:${region.seat}`}
        aria-label={`Browse ${region.label} exile`}
        onClick={() => interaction.onOpenZone(region.seat, 'exile')}
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

  useEffect(() => {
    const geometry = new Map<string, Rect>();
    for (const region of planeRegions(plane)) {
      geometry.set(`crest:${region.seat}`, region.crest);
      geometry.set(`graveyard:${region.seat}`, {
        x: region.piles.x,
        y: region.piles.y,
        w: Math.max(44, region.piles.w),
        h: 44,
      });
      geometry.set(`exile:${region.seat}`, {
        x: region.piles.x,
        y: region.piles.y + Math.max(18, region.piles.h - 28),
        w: Math.max(44, region.piles.w),
        h: 44,
      });
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
          view={view}
          interaction={interaction}
          candidateSet={candidateSet}
          playerCandidateSet={playerCandidateSet}
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
