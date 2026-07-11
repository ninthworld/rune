/**
 * The interactive DOM overlay anchored over the Pixi canvas (ADR 0003: DOM
 * anchors to canvas objects via reported rects; the DOM never reaches into the
 * scene). This is where subject-owned action routing (ADR 0004) becomes tangible:
 *
 * - Every entity that carries `valid_actions` gets a focusable, touch-sized
 *   hotspot ON the card — the "select" step of select-then-confirm.
 * - Selecting an entity reveals its actions as chips ON the entity, so an entity
 *   action fires from the entity itself (and is echoed in the action bar).
 *
 * In **targeting mode** the same overlay drives target picking (ADR 0009 §Client):
 * every legal target the server listed gets a hotspot ON the card (select-then-
 * confirm on the target entity, per ADR 0004), and nothing else is interactive —
 * ineligible cards are dimmed in the canvas and carry no hotspot. The client
 * computes no legality; it only makes the server's candidates pickable.
 *
 * Cards with no actions (and, in targeting mode, no candidacy) get no hotspot:
 * nothing outside `valid_actions[]` / the server's candidate list is clickable or
 * focusable (hard rule).
 */
import { Fragment } from 'react';
import type { EntityId, ValidAction } from '../protocol';
import type { RenderedCard, TableScene } from './scene';
import { chip, entityActions, hotspot, overlay, targetHotspot } from './styles';

interface Props {
  /** The scene whose actionable (or targetable) cards get overlay affordances. */
  scene: TableScene;
  /** The currently selected entity, if any (ignored in targeting mode). */
  selectedId: EntityId | null;
  /** Whether targeting mode is active — picks a target instead of an action. */
  targeting: boolean;
  /** Toggle selection of an entity (the select step). */
  onSelect: (id: EntityId) => void;
  /** Confirm one of the entity's offered actions (echoes the `ValidAction`). */
  onChoose: (action: ValidAction) => void;
  /** Pick an entity as the current target slot's answer (targeting mode). */
  onPickTarget: (id: EntityId) => void;
}

export function EntityOverlay({
  scene,
  selectedId,
  targeting,
  onSelect,
  onChoose,
  onPickTarget,
}: Props) {
  const allCards: RenderedCard[] = [...scene.bands.flatMap((band) => band.cards), ...scene.hand];

  // In targeting mode the only interactive cards are the server-listed candidates;
  // otherwise it is every card that carries a subject-action.
  const interactive = targeting
    ? allCards.filter((card) => card.targetable)
    : allCards.filter((card) => card.actions.length > 0);

  return (
    <div style={overlay(scene.width, scene.height)}>
      {interactive.map((card) => {
        if (targeting) {
          return (
            <button
              key={card.entityId}
              type="button"
              data-testid={`target-${card.entityId}`}
              aria-label={`Target ${card.name}`}
              onClick={() => onPickTarget(card.entityId)}
              style={targetHotspot(card.rect)}
            />
          );
        }
        const selected = selectedId === card.entityId;
        return (
          <Fragment key={card.entityId}>
            <button
              type="button"
              data-testid={`entity-${card.entityId}`}
              aria-pressed={selected}
              aria-label={`Select ${card.name}`}
              onClick={() => onSelect(card.entityId)}
              style={hotspot(card.rect, selected)}
            />
            {selected && (
              <div data-testid={`entity-actions-${card.entityId}`} style={entityActions(card.rect)}>
                {card.actions.map((action) => (
                  <button
                    key={action.id}
                    type="button"
                    onClick={() => onChoose(action)}
                    style={chip}
                  >
                    {action.label}
                  </button>
                ))}
              </div>
            )}
          </Fragment>
        );
      })}
    </div>
  );
}
