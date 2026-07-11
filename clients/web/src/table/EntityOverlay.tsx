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
 * Cards with no actions get no hotspot: nothing outside `valid_actions[]` is
 * clickable or focusable (hard rule).
 */
import { Fragment } from 'react';
import type { EntityId } from '../protocol';
import type { RenderedCard, TableScene } from './scene';
import { chip, entityActions, hotspot, overlay } from './styles';

interface Props {
  /** The scene whose actionable cards get overlay affordances. */
  scene: TableScene;
  /** The currently selected entity, if any. */
  selectedId: EntityId | null;
  /** Toggle selection of an entity (the select step). */
  onSelect: (id: EntityId) => void;
  /** Confirm one of the entity's offered actions (echoes `valid_actions.id`). */
  onChoose: (actionId: string) => void;
}

export function EntityOverlay({ scene, selectedId, onSelect, onChoose }: Props) {
  const actionable: RenderedCard[] = [
    ...scene.bands.flatMap((band) => band.cards),
    ...scene.hand,
  ].filter((card) => card.actions.length > 0);

  return (
    <div style={overlay(scene.width, scene.height)}>
      {actionable.map((card) => {
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
                    onClick={() => onChoose(action.id)}
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
