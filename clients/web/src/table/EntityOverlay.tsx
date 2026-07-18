/**
 * The interactive DOM overlay anchored over the Pixi canvas (ADR 0003: DOM
 * anchors to canvas objects via reported rects; the DOM never reaches into the
 * scene). This is where subject-owned action routing (ADR 0004, reinterpreted by
 * ADR 0023) becomes tangible:
 *
 * - Every entity that carries `valid_actions` gets a focusable, touch-sized
 *   hotspot ON the card — the "select" step of select-then-confirm.
 * - Selecting an entity lifts/rings it and routes its offered actions to the
 *   **action dock** — the one action home. Per-card action popups are abolished
 *   (a popup under a bottom-edge card is guaranteed to clip; in the fixed shell
 *   it cannot exist).
 *
 * In **targeting mode** the same overlay drives target picking (ADR 0009 §Client):
 * every legal target the server listed gets a hotspot ON the card (select-then-
 * confirm on the target entity, per ADR 0004), and nothing else is interactive —
 * ineligible cards are dimmed in the canvas and carry no hotspot.
 *
 * **Inspect (issue #321) rides the interactions the player is already making** — no
 * card carries a permanently visible inspect handle. Selecting a card surfaces its
 * preview (the caller shows it in one consistent home). Precise pointers get a
 * hover-dwell peek; touch gets a long-press peek; right-click and keyboard activate
 * pin the full panel. A card with no select/target hotspot (an opponent's permanent,
 * an inert hand card) hosts these gestures on a transparent, focusable inspect
 * surface, so inspect reaches every card in every input mode without board noise.
 */
import { useCallback, useEffect, useRef } from 'react';
import type { MouseEvent as ReactMouseEvent, PointerEvent as ReactPointerEvent } from 'react';
import type { EntityId } from '../protocol';
import type { RenderedCard, TableScene } from './scene';
import { hotspot, inspectSurface, overlay, targetHotspot } from './styles';
import s from './chrome.module.css';

/** Precise-pointer hover-dwell delay before a peek opens (ms). */
const DWELL_MS = 400;
/** Touch long-press delay before a peek opens (ms). */
const LONG_PRESS_MS = 500;

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
  /**
   * The environment's pointer precision (issue #321): `fine` enables the hover-dwell
   * peek; `coarse` enables the long-press peek. A capability, not a device.
   */
  pointer?: 'fine' | 'coarse';
  /** Toggle selection of an entity (the select step — actions route to the dock). */
  onSelect: (id: EntityId) => void;
  /** Pick an entity as the current target slot's answer (targeting mode). */
  onPickTarget: (id: EntityId) => void;
  /**
   * Open a **transient peek** for an entity (issue #321), or clear it with `null`.
   * Driven by hover-dwell / long-press; never blocks input. Absent ⇒ no peeks.
   */
  onPeek?: (id: EntityId | null) => void;
  /**
   * **Pin** the full inspect panel for an entity (issue #321) — right-click or
   * keyboard activate on a card. Absent ⇒ no inspect is reachable from the overlay.
   */
  onPinInspect?: (id: EntityId) => void;
}

/** The React pointer handlers the inspect gestures install on a card's layer. */
interface InspectGestures {
  onPointerEnter: (e: ReactPointerEvent) => void;
  onPointerLeave: () => void;
  onPointerDown: (e: ReactPointerEvent) => void;
  onPointerMove: () => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
  onContextMenu: (e: ReactMouseEvent) => void;
}

/**
 * Build the inspect gestures for a card's interactive layer (issue #321). Hover-dwell
 * opens a peek on precise pointers; a touch long-press opens one; movement/release
 * cancels; right-click pins the full panel. Hover and long-press are suppressed while
 * `disabled` (targeting/drag) so a peek never fires mid-pick, but pinning stays. All
 * peeks are transient — the caller renders them non-blocking.
 */
function useInspectGestures(opts: {
  pointer: 'fine' | 'coarse';
  disabled: boolean;
  onPeek?: (id: EntityId | null) => void;
  onPinInspect?: (id: EntityId) => void;
}): (id: EntityId) => InspectGestures {
  const { pointer, disabled, onPeek, onPinInspect } = opts;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const kind = useRef<'hover' | 'press' | null>(null);

  const clear = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    kind.current = null;
  }, []);
  // Cancel any pending timer if the overlay unmounts.
  useEffect(() => clear, [clear]);

  return useCallback(
    (id: EntityId): InspectGestures => ({
      onPointerEnter: (e) => {
        if (disabled || !onPeek || pointer !== 'fine' || e.pointerType === 'touch') return;
        clear();
        kind.current = 'hover';
        timer.current = setTimeout(() => onPeek(id), DWELL_MS);
      },
      onPointerLeave: () => {
        clear();
        onPeek?.(null);
      },
      onPointerDown: (e) => {
        if (disabled || !onPeek) return;
        if (e.pointerType !== 'touch' && pointer !== 'coarse') return;
        clear();
        kind.current = 'press';
        timer.current = setTimeout(() => onPeek(id), LONG_PRESS_MS);
      },
      // Movement cancels a pending long-press (so a scroll/drag never peeks); a
      // hover-dwell timer is left alone so pointer motion inside the card is fine.
      onPointerMove: () => {
        if (kind.current === 'press') clear();
      },
      onPointerUp: () => {
        clear();
        onPeek?.(null);
      },
      onPointerCancel: () => {
        clear();
        onPeek?.(null);
      },
      onContextMenu: (e) => {
        if (!onPinInspect) return;
        e.preventDefault();
        onPinInspect(id);
      },
    }),
    [pointer, disabled, onPeek, onPinInspect, clear],
  );
}

export function EntityOverlay({
  scene,
  selectedId,
  targeting,
  multiSelect = false,
  pointer = 'fine',
  onSelect,
  onPickTarget,
  onPeek,
  onPinInspect,
}: Props) {
  const allCards: RenderedCard[] = [...scene.bands.flatMap((band) => band.cards), ...scene.hand];

  // In targeting mode the only interactive cards are the server-listed candidates;
  // otherwise it is every card that carries a subject-action.
  const interactive = targeting
    ? allCards.filter((card) => card.targetable)
    : allCards.filter((card) => card.actions.length > 0);
  const interactiveIds = new Set(interactive.map((card) => card.entityId));

  // Hover-dwell / long-press are suppressed mid-pick; pinning (right-click / keyboard)
  // stays available so inspect never becomes unreachable.
  const gesturesFor = useInspectGestures({ pointer, disabled: targeting, onPeek, onPinInspect });

  return (
    <div style={overlay(scene.width, scene.height)}>
      {/*
       * Inspect surfaces (issue #321): a transparent, focusable layer over every card
       * that carries NO select/target hotspot, so inspect reaches an opponent's
       * permanent or an inert hand card in every input mode — with no visible handle.
       * Cards that do carry a hotspot host the same gestures on it (below), so a card
       * never stacks two interactive layers.
       */}
      {(onPeek || onPinInspect) &&
        allCards
          .filter((card) => !interactiveIds.has(card.entityId))
          .map((card) => (
            <button
              key={`inspect-${card.entityId}`}
              type="button"
              data-testid={`inspect-surface-${card.entityId}`}
              data-entity={card.entityId}
              aria-label={`Inspect ${card.name}`}
              className={s.canvasControl}
              style={inspectSurface(card.rect)}
              onClick={() => onPinInspect?.(card.entityId)}
              {...gesturesFor(card.entityId)}
            />
          ))}
      {interactive.map((card) => {
        const gestures = gesturesFor(card.entityId);
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
              {...gestures}
            />
          );
        }
        const selected = selectedId === card.entityId;
        // The select hotspot is only rendered on cards that carry an action, so
        // this list is always non-empty. Naming the offered action(s) gives the
        // canvas's visual "playable" edge bar an accessible-tree equivalent for a
        // screen-reader / no-color-vision user (issue #277, ui-requirements §10).
        // Selecting routes the actions to the action dock (ADR 0023 commitment 2)
        // — no per-card popup ever renders on the entity.
        const actionHint = card.actions.map((action) => action.label).join(', ');
        return (
          <button
            key={card.entityId}
            type="button"
            data-testid={`entity-${card.entityId}`}
            data-entity={card.entityId}
            data-actionable="true"
            aria-pressed={selected}
            aria-label={`${card.name} — playable: ${actionHint}`}
            onClick={() => onSelect(card.entityId)}
            className={s.canvasControl}
            style={hotspot(card.rect, selected)}
            {...gestures}
          />
        );
      })}
    </div>
  );
}
