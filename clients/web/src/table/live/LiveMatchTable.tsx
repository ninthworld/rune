/**
 * Production composition root for the ADR 0030 2.5D match surface.
 *
 * The latest complete GameView feeds every layer: environment + staged DOM
 * plane + passive effects + the existing screen-space chrome. The scene reuses
 * the established server-authoritative interaction state machine; gestures
 * only route actions and candidates already present in the latest GameView.
 */
import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../../card/art/artStore';
import type { CardView, EntityId, PlayerId, ValidAction } from '../../protocol';
import { playerName } from '../../playerNames';
import { selectPendingPrompt, useGameStore } from '../../store';
import { publishPlane, publishScene, publishView } from '../../testHooks';
import { CardFace } from '../../card/dom';
import { ArtSettings } from '../ArtSettings';
import { PresentationSettings } from '../PresentationSettings';
import { CardInspect } from '../CardInspect';
import { GameMenu } from '../GameMenu';
import { GameOverOverlay } from '../GameOverOverlay';
import type { BrowsableZone } from '../PanelChrome';
import { RejectionToast } from '../RejectionToast';
import { ShortcutHelp } from '../ShortcutHelp';
import { ZoneBrowser } from '../ZoneBrowser';
import type { EffectDensity, EffectQuality } from '../effects';
import type { MotionPreference } from '../settings/presentationSettings';
import { usePresentationSettings } from '../settings/usePresentationSettings';
import { useReducedMotion } from '../hooks/useReducedMotion';
import { useTableInteractions } from '../hooks/useTableInteractions';
import { useTableKeyboard } from '../hooks/useTableKeyboard';
import { useViewport } from '../hooks/useViewport';
import { activeSlot as msActiveSlot, beginMultiSelect, toggle as msToggle } from '../multiSelect';
import { domCardArt, handDisplayData } from '../planeDisplayData';
import { declarationFor } from '../scene/action-helpers';
import type { Rect } from '../scene';
import { activeCandidates, activeRequirement, canRetract } from '../targeting';
import {
  buildShortcutBindings,
  demandsDecision,
  forcedDecision,
  resolveInspect,
} from '../tableView';
import { ControlCluster } from '../controls';
import { DecisionArea, deriveDecision } from '../decision';
import { ActivitySurface, StackStage } from '../stack';
import { HandFan } from './HandFan';
import { LivePlane } from './LivePlane';
import type { LivePlaneInteractionProps } from './LivePlaneControls';
import type { TargetingPresentationPath } from './gameViewPresentation';
import { isCompactShell, shellBands, shellStyleVars } from './shellLayout';
import { useSessionMoments } from './useSessionMoments';
import styles from './live-match.module.css';

/**
 * Presentation inputs. Left unset, they come from the device-local presentation
 * settings (issue #505) — the props are an explicit override seam for tests and
 * embedding.
 */
export interface LiveMatchTableProps {
  quality?: EffectQuality;
  density?: EffectDensity;
  motion?: MotionPreference;
}

interface OpenZone {
  playerId: PlayerId;
  zone: BrowsableZone;
}

/**
 * A live hand drag. The **card** rides here, not just its name (issue #569): the
 * proxy draws the real resolved face with the one card renderer, so what follows
 * the pointer is the card the player picked up rather than a text box standing
 * in for it. `action` is the server-issued action the drop will fire — the
 * client still sends nothing but an `action_id`.
 */
interface HandDrag {
  card: CardView;
  action: ValidAction;
  x: number;
  y: number;
}

/** Render a real personalized match on the 2.5D scene stack. */
export function LiveMatchTable(props: LiveMatchTableProps = {}) {
  const settings = usePresentationSettings();
  const quality = props.quality ?? settings.quality;
  const density = props.density ?? settings.density;
  const motion = props.motion ?? settings.motion;
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const setStops = useGameStore((state) => state.setStops);
  const disconnect = useGameStore((state) => state.disconnect);
  const leaveGame = useGameStore((state) => state.leaveGame);
  const rejectionNonce = useGameStore((state) => state.rejectionNonce);
  const sessionEpoch = useGameStore((state) => state.sessionEpoch);
  const artVersion = useSyncExternalStore(subscribeArt, getArtVersion);
  const viewport = useViewport();
  const reducedMotion = useReducedMotion(motion);
  const [highlightedId, setHighlightedId] = useState<EntityId | null>(null);
  const [focusedSeat, setFocusedSeat] = useState<PlayerId | null>(null);
  const [pendingSeatFocus, setPendingSeatFocus] = useState<PlayerId | null>(null);
  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  const [peekId, setPeekId] = useState<EntityId | null>(null);
  const [browsing, setBrowsing] = useState<OpenZone | null>(null);
  const [showHelp, setShowHelp] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showArtSettings, setShowArtSettings] = useState(false);
  const [previewTargetId, setPreviewTargetId] = useState<EntityId | PlayerId | null>(null);
  const [handDrag, setHandDrag] = useState<HandDrag | null>(null);
  // The §8 session moments the shell owns: the game-start assembly, the
  // reconnect acknowledgment, and the recede that hands off to the lobby.
  const { moment, notePresentationMode, leave } = useSessionMoments(reducedMotion, leaveGame);
  const swallowClick = useRef(false);
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
    setNumber,
    chooseOption,
    retractTarget,
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
    setHandDrag(null);
    setPreviewTargetId(null);
    // The setters are stable; the latest complete view is the sole reset trigger.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  // …but a decision the view forces (the pre-game mulligan) is re-opened straight
  // away, and re-opens if it is dismissed (issue #451). The reset above drops the
  // session on *every* frame — including the resync answering a rejection and the
  // fresh hand a mulligan deals — which left the player hunting for a dock button
  // after each click, and able to close the only thing the server was waiting on.
  // The session itself stays ephemeral: it is rebuilt from the view's own action.
  const forced = view ? forcedDecision(view) : null;
  useEffect(() => {
    if (forced === null || multiSelect !== null) return;
    setMultiSelect(beginMultiSelect(forced));
    // The setter is stable; re-opening is driven by the forced action and whether a
    // session is currently open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [forced, multiSelect]);

  useEffect(() => () => dragCleanup.current?.(), []);

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
    showSettings,
    showArtSettings,
    inspectedId,
    peekId,
    browsing,
    focusedTileId: focusedSeat,
    mainRef,
    focusGeometryRef,
    setSelectedId,
    setTargeting,
    setMultiSelect,
    setInspectedId,
    setPeekId,
    setBrowsing,
    setFocusedTileId: setFocusedSeat,
    setShowHelp,
    setShowSettings,
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

  // One composition switch, shared with the stylesheet's media query and the
  // geometry mirror (`shellLayout.SHELL.compactBreakpoint`); all three move together.
  const compact = isCompactShell(viewport);
  // The one geometry source: the scene's staging box, the hand's span, and the
  // cluster's column all come from here, and `shellLayout.test.ts` proves none
  // of them covers another (invariant I1).
  const bands = shellBands(viewport, {}, { stackPresent: view.stack.length > 0 });
  const prompt = selectPendingPrompt(view);
  const inspectId = inspectedId ?? peekId ?? selectedId;
  const inspectTarget = inspectId ? resolveInspect(view, inspectId) : null;
  const browserData = browsing
    ? {
        zone: browsing.zone,
        owner: playerName(view, browsing.playerId),
        cards:
          (browsing.zone === 'graveyard' ? view.graveyards : view.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;
  // ONE derivation of the open decision (#567): the surface's words, rows,
  // choices, and controls, and the candidates the plane lights, all from a single
  // pure read of the session. Three surfaces used to derive this separately and
  // draw the same question at once.
  const { surface: decision, staging: decisionStaging } = deriveDecision(view, {
    targeting,
    multiSelect,
    forced,
    deadline: prompt?.deadline,
    canRetract: targeting !== null && canRetract(targeting),
  });
  const selecting = decisionStaging.selecting;
  const passOffered =
    view.valid_actions.some((action) => action.type === 'pass_priority') && !selecting;
  const shortcuts = buildShortcutBindings(passOffered);
  const mode = selecting || demandsDecision(view) ? 'focus' : 'overview';

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
          // Slots already answered are PROVISIONAL (dashed, crawl stopped); the
          // slot under the cursor is PENDING (dashed, crawling). The numeral is
          // the slot's own 1-based place in the action's requirement order —
          // the §4.5 ordering channel, never screen order.
          ...targeting.picks.flatMap((chosenIds, slotIndex) =>
            chosenIds.map((to, choiceIndex) => ({
              id: `${targeting.action.id}:${slotIndex}:${choiceIndex}`,
              from: targetingSource,
              to,
              pending: false,
              numeral: slotIndex + 1,
            })),
          ),
          ...(previewTarget === null
            ? []
            : [
                {
                  id: `${targeting.action.id}:preview`,
                  from: targetingSource,
                  to: previewTarget,
                  pending: true,
                  numeral: targeting.picks.length + 1,
                },
              ]),
        ];

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
    card: CardView,
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
      setHandDrag({ card, action, x: move.clientX, y: move.clientY });
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
    candidates: decisionStaging.candidates,
    chosen: decisionStaging.chosen,
    playerCandidates: decisionStaging.playerCandidates,
    // Issue #457: the multiplayer "whom does this creature attack" question. Both
    // values come straight off the active `defend_` slot — the seats the server
    // listed and the attacker the slot is keyed by — so the control layer can make
    // the panels unmistakable without deriving anything about combat.
    assigningDefender: decisionStaging.assigningDefender,
    routedAttacker: decisionStaging.routedAttacker,
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

  // The shell's session-moment flags (visual-system §8, issue #509), all pure
  // CSS staging over an unchanged, fully interactive tree:
  // - `data-moment` — the entry assembly, the reconnect cue, or the exit recede.
  // - `data-orienting` — the reconnect half the phase pill wears (carried).
  // - `data-forced-decision` — the mulligan card moment: while the view forces
  //   the opening-hand decision the hand is *presented*, lifted forward so the
  //   cards being kept or bottomed read as the subject of the question.
  return (
    <main
      ref={mainRef}
      className={styles.shell}
      data-testid="live-match-table"
      data-mode={mode}
      data-composition={compact ? 'compact' : 'full'}
      data-moment={moment ?? undefined}
      data-orienting={moment === 'reconnect' || undefined}
      data-forced-decision={forced === null ? undefined : 'true'}
      // The viewport/safe-area contract (issue #528): every shell track size and
      // hand offset in `live-match.module.css` reads one of these properties, so
      // the geometry the layout tests reason about is the geometry that ships.
      style={shellStyleVars(viewport)}
      onFocusCapture={(event) => {
        focusedEntity.current =
          (event.target as HTMLElement).closest<HTMLElement>('[data-entity]')?.dataset.entity ??
          null;
      }}
    >
      {/* The battlefield spans the whole safe viewport (ADR 0032): the arena
          stays visible BEHIND the contextual controls rather than ending where
          they begin. What the plane may NOT do is stage an object under a
          control — that is what `bands.staging` bounds (shellLayout.ts, I1). */}
      <section className={styles.scene} aria-label="Battlefield">
        <LivePlane
          view={view}
          staging={{
            focusSeat: focusedSeat ?? undefined,
            selectedId: decisionStaging.routedAttacker ?? selectedId ?? highlightedId ?? undefined,
            candidates: decisionStaging.candidates,
          }}
          safeArea={bands.staging}
          quality={quality}
          density={density}
          reducedMotion={reducedMotion}
          artVersion={artVersion}
          sessionEpoch={sessionEpoch}
          targetingPaths={targetingPaths}
          onPlane={publishPlane}
          onMode={notePresentationMode}
          interaction={planeInteraction}
        />
      </section>

      {/* The receiver's curved fan (issue #533), still a shell region rather
          than a scene object (ADR 0032 §7). Its geometry, overlap rule, and
          paging live in `table/handFan.ts`, which every opponent's face-down
          fan reads too — one curve family, two tiers. */}
      <div className={styles.hand}>
        <HandFan
          view={view}
          bandWidth={bands.hand.w}
          selecting={selecting}
          multiSelect={multiSelect !== null}
          candidates={decisionStaging.candidates}
          chosen={decisionStaging.chosen}
          selectedId={selectedId}
          previewTargetId={previewTargetId}
          draggingId={handDrag?.card.id ?? null}
          shouldSwallowClick={() => {
            if (!swallowClick.current) return false;
            swallowClick.current = false;
            return true;
          }}
          onPick={multiSelect ? toggleCandidate : pickTarget}
          onActivate={activateEntity}
          onInspect={setInspectedId}
          onPreviewTarget={setPreviewTargetId}
          onCardPointerDown={(card, event) => {
            const play = playActionFor(card.id);
            if (play) armHandDrag(card, play, event);
          }}
        />
      </div>

      {/* The one action home — the ADR 0023 commitment kept, its location moved
          from beside the hand to the lower-right cluster (ADR 0032 commitment
          2, which is what settles control-language C5). */}
      <div className={styles.cluster} data-focus-region="actions">
        <ControlCluster
          view={view}
          selectedId={selectedId ?? undefined}
          session={targeting ? 'targeting' : multiSelect ? 'multiSelect' : undefined}
          onChoose={fire}
          onRespond={() => {
            // §4.3/D6: RESPOND sends nothing. It moves focus into the fan so the
            // player can pick something to cast instead of passing.
            mainRef.current
              ?.querySelector<HTMLElement>('[data-focus-region="hand"] button:not(:disabled)')
              ?.focus();
          }}
          onOpenMenu={() => setMenuOpen((current) => !current)}
          menuOpen={menuOpen}
          menuControls="game-menu"
          onSetStops={setStops}
          onUndo={targeting !== null && canRetract(targeting) ? retractTarget : undefined}
          shakeNonce={rejectionNonce}
        />
        {/* D5/D18: the menu is where settings, shortcuts, and CONCEDE live —
            concede is deliberately never adjacent to the ordinary primary, and
            it carries a two-step confirmation. The cluster's circular icon is
            its handle, which is how §15's C7 duplication is resolved. */}
        <GameMenu
          open={menuOpen}
          onOpenChange={setMenuOpen}
          concede={view.valid_actions.find((action) => action.type === 'concede')}
          onChoose={choose}
          onShowShortcuts={() => setShowHelp(true)}
          onShowSettings={() => setShowSettings(true)}
          onShowArtSettings={() => setShowArtSettings(true)}
        />
      </div>

      {/* Both absent when they have nothing to say — an empty stack and a quiet
          log consume no battlefield width at all (#534). */}
      <StackStage
        view={view}
        compact={compact}
        targeting={
          targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
        }
        onInspect={setInspectedId}
      />
      <ActivitySurface view={view} onHighlight={highlight} highlightedId={highlightedId} />

      {/* THE decision surface (#567): the question, its rows, its named choices,
          and its controls, at the top of the same lower-right action area the
          cluster occupies. Absent unless a decision is open — there is no
          permanent dock to park it in any more, and no second surface repeating
          the question. It is a SIBLING of the shell's regions on purpose: a
          region carrying a z-index creates a stacking context, so a decision
          rendered inside `.cluster` could be covered by the chrome the
          `decision` rung exists to keep off it (#528). */}
      {decision && (
        <DecisionArea
          surface={decision}
          onConfirm={confirmMultiSelect}
          onAdvance={advanceSlot}
          onUndo={retractTarget}
          onCancel={multiSelect ? cancelMultiSelect : cancelTargeting}
          onToggleRow={toggleCandidate}
          onMoveRow={moveOrder}
          onChooseOption={chooseOption}
          onNumber={setNumber}
        />
      )}

      {/* The drag proxy (`control-language.md` §6.2 stage 2): the REAL card,
          drawn by the one card renderer at its hand tier and held at the drag
          elevation, following the pointer. It is `aria-hidden` and pointer-
          transparent, so it is invisible both to assistive technology — the
          origin button keeps the accessible name and every keyboard path — and
          to hit testing, which is what lets `elementFromPoint` find the drop
          receiver underneath it. */}
      {handDrag && (
        <div
          className={styles.dragProxy}
          data-testid="drag-ghost"
          data-entity-drag={handDrag.card.id}
          aria-hidden="true"
          style={{ left: handDrag.x, top: handDrag.y }}
        >
          <CardFace
            data={handDisplayData(view, handDrag.card)}
            tier="hand"
            elevation="held"
            art={domCardArt(handDrag.card)}
            rulesText={handDrag.card.rules_text}
          />
        </div>
      )}

      {browserData && (
        <ZoneBrowser
          zone={browserData.zone}
          owner={browserData.owner}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={() => setBrowsing(null)}
        />
      )}
      {inspectTarget && (
        <CardInspect
          target={inspectTarget}
          transient={inspectedId === null}
          // A pinned inspect opened while a decision is live parks clear of the
          // action column and drops its veil, so neither the decision nor any
          // candidate it lit is covered (`control-language.md` §10).
          deferring={decision !== null}
          onOpenArtSettings={() => setShowArtSettings(true)}
          onClose={() => {
            setInspectedId(null);
            setPeekId(null);
          }}
        />
      )}
      {showHelp && <ShortcutHelp bindings={shortcuts} onClose={() => setShowHelp(false)} />}
      {showSettings && <PresentationSettings onClose={() => setShowSettings(false)} />}
      {showArtSettings && (
        <ArtSettings
          // A card from this game, so the preview is the player's own board
          // under the chosen art mode rather than a stock illustration.
          previewCard={view.my_hand[0] ?? view.battlefield[0]?.card}
          onClose={() => setShowArtSettings(false)}
        />
      )}
      {view.result && (
        <GameOverOverlay
          result={view.result}
          you={view.you}
          names={view.player_names}
          reducedMotion={reducedMotion}
          onLeave={leave}
        />
      )}
      <RejectionToast nonce={rejectionNonce} />
    </main>
  );
}
