/**
 * Production composition root for the ADR 0030 2.5D match surface.
 *
 * The latest complete GameView feeds every layer: environment + staged DOM
 * plane + passive effects + the existing screen-space chrome. The scene reuses
 * the established server-authoritative interaction state machine; gestures
 * only route actions and candidates already present in the latest GameView.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../../card/art/artStore';
import { CardFace } from '../../card/dom';
import type { EntityId, PlayerId, ValidAction } from '../../protocol';
import { playerName } from '../../playerNames';
import { selectPendingPrompt, useGameStore } from '../../store';
import { publishPlane, publishScene, publishView } from '../../testHooks';
import { ActionDock } from '../ActionDock';
import { ArtSettings } from '../ArtSettings';
import { CardInspect } from '../CardInspect';
import { DecisionSheet } from '../DecisionSheet';
import { GameOverOverlay } from '../GameOverOverlay';
import { MePanel } from '../MePanel';
import type { BrowsableZone } from '../PanelChrome';
import { PromptStrip, type MultiSelectBanner, type TargetingBanner } from '../PromptStrip';
import { Rail } from '../Rail';
import { RejectionToast } from '../RejectionToast';
import { ShortcutHelp } from '../ShortcutHelp';
import { TopBar, type RailSheet } from '../TopBar';
import { ZoneBrowser } from '../ZoneBrowser';
import type { EffectDensity, EffectQuality } from '../effects';
import { useReducedMotion } from '../hooks/useReducedMotion';
import { useTableInteractions } from '../hooks/useTableInteractions';
import { useTableKeyboard } from '../hooks/useTableKeyboard';
import { useViewport } from '../hooks/useViewport';
import {
  activeAttacker as msActiveAttacker,
  activeCandidates as msActiveCandidates,
  activeChosen as msActiveChosen,
  activeSlot as msActiveSlot,
  allSlotsSatisfied,
  beginMultiSelect,
  hasOptions,
  isLastSlot,
  toggle as msToggle,
} from '../multiSelect';
import { domCardArt, handDisplayData } from '../planeDisplayData';
import { declarationFor } from '../scene/action-helpers';
import type { Rect } from '../scene';
import { activeCandidates, activeRequirement } from '../targeting';
import {
  buildShortcutBindings,
  cardNameOf,
  demandsDecision,
  isOnCanvas,
  resolveInspect,
} from '../tableView';
import { LivePlane } from './LivePlane';
import type { LivePlaneInteractionProps } from './LivePlaneControls';
import type { TargetingPresentationPath } from './gameViewPresentation';
import { ORIENTATION_PULSE_MS, type PresentationMode } from './presentationMode';
import styles from './live-match.module.css';

/** Tunable presentation inputs. Preferences can replace these defaults later. */
export interface LiveMatchTableProps {
  quality?: EffectQuality;
  density?: EffectDensity;
}

interface OpenZone {
  playerId: PlayerId;
  zone: BrowsableZone;
}

interface HandDrag {
  cardId: EntityId;
  name: string;
  action: ValidAction;
  x: number;
  y: number;
}

/** Render a real personalized match on the 2.5D scene stack. */
export function LiveMatchTable({ quality = 'standard', density = 'reduced' }: LiveMatchTableProps) {
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const setStops = useGameStore((state) => state.setStops);
  const disconnect = useGameStore((state) => state.disconnect);
  const rejectionNonce = useGameStore((state) => state.rejectionNonce);
  const sessionEpoch = useGameStore((state) => state.sessionEpoch);
  const artVersion = useSyncExternalStore(subscribeArt, getArtVersion);
  const viewport = useViewport();
  const reducedMotion = useReducedMotion();
  const [highlightedId, setHighlightedId] = useState<EntityId | null>(null);
  const [focusedSeat, setFocusedSeat] = useState<PlayerId | null>(null);
  const [pendingSeatFocus, setPendingSeatFocus] = useState<PlayerId | null>(null);
  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  const [peekId, setPeekId] = useState<EntityId | null>(null);
  const [browsing, setBrowsing] = useState<OpenZone | null>(null);
  const [railSheet, setRailSheet] = useState<RailSheet | null>(null);
  const [showHelp, setShowHelp] = useState(false);
  const [showArtSettings, setShowArtSettings] = useState(false);
  const [previewTargetId, setPreviewTargetId] = useState<EntityId | PlayerId | null>(null);
  const [handDrag, setHandDrag] = useState<HandDrag | null>(null);
  const [orienting, setOrienting] = useState(false);
  const swallowClick = useRef(false);
  const orientTimer = useRef<number | null>(null);
  const dragCleanup = useRef<(() => void) | null>(null);
  const focusedEntity = useRef<EntityId | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  const focusGeometryRef = useRef<Map<string, Rect>>(new Map());
  const {
    selectedId,
    setSelectedId,
    targeting,
    setTargeting,
    multiSelect,
    setMultiSelect,
    fire,
    fireOnTarget,
    pickTarget,
    toggleCandidate,
    pickDefender,
    advanceSlot,
    confirmMultiSelect,
    moveOrder,
    chooseOption,
    cancelTargeting,
    cancelMultiSelect,
  } = useTableInteractions(choose);

  useEffect(() => {
    if (view) noteCards(collectArtCards(view));
  }, [view]);

  // A new complete view supersedes every ephemeral presentation choice.
  useEffect(() => {
    dragCleanup.current?.();
    dragCleanup.current = null;
    setHighlightedId(null);
    setFocusedSeat(null);
    setPendingSeatFocus(null);
    setSelectedId(null);
    setTargeting(null);
    setMultiSelect(null);
    setInspectedId(null);
    setPeekId(null);
    setBrowsing(null);
    setRailSheet(null);
    setHandDrag(null);
    setPreviewTargetId(null);
    // The setters are stable; the latest complete view is the sole reset trigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  useEffect(
    () => () => {
      dragCleanup.current?.();
      if (orientTimer.current !== null) window.clearTimeout(orientTimer.current);
    },
    [],
  );

  // The "you are here" phase-pill pulse after a reconnect/resync rebuild
  // (visual-system §8). The active-crest half plays on the effects layer; this
  // flags the shell so the phase pill pulses too. Reduced motion gets neither.
  const handlePresentationMode = useCallback(
    (mode: PresentationMode): void => {
      if (mode !== 'rebuild' || reducedMotion) return;
      setOrienting(true);
      if (orientTimer.current !== null) window.clearTimeout(orientTimer.current);
      orientTimer.current = window.setTimeout(() => {
        orientTimer.current = null;
        setOrienting(false);
      }, ORIENTATION_PULSE_MS);
    },
    [reducedMotion],
  );

  useEffect(() => {
    const id = focusedEntity.current;
    if (id === null) return;
    const timeout = window.setTimeout(() => {
      const controls = Array.from(
        mainRef.current?.querySelectorAll<HTMLElement>(
          '[data-testid="live-plane-controls"] [data-entity]',
        ) ?? [],
      );
      const exact = controls.find((control) => control.dataset.entity === id);
      const fallback =
        mainRef.current?.querySelector<HTMLElement>(
          '[data-testid="live-plane-controls"] button:not(:disabled)',
        ) ??
        mainRef.current?.querySelector<HTMLElement>(
          '[data-focus-region="actions"] button:not(:disabled)',
        );
      (exact ?? fallback)?.focus();
    }, 0);
    return () => window.clearTimeout(timeout);
  }, [view]);

  useEffect(() => {
    publishScene(null);
    return () => {
      publishPlane(null);
      publishView(null);
    };
  }, []);

  useEffect(() => {
    publishView(view);
  }, [view]);

  useTableKeyboard({
    view,
    choose,
    targeting,
    multiSelect,
    showHelp,
    showArtSettings,
    inspectedId,
    peekId,
    browsing,
    railSheet,
    focusedTileId: focusedSeat,
    mainRef,
    focusGeometryRef,
    setSelectedId,
    setTargeting,
    setMultiSelect,
    setInspectedId,
    setPeekId,
    setBrowsing,
    setRailSheet,
    setFocusedTileId: setFocusedSeat,
    setShowHelp,
    setShowArtSettings,
  });

  useEffect(() => {
    setPreviewTargetId(null);
  }, [multiSelect?.action.id, targeting?.action.id]);

  // A compact tile can be replaced by an expanded region after activation.
  // Restore focus into that region after its destination controls mount; this
  // presentation state is cleared with every fresh GameView.
  useEffect(() => {
    if (pendingSeatFocus === null) return;
    const control = mainRef.current?.querySelector<HTMLElement>(
      `[data-focus-region="plane-seat-${pendingSeatFocus}"] button`,
    );
    if (!control) return;
    control.focus();
    setPendingSeatFocus(null);
  }, [focusedSeat, pendingSeatFocus]);

  if (!view) {
    return (
      <main className={styles.waiting} data-testid="table-waiting">
        <span>Connected — waiting for first game state…</span>
        <button type="button" onClick={disconnect} data-testid="disconnect-button">
          Disconnect
        </button>
      </main>
    );
  }

  const compact = viewport.width < 900;
  const prompt = selectPendingPrompt(view);
  const localId = view.you || undefined;
  const inspectId = inspectedId ?? peekId ?? selectedId;
  const inspectTarget = inspectId ? resolveInspect(view, inspectId) : null;
  const browserData = browsing
    ? {
        title: `${playerName(view, browsing.playerId)} — ${
          browsing.zone === 'graveyard' ? 'Graveyard' : 'Exile'
        }`,
        cards:
          (browsing.zone === 'graveyard' ? view.graveyards : view.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;
  const selecting = targeting !== null || multiSelect !== null;
  const passOffered =
    view.valid_actions.some((action) => action.type === 'pass_priority') && !selecting;
  const shortcuts = buildShortcutBindings(passOffered);
  const mode = selecting || demandsDecision(view) ? 'focus' : 'overview';

  const msSlot = multiSelect ? msActiveSlot(multiSelect) : null;
  const defenderSlot = msSlot?.kind === 'defender';
  const sheetMode =
    !!msSlot &&
    !defenderSlot &&
    (msSlot.kind === 'order' || !msSlot.candidates.some((id) => isOnCanvas(view, id)));
  const pickingCandidates = multiSelect
    ? sheetMode || defenderSlot
      ? []
      : msActiveCandidates(multiSelect)
    : targeting
      ? activeCandidates(targeting)
      : [];
  const playerCandidates = multiSelect
    ? defenderSlot
      ? msActiveCandidates(multiSelect)
      : []
    : targeting
      ? activeCandidates(targeting).filter(
          (id) => id === view.you || view.opponents.some((opponent) => opponent.player_id === id),
        )
      : [];
  const chosen = multiSelect ? msActiveChosen(multiSelect) : [];
  const routingAttacker = multiSelect && defenderSlot ? msActiveAttacker(multiSelect) : null;

  const selectedActions =
    selectedId === null || selecting
      ? []
      : view.valid_actions.filter((action) => action.subject?.includes(selectedId));
  const selectedName = selectedId === null ? undefined : cardNameOf(view, selectedId);

  const activateEntity = (id: EntityId): void => {
    const actions = view.valid_actions.filter((action) => action.subject?.includes(id));
    const declaration = declarationFor(id, view.valid_actions);
    if (actions.length === 0 && declaration) {
      const session = beginMultiSelect(declaration);
      const slot = msActiveSlot(session);
      setSelectedId(null);
      setTargeting(null);
      setMultiSelect(
        slot && slot.kind !== 'order' && slot.candidates.includes(id)
          ? msToggle(session, id)
          : session,
      );
      return;
    }
    // Issue #463 deliberately supersedes ADR 0025's one-click mana shortcut:
    // every ability, including a flagged mana ability, first selects. A second
    // activation fires a sole action; the dock is the single-gesture fallback.
    if (selectedId !== id) {
      setSelectedId(id);
      return;
    }
    if (actions.length === 1) fire(actions[0]!);
  };

  const activeReq = targeting ? activeRequirement(targeting) : null;
  const previewTarget =
    activeReq?.candidates?.includes(previewTargetId ?? '') === true
      ? previewTargetId
      : activeReq?.candidates?.length === 1
        ? activeReq.candidates[0]
        : null;
  const targetingSource =
    targeting?.action.subject?.[0] === undefined
      ? null
      : view.my_hand.some((card) => card.id === targeting.action.subject![0])
        ? `hand:${view.you}`
        : targeting.action.subject[0];
  const targetingPaths: TargetingPresentationPath[] =
    targeting === null || targetingSource === null
      ? []
      : [
          ...targeting.picks.flatMap((chosenIds, slotIndex) =>
            chosenIds.map((to, choiceIndex) => ({
              id: `${targeting.action.id}:${slotIndex}:${choiceIndex}`,
              from: targetingSource,
              to,
            })),
          ),
          ...(previewTarget === null
            ? []
            : [
                {
                  id: `${targeting.action.id}:preview`,
                  from: targetingSource,
                  to: previewTarget,
                },
              ]),
        ];
  const targetingBanner: TargetingBanner | null =
    targeting && activeReq
      ? {
          label: targeting.action.label,
          prompt: activeReq.prompt,
          step: targeting.picks.length + 1,
          total: targeting.action.requirements?.length ?? 0,
        }
      : null;
  const multiSelectBanner: MultiSelectBanner | null =
    multiSelect && (msSlot || hasOptions(multiSelect))
      ? {
          label: multiSelect.action.label,
          prompt: msSlot?.prompt ?? multiSelect.options[0]?.prompt ?? '',
          step: multiSelect.active + 1,
          total: multiSelect.slots.length,
          chosen: msActiveChosen(multiSelect).length,
          required: msSlot?.kind === 'count' ? msSlot.count : undefined,
          slotKind: msSlot?.kind,
        }
      : null;
  const multiSelectControls = multiSelect
    ? {
        canAdvance: multiSelect.slots.length > 1 && !isLastSlot(multiSelect),
        onAdvance: advanceSlot,
        confirm: hasOptions(multiSelect)
          ? undefined
          : {
              label: 'Confirm',
              enabled: allSlotsSatisfied(multiSelect),
              onConfirm: confirmMultiSelect,
            },
        onCancel: cancelMultiSelect,
      }
    : undefined;

  const highlight = (id: EntityId): void =>
    setHighlightedId((current) => (current === id ? null : id));
  const openZone = (playerId: PlayerId, zone: BrowsableZone): void =>
    setBrowsing({ playerId, zone });

  const playActionFor = (id: EntityId): ValidAction | undefined =>
    view.valid_actions.find(
      (action) =>
        action.subject?.includes(id) &&
        (action.type === 'play_land' || action.type === 'cast_spell'),
    );

  const armHandDrag = (
    cardId: EntityId,
    name: string,
    action: ValidAction,
    event: ReactPointerEvent<HTMLButtonElement>,
  ): void => {
    if ((event.button ?? 0) !== 0) return;
    dragCleanup.current?.();
    const start = { x: event.clientX, y: event.clientY };
    let live = false;
    const detach = (): void => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('keydown', onKey, true);
      if (dragCleanup.current === detach) dragCleanup.current = null;
    };
    const onMove = (move: PointerEvent): void => {
      if (!live && Math.hypot(move.clientX - start.x, move.clientY - start.y) < 6) return;
      live = true;
      setHandDrag({ cardId, name, action, x: move.clientX, y: move.clientY });
    };
    const endDrag = (): void => {
      setHandDrag(null);
      swallowClick.current = true;
      window.setTimeout(() => {
        swallowClick.current = false;
      }, 0);
    };
    const onUp = (up: PointerEvent): void => {
      detach();
      if (!live) return;
      endDrag();
      const hit = document.elementFromPoint(up.clientX, up.clientY) as HTMLElement | null;
      const targetId = hit?.closest<HTMLElement>('[data-entity]')?.dataset.entity;
      const firstCandidates = action.requirements?.[0]?.candidates ?? [];
      if (firstCandidates.length > 0) {
        if (targetId && firstCandidates.includes(targetId)) fireOnTarget(action, targetId);
      } else if (hit?.closest('[data-drop-receiver="true"]')) {
        fire(action);
      }
    };
    const onKey = (key: KeyboardEvent): void => {
      if (key.key !== 'Escape' || !live) return;
      key.stopPropagation();
      detach();
      endDrag();
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('keydown', onKey, true);
    dragCleanup.current = detach;
  };

  const notePlaneGeometry = (
    geometry: Map<string, Rect>,
    planeSize: { width: number; height: number },
  ): void => {
    const host = mainRef.current?.querySelector<HTMLElement>('[data-testid="live-2-5d-plane"]');
    const hostRect = host?.getBoundingClientRect();
    const scaleX = hostRect && hostRect.width > 0 ? hostRect.width / planeSize.width : 1;
    const scaleY = hostRect && hostRect.height > 0 ? hostRect.height / planeSize.height : 1;
    const offsetX = hostRect?.x ?? 0;
    const offsetY = hostRect?.y ?? 0;
    const next = new Map<string, Rect>();
    for (const [key, rect] of geometry) {
      next.set(key, {
        x: offsetX + rect.x * scaleX,
        y: offsetY + rect.y * scaleY,
        w: rect.w * scaleX,
        h: rect.h * scaleY,
      });
    }
    const noteDomRegion = (id: string): void => {
      const el = mainRef.current?.querySelector<HTMLElement>(`[data-focus-region="${id}"]`);
      const rect = el?.getBoundingClientRect();
      if (rect && rect.width > 0 && rect.height > 0) {
        next.set(id, { x: rect.x, y: rect.y, w: rect.width, h: rect.height });
      }
    };
    noteDomRegion('top');
    noteDomRegion('hand');
    noteDomRegion('actions');
    focusGeometryRef.current = next;
  };

  const planeInteraction: LivePlaneInteractionProps = {
    selectedId,
    picking: selecting,
    multiSelect: multiSelect !== null,
    candidates: pickingCandidates,
    chosen,
    playerCandidates,
    dropBoard: handDrag !== null && (handDrag.action.requirements?.length ?? 0) === 0,
    dropCandidates: handDrag?.action.requirements?.[0]?.candidates,
    onActivateEntity: activateEntity,
    onPickEntity: multiSelect ? toggleCandidate : pickTarget,
    onPickPlayer: multiSelect ? pickDefender : pickTarget,
    onPreviewTarget: setPreviewTargetId,
    onInspect: setInspectedId,
    onOpenZone: openZone,
    onFocusSeat: (seat) => {
      setPendingSeatFocus(seat);
      setFocusedSeat(seat);
    },
    onFocusGeometry: notePlaneGeometry,
  };

  const rail = (
    <Rail
      view={view}
      targeting={
        targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
      }
      onInspect={setInspectedId}
      onHighlight={highlight}
      highlightedId={highlightedId}
    />
  );

  return (
    <main
      ref={mainRef}
      className={styles.shell}
      data-testid="live-match-table"
      data-mode={mode}
      data-composition={compact ? 'compact' : 'full'}
      data-orienting={orienting || undefined}
      onFocusCapture={(event) => {
        focusedEntity.current =
          (event.target as HTMLElement).closest<HTMLElement>('[data-entity]')?.dataset.entity ??
          null;
      }}
    >
      <header className={styles.top} data-focus-region="top">
        <TopBar
          view={view}
          mode={mode}
          localId={localId}
          compact={compact}
          onSetStops={setStops}
          onOpenSheet={compact ? setRailSheet : undefined}
          concede={view.valid_actions.find((action) => action.type === 'concede')}
          onChoose={choose}
          onShowShortcuts={() => setShowHelp(true)}
          onShowArtSettings={() => setShowArtSettings(true)}
        />
      </header>

      <section className={styles.scene} aria-label="Battlefield">
        <LivePlane
          view={view}
          staging={{
            focusSeat: focusedSeat ?? undefined,
            selectedId: routingAttacker ?? selectedId ?? highlightedId ?? undefined,
            candidates: pickingCandidates,
          }}
          quality={quality}
          density={density}
          reducedMotion={reducedMotion}
          artVersion={artVersion}
          sessionEpoch={sessionEpoch}
          targetingPaths={targetingPaths}
          onPlane={publishPlane}
          onMode={handlePresentationMode}
          interaction={planeInteraction}
        />
      </section>

      {!compact && <aside className={styles.rail}>{rail}</aside>}

      <section className={styles.bottom} aria-label="Player controls">
        <div className={styles.identity}>
          <MePanel
            view={view}
            localId={localId}
            condensed
            onOpenZone={openZone}
            highlightedId={highlightedId}
            targeting={
              playerCandidates.length > 0
                ? {
                    candidates: playerCandidates,
                    onPick: multiSelect ? pickDefender : pickTarget,
                  }
                : undefined
            }
          />
        </div>
        <div className={styles.hand} data-testid="live-hand" data-focus-region="hand">
          {view.my_hand.map((card, index) => (
            <button
              key={card.id}
              type="button"
              className={styles.handCard}
              data-testid={`live-hand-card-${card.id}`}
              data-entity={card.id}
              data-actionable={
                (!selecting &&
                  view.valid_actions.some((action) => action.subject?.includes(card.id))) ||
                undefined
              }
              aria-label={
                pickingCandidates.includes(card.id)
                  ? `${multiSelect ? 'Toggle' : 'Target'} ${card.name}`
                  : view.valid_actions.some((action) => action.subject?.includes(card.id))
                    ? `${card.name} — playable`
                    : `Inspect ${card.name}`
              }
              aria-pressed={
                pickingCandidates.includes(card.id) && multiSelect
                  ? chosen.includes(card.id)
                  : selectedId === card.id
              }
              onClick={() => {
                if (swallowClick.current) {
                  swallowClick.current = false;
                  return;
                }
                if (pickingCandidates.includes(card.id)) {
                  (multiSelect ? toggleCandidate : pickTarget)(card.id);
                } else if (
                  !selecting &&
                  view.valid_actions.some((action) => action.subject?.includes(card.id))
                ) {
                  activateEntity(card.id);
                } else {
                  setInspectedId(card.id);
                }
              }}
              onPointerEnter={() => {
                if (pickingCandidates.includes(card.id)) setPreviewTargetId(card.id);
              }}
              onPointerLeave={() => {
                if (previewTargetId === card.id) setPreviewTargetId(null);
              }}
              onFocus={() => {
                if (pickingCandidates.includes(card.id)) setPreviewTargetId(card.id);
              }}
              onBlur={() => {
                if (previewTargetId === card.id) setPreviewTargetId(null);
              }}
              onPointerDown={(event) => {
                if (selecting) return;
                const play = playActionFor(card.id);
                if (play) armHandDrag(card.id, card.name, play, event);
              }}
              onContextMenu={(event) => {
                event.preventDefault();
                setInspectedId(card.id);
              }}
              style={
                {
                  '--hand-left':
                    view.my_hand.length <= 1
                      ? '50%'
                      : `${10 + (80 * index) / (view.my_hand.length - 1)}%`,
                  '--hand-angle': `${Math.max(
                    -8,
                    Math.min(8, (index - (view.my_hand.length - 1) / 2) * 2),
                  )}deg`,
                } as CSSProperties
              }
            >
              <CardFace data={handDisplayData(view, card)} tier="hand" art={domCardArt(card)} />
            </button>
          ))}
        </div>
        <div className={styles.decisions} data-focus-region="actions">
          <PromptStrip
            view={view}
            prompt={prompt}
            targeting={targetingBanner}
            multiSelect={multiSelectBanner}
          />
          <ActionDock
            globalActions={
              selecting
                ? []
                : (prompt?.globalActions ?? []).filter((action) => action.type !== 'concede')
            }
            selectedActions={selectedActions}
            selectedName={selectedName}
            onChoose={fire}
            onClearSelection={selectedId !== null ? () => setSelectedId(null) : undefined}
            onCancelTargeting={targeting ? cancelTargeting : undefined}
            multiSelect={multiSelectControls}
            waiting={prompt === null}
            deadline={selecting ? undefined : prompt?.deadline}
          />
        </div>
      </section>

      <DecisionSheet
        view={view}
        multiSelect={multiSelect}
        sheetMode={sheetMode}
        msSlot={msSlot}
        onToggle={toggleCandidate}
        onMove={moveOrder}
        onChooseOption={chooseOption}
      />
      {handDrag && (
        <div
          className={styles.dragGhost}
          data-testid="drag-ghost"
          aria-hidden="true"
          style={{ left: handDrag.x, top: handDrag.y }}
        >
          {handDrag.name}
        </div>
      )}

      {railSheet && (
        <div className={styles.sheetBackdrop} data-testid={`rail-sheet-${railSheet}`}>
          <div className={styles.sheet}>
            <button
              type="button"
              className={styles.close}
              aria-label="Close"
              data-testid="rail-sheet-close"
              onClick={() => setRailSheet(null)}
            >
              ×
            </button>
            {rail}
          </div>
        </div>
      )}
      {browserData && (
        <ZoneBrowser
          title={browserData.title}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={() => setBrowsing(null)}
        />
      )}
      {inspectTarget && (
        <CardInspect
          target={inspectTarget}
          transient={inspectedId === null}
          onClose={() => {
            setInspectedId(null);
            setPeekId(null);
          }}
        />
      )}
      {showHelp && <ShortcutHelp bindings={shortcuts} onClose={() => setShowHelp(false)} />}
      {showArtSettings && <ArtSettings onClose={() => setShowArtSettings(false)} />}
      {view.result && (
        <GameOverOverlay result={view.result} you={view.you} names={view.player_names} />
      )}
      <RejectionToast nonce={rejectionNonce} />
    </main>
  );
}
