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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MouseEvent as ReactMouseEvent, PointerEvent as ReactPointerEvent } from 'react';
import type { EntityId, ValidAction } from '../protocol';
import type { Rect, RenderedCard, TableScene } from './scene';
import {
  dragGhostBox,
  dropBoardInset,
  dropTargetRing,
  hotspot,
  inspectSurface,
  overlay,
  targetHotspot,
} from './styles';
import s from './chrome.module.css';

/** Precise-pointer hover-dwell delay before a peek opens (ms). */
const DWELL_MS = 400;
/** Touch long-press delay before a peek opens (ms). */
const LONG_PRESS_MS = 500;
/** Pointer travel (px) before an armed press becomes a drag rather than a click. */
const DRAG_THRESHOLD = 6;

/** Whether a point (scene coordinates) lands inside a scene rect. */
function contains(rect: Rect, x: number, y: number): boolean {
  return x >= rect.x && x <= rect.x + rect.w && y >= rect.y && y <= rect.y + rect.h;
}

/** The offered action a hand card can be dragged to play, if any: its server-offered
 * play/cast entry. Presentation only — the drop fires exactly this action. */
function playActionOf(card: RenderedCard): ValidAction | undefined {
  if (card.zone !== 'hand') return undefined;
  return card.actions.find((a) => a.type === 'play_land' || a.type === 'cast_spell');
}

/** An in-flight drag: the card, the play action it will fire, and the pointer's
 * current scene position (drives the ghost + drop hit-test). Ephemeral — never
 * survives a view change, let alone a message. */
interface DragSession {
  entityId: EntityId;
  name: string;
  action: ValidAction;
  x: number;
  y: number;
}

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
  /**
   * Fire an untargeted play/cast dropped on the receiver's battlefield — the
   * drag-to-play enhancement (blueprint §Interaction model). Dragging ghosts the
   * card under the pointer and lights the legal drop area; release fires exactly
   * the server-offered action. Absent ⇒ dragging is disabled; the select-then-act
   * path is always available, so drag is an enhancement, never required.
   */
  onPlay?: (action: ValidAction) => void;
  /**
   * Fire a targeted spell dropped on one of its slot-0 candidates: cast + first
   * target in one gesture (any remaining slots continue in the normal targeting
   * flow). The candidates come straight from the action's server-enumerated
   * `requirements` — the overlay derives no legality.
   */
  onPlayOnTarget?: (action: ValidAction, target: EntityId) => void;
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
  onPlay,
  onPlayOnTarget,
}: Props) {
  const allCards: RenderedCard[] = useMemo(
    () => [...scene.bands.flatMap((band) => band.cards), ...scene.hand],
    [scene],
  );

  // ── Drag-to-play (blueprint §Interaction model) ─────────────────────────────
  // An armed press on a playable hand card becomes a drag once the pointer travels
  // past the threshold; a shorter press stays an ordinary click (select). The drop
  // is resolved by hit-testing the pointer against SCENE rects — deterministic
  // where the DOM reports no geometry (jsdom/SSR), the same choice the focus
  // engine makes — never by DOM hit-testing.
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [drag, setDrag] = useState<DragSession | null>(null);
  // Swallow exactly one click after a real drag/cancel, so releasing over the
  // origin card does not also toggle its selection.
  const swallowClick = useRef(false);
  const dragEnabled = !targeting && (onPlay !== undefined || onPlayOnTarget !== undefined);

  // A fresh scene (new view) invalidates any in-flight drag: the card, action, or
  // geometry may be gone. Nothing client-side is load-bearing across messages.
  useEffect(() => setDrag(null), [scene]);

  const toScene = useCallback((clientX: number, clientY: number): { x: number; y: number } => {
    const origin = rootRef.current?.getBoundingClientRect();
    return { x: clientX - (origin?.left ?? 0), y: clientY - (origin?.top ?? 0) };
  }, []);

  const resolveDrop = useCallback(
    (action: ValidAction, x: number, y: number): void => {
      const requirements = action.requirements ?? [];
      if (requirements.length === 0) {
        // Untargeted play: the legal drop area is the receiver's battlefield panel.
        const board = scene.bands.find((band) => band.isLocal)?.rect;
        if (board && contains(board, x, y)) onPlay?.(action);
        return;
      }
      // Targeted spell: the legal drops are the first slot's server candidates.
      const candidates = new Set(requirements[0]?.candidates ?? []);
      const hit = allCards.find(
        (card) => candidates.has(card.entityId) && contains(card.rect, x, y),
      );
      if (hit) onPlayOnTarget?.(action, hit.entityId);
    },
    [scene, allCards, onPlay, onPlayOnTarget],
  );

  /** Arm a drag on pointerdown; it goes live only past the travel threshold. */
  const armDrag = useCallback(
    (card: RenderedCard, action: ValidAction, e: ReactPointerEvent): void => {
      // A secondary/middle press never drags; an environment that reports no
      // button (some synthetic pointer events) counts as primary.
      if ((e.button ?? 0) !== 0) return;
      const start = { x: e.clientX, y: e.clientY };
      let live = false;
      const detach = (): void => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('keydown', onKey, true);
      };
      const onMove = (ev: PointerEvent): void => {
        if (!live && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < DRAG_THRESHOLD) {
          return;
        }
        live = true;
        const p = toScene(ev.clientX, ev.clientY);
        setDrag({ entityId: card.entityId, name: card.name, action, x: p.x, y: p.y });
      };
      // A completed (or cancelled) drag swallows the click the browser fires on
      // release; the flag self-clears next tick so it can never eat a later,
      // genuine click when the release landed on a non-interactive area.
      const swallowNextClick = (): void => {
        swallowClick.current = true;
        setTimeout(() => {
          swallowClick.current = false;
        }, 0);
      };
      const onUp = (ev: PointerEvent): void => {
        detach();
        if (!live) return; // an ordinary click — the hotspot's onClick selects
        setDrag(null);
        swallowNextClick();
        const p = toScene(ev.clientX, ev.clientY);
        resolveDrop(action, p.x, p.y);
      };
      // Esc cancels back to the origin slot (blueprint) — nothing fires, and the
      // shell's own Escape handling is suppressed for this press (the cancel is
      // the drag's, not the selection's).
      const onKey = (ev: KeyboardEvent): void => {
        if (ev.key !== 'Escape' || !live) return;
        ev.stopPropagation();
        detach();
        swallowNextClick();
        setDrag(null);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('keydown', onKey, true);
    },
    [toScene, resolveDrop],
  );

  /** The select handler, swallowing the click a completed drag releases. */
  const selectGuarded = useCallback(
    (id: EntityId): void => {
      if (swallowClick.current) {
        swallowClick.current = false;
        return;
      }
      onSelect(id);
    },
    [onSelect],
  );

  const dragBoard = drag && (drag.action.requirements?.length ?? 0) === 0;
  const dragCandidates = drag
    ? new Set(drag.action.requirements?.[0]?.candidates ?? [])
    : new Set<EntityId>();
  const localBoard = scene.bands.find((band) => band.isLocal)?.rect;

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
    <div ref={rootRef} style={overlay(scene.width, scene.height)}>
      {/*
       * Drag-to-play affordances: while a playable hand card is in flight the legal
       * drop area lights — a gold inset on the receiver's battlefield for an
       * untargeted play, orange rings on the server-listed candidates for a
       * targeted spell. All non-interactive; the drop resolves via scene rects.
       */}
      {drag && dragBoard && localBoard && (
        <div data-testid="drop-board" aria-hidden="true" style={dropBoardInset(localBoard)} />
      )}
      {drag &&
        !dragBoard &&
        allCards
          .filter((card) => dragCandidates.has(card.entityId))
          .map((card) => (
            <div
              key={`drop-${card.entityId}`}
              data-testid={`drop-target-${card.entityId}`}
              aria-hidden="true"
              style={dropTargetRing(card.rect)}
            />
          ))}
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
        // A playable hand card is also draggable (the pointer enhancement): its
        // pointerdown arms a drag that goes live past the travel threshold, while
        // a plain click still selects — drag is layered ON the universal path,
        // never replacing it (keyboard/AT interact exactly as before).
        const playAction = dragEnabled ? playActionOf(card) : undefined;
        const onPointerDown =
          playAction !== undefined
            ? (e: ReactPointerEvent): void => {
                gestures.onPointerDown(e);
                armDrag(card, playAction, e);
              }
            : gestures.onPointerDown;
        return (
          <button
            key={card.entityId}
            type="button"
            data-testid={`entity-${card.entityId}`}
            data-entity={card.entityId}
            data-actionable="true"
            data-draggable={playAction !== undefined || undefined}
            aria-pressed={selected}
            aria-label={`${card.name} — playable: ${actionHint}`}
            onClick={() => selectGuarded(card.entityId)}
            className={s.canvasControl}
            style={hotspot(card.rect, selected)}
            {...gestures}
            onPointerDown={onPointerDown}
          />
        );
      })}
      {/* The dragged card's ghost rides under the pointer; its origin slot stays
          open in the hand (the scene never re-lays-out mid-drag). */}
      {drag && (
        <div data-testid="drag-ghost" aria-hidden="true" style={dragGhostBox(drag.x, drag.y)}>
          {drag.name}
        </div>
      )}
    </div>
  );
}
