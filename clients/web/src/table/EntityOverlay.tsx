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
import { entityActions, hotspot, inspectHandle, overlay, targetHotspot } from './styles';
import s from './chrome.module.css';

interface Props {
  /** The scene whose actionable (or targetable) cards get overlay affordances. */
  scene: TableScene;
  /** The currently selected entity, if any (ignored in targeting mode). */
  selectedId: EntityId | null;
  /** Whether targeting mode is active — picks a target instead of an action. */
  targeting: boolean;
  /**
   * Whether the active targeting slot is a **multi-select** (issue #143): a
   * candidate click toggles it into the answer (staying in the slot) rather than
   * picking-and-submitting. The chosen cards render pressed. `false` for the
   * single-target flow, where a click picks exactly one and submits.
   */
  multiSelect?: boolean;
  /** Toggle selection of an entity (the select step). */
  onSelect: (id: EntityId) => void;
  /** Confirm one of the entity's offered actions (echoes the `ValidAction`). */
  onChoose: (action: ValidAction) => void;
  /** Pick an entity as the current target slot's answer (targeting mode). */
  onPickTarget: (id: EntityId) => void;
  /**
   * Open the inspect popover for a card (issue #261). When provided, every card in
   * the scene gets an inspect handle — a distinct control from its select/target
   * hotspot, so inspect works on any card (own, opponent's, actionable or not) and
   * coexists with targeting/multi-select. Absent ⇒ no handles are drawn.
   */
  onInspect?: (id: EntityId) => void;
}

export function EntityOverlay({
  scene,
  selectedId,
  targeting,
  multiSelect = false,
  onSelect,
  onChoose,
  onPickTarget,
  onInspect,
}: Props) {
  const allCards: RenderedCard[] = [...scene.bands.flatMap((band) => band.cards), ...scene.hand];

  // In targeting mode the only interactive cards are the server-listed candidates;
  // otherwise it is every card that carries a subject-action.
  const interactive = targeting
    ? allCards.filter((card) => card.targetable)
    : allCards.filter((card) => card.actions.length > 0);

  return (
    <div style={overlay(scene.width, scene.height)}>
      {/*
       * Inspect handles for every card (issue #261). Rendered first but stacked
       * above via z-index, so inspect is reachable on any card — including cards
       * with no action (opponent permanents) that carry no select hotspot — without
       * changing what the select/target hotspots below offer.
       */}
      {onInspect &&
        allCards.map((card) => (
          <button
            key={`inspect-${card.entityId}`}
            type="button"
            data-testid={`inspect-${card.entityId}`}
            data-entity={card.entityId}
            aria-label={`Inspect ${card.name}`}
            onClick={() => onInspect(card.entityId)}
            className={s.canvasControl}
            style={inspectHandle(card.rect)}
          >
            i
          </button>
        ))}
      {interactive.map((card) => {
        if (targeting) {
          // A multi-select candidate toggles (pressed when chosen); a single-target
          // candidate picks-and-submits. Both only ever fire on server candidates.
          const verb = multiSelect ? 'Toggle' : 'Target';
          return (
            <button
              key={card.entityId}
              type="button"
              data-testid={`target-${card.entityId}`}
              data-entity={card.entityId}
              aria-label={`${verb} ${card.name}`}
              aria-pressed={multiSelect ? card.chosen : undefined}
              onClick={() => onPickTarget(card.entityId)}
              className={s.canvasControl}
              style={targetHotspot(card.rect, card.chosen)}
            />
          );
        }
        const selected = selectedId === card.entityId;
        // The select hotspot is only rendered on cards that carry an action, so
        // this list is always non-empty. Naming the offered action(s) gives the
        // canvas's visual "playable" edge bar an accessible-tree equivalent for a
        // screen-reader / no-color-vision user (issue #277, ui-requirements §10).
        const actionHint = card.actions.map((action) => action.label).join(', ');
        return (
          <Fragment key={card.entityId}>
            <button
              type="button"
              data-testid={`entity-${card.entityId}`}
              data-entity={card.entityId}
              data-actionable="true"
              aria-pressed={selected}
              aria-label={`${card.name} — playable: ${actionHint}`}
              onClick={() => onSelect(card.entityId)}
              className={s.canvasControl}
              style={hotspot(card.rect, selected)}
            />
            {selected && (
              <div data-testid={`entity-actions-${card.entityId}`} style={entityActions(card.rect)}>
                {card.actions.map((action) => (
                  <button
                    key={action.id}
                    type="button"
                    onClick={() => onChoose(action)}
                    className={s.chip}
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
