/**
 * The playable table: the single React tree that reconstructs the whole UI from
 * the store's latest {@link GameView} (plus its derived pending prompt), laid out
 * in the **fixed shell** (ADR 0023; `docs/design/ui-blueprint.md`).
 *
 * - The shell is carved by `layout()`: top bar, per-player panels, right rail
 *   (stack + activity), and a bottom shell owning the receiver's identity panel,
 *   the prompt strip + hand, and the single action dock. Nothing floats over
 *   anything, so nothing can overlap or clip by construction.
 * - Pixi draws the cards inside the carved panel areas (ADR 0003); DOM layers
 *   render the panel chrome, prompts, and controls.
 * - Interactivity is driven entirely by `valid_actions[]`; choosing an action
 *   sends `ChooseAction` through the store, and the UI then rebuilds purely from
 *   the resulting GameView. The only client-side state is ephemeral selection
 *   (nothing load-bearing across messages — the reconnect/replay invariant).
 * - **One action home** (ADR 0023 commitment 2): selecting a card routes its
 *   offered actions to the action dock; the prompt strip states the pending
 *   question in words. Zone browsers, option pickers, and decision sheets are the
 *   only layer permitted to cover the shell — always viewport-clamped, always
 *   dismissible.
 *
 * Targeting mode (ADR 0009 §Client): choosing an action that carries
 * `requirements` enters a data-driven targeting flow. The client walks the
 * requirement slots as a prompt queue, offering only the server-listed candidates
 * (highlighted; everything else dimmed and inert), then submits the whole answer
 * atomically with the action's content-binding token — one `ChooseAction`, never
 * a multi-message handshake. The in-progress selection is ephemeral and discarded
 * on the next view, so the UI stays reconstructable from one GameView + prompt.
 *
 * This module is the composition root only. The ephemeral selection state machine
 * and action wiring live in `hooks/useTableInteractions`; the two global keyboard
 * listeners in `hooks/useTableKeyboard`; the viewport/reduced-motion trackers in
 * `hooks/`; the pure view derivations in `tableView`; and the game-over screen in
 * `GameOverTable` — all preserving the exact DOM/behavior this file wires together.
 */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import type { EntityId, PlayerId } from '../protocol';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../card/art/artStore';
import { ArtSettings } from './ArtSettings';
import { selectPendingPrompt, useGameStore } from '../store';
import { playerName } from '../playerNames';
import { publishScene, publishView } from '../testHooks';
import { ActionDock } from './ActionDock';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { PanelChrome, type BrowsableZone, type TileFocus } from './PanelChrome';
import { CardInspect } from './CardInspect';
import { DecisionSheet } from './DecisionSheet';
import { EntityOverlay } from './EntityOverlay';
import { GameOverTable } from './GameOverTable';
import { MePanel } from './MePanel';
import { PromptStrip, type MultiSelectBanner, type TargetingBanner } from './PromptStrip';
import { RejectionToast } from './RejectionToast';
import { ShortcutHelp } from './ShortcutHelp';
import { Rail } from './Rail';
import { TopBar, type RailSheet } from './TopBar';
import { ZoneBrowser } from './ZoneBrowser';
import { buildTableScene, orderedOpponentIds, type Rect, type TargetingScene } from './scene';
import { activeCandidates, activeRequirement } from './targeting';
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
} from './multiSelect';
import { layout, type RegionId } from './layout';
import { identityAccent } from './identityAccents';
import { useViewport } from './hooks/useViewport';
import { useReducedMotion } from './hooks/useReducedMotion';
import { useTableInteractions } from './hooks/useTableInteractions';
import { useTableKeyboard } from './hooks/useTableKeyboard';
import {
  buildShortcutBindings,
  demandsDecision,
  findCard,
  isOnCanvas,
  opponentsWithBoardCandidates,
  resolveInspect,
} from './tableView';
import { regionBox, sceneBox, shellBox } from './styles';
import s from './chrome.module.css';

/** The table's presentation mode (issue #267). */
export type TableMode = 'overview' | 'focus';

export function Table() {
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const setStops = useGameStore((state) => state.setStops);
  const disconnect = useGameStore((state) => state.disconnect);
  // The rejected-action trigger (issue #265): a counter the store bumps whenever the
  // server flags a view as answering a rejected action. Feeds the transient toast below;
  // purely ephemeral presentation, nothing the table reconstructs from.
  const rejectionNonce = useGameStore((state) => state.rejectionNonce);

  // The ephemeral selection state machine and action-submission wiring (ADR 0025):
  // the in-progress selection, targeting session, and multi-select session — none of
  // it load-bearing across messages (cleared by the view-reset effect below).
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

  // The entity a game-log reference last highlighted, if any (issue #260): a permanent
  // rings on the canvas and a player's panel lights up. Purely presentational — it opens
  // no actions and derives nothing — and ephemeral like every other selection here
  // (dropped on the next view below), so the table stays reconstructable from one view.
  const [highlightedId, setHighlightedId] = useState<EntityId | null>(null);
  // The entity whose **pinned** inspect panel is open, if any (issue #261). Ephemeral
  // presentation state like every selection here — never load-bearing across
  // messages (dropped on the next view below).
  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  // The entity whose **transient peek** is showing (issue #321) — a hover-dwell /
  // long-press preview. Distinct from the pinned panel: it never blocks input and is
  // cleared as soon as the pointer leaves. Also ephemeral, dropped on the next view.
  const [peekId, setPeekId] = useState<EntityId | null>(null);
  // The public zone whose browser is open, if any (issue #262) — a player's
  // graveyard or exile pile. Ephemeral like the inspect popover above.
  const [browsing, setBrowsing] = useState<{ playerId: PlayerId; zone: BrowsableZone } | null>(
    null,
  );
  // The rail sheet open on the compact composition (blueprint §Phone portrait):
  // the stack or the log, opened from the top bar's chips. Ephemeral presentation.
  const [railSheet, setRailSheet] = useState<RailSheet | null>(null);
  // The opponent the player has manually **expanded** on the phone-portrait
  // summary-tile composition (issue #400), if any. Purely ephemeral focus state —
  // never load-bearing: it is dropped on the next view (below) so a refresh
  // mid-focus reconstructs cleanly, and a decision auto-expands the needed board
  // regardless. Only meaningful in the tile composition.
  const [focusedTileId, setFocusedTileId] = useState<PlayerId | null>(null);
  // The control to move DOM focus to after an expand/collapse (issue #400), so the
  // focus order survives the tile ↔ panel swap: expanding lands on the new collapse
  // control, collapsing returns to the restored tile. A data-testid; transient.
  const [pendingTileFocus, setPendingTileFocus] = useState<string | null>(null);
  // Whether the keyboard shortcut reference overlay is open (issue #266). Ephemeral
  // UI, not game state — toggled with `?`.
  const [showHelp, setShowHelp] = useState(false);
  // Whether the card-art settings overlay is open (ADR 0024). Ephemeral UI state;
  // the chosen source itself is a device preference owned by the art store.
  const [showArtSettings, setShowArtSettings] = useState(false);
  // The art store's change counter (ADR 0024): bumps when an illustration finishes
  // loading (or the source/preference changes), so the scene rebuilds and cards
  // whose art arrived re-render. Presentation cache only — the scene remains fully
  // reconstructable from the view alone when the store is empty.
  const artVersion = useSyncExternalStore(subscribeArt, getArtVersion);

  // Tell the art store which cards the current view shows so their illustrations
  // load in the background under the player's chosen source (a no-op under the
  // procedural default).
  useEffect(() => {
    if (view) noteCards(collectArtCards(view));
  }, [view]);
  // The live table's root, for keyboard focus navigation and activation (issue
  // #266): the keyboard layer moves focus among and activates the buttons within it.
  const mainRef = useRef<HTMLElement>(null);
  // The shell's region geometry (issue #301), from the layout function — the basis
  // for the spatial focus model. Kept in a ref so the always-on key listener reads
  // the latest rects without re-subscribing on every resize/mode change. Assigned in
  // the render body below from the live `shell`.
  const focusGeometryRef = useRef<Map<string, Rect>>(new Map());

  // A fresh view supersedes any in-progress targeting or multi-select: the answer
  // either landed (server's response) or is now stale — most importantly, a changed
  // content-binding `token` invalidates the pending selection. Discarding both here
  // (and any open inspect popover / sheet) is what keeps the whole selection UI
  // reconstructable from one GameView + prompt (no load-bearing state across messages).
  useEffect(() => {
    setTargeting(null);
    setMultiSelect(null);
    setInspectedId(null);
    setPeekId(null);
    setBrowsing(null);
    setRailSheet(null);
    setHighlightedId(null);
    setFocusedTileId(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  // Escape dismissal and keyboard parity for core play (issues #266): two always-on
  // window key listeners that only ever act on what is on screen / in `valid_actions`.
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
    focusedTileId,
    mainRef,
    focusGeometryRef,
    setSelectedId,
    setTargeting,
    setMultiSelect,
    setInspectedId,
    setPeekId,
    setBrowsing,
    setRailSheet,
    setFocusedTileId,
    setShowHelp,
    setShowArtSettings,
  });

  const viewport = useViewport();
  const reducedMotion = useReducedMotion();
  // The number of seats the shell carves panels for: the receiver plus opponents.
  const playerCount = view ? view.opponents.length + 1 : 2;

  // Summary-tile focus (issue #400), resolved purely from the view plus the ephemeral
  // manual pick. `orderedOpp` is the seat-order opponent list the shell/scene index
  // frames by, so the expanded id maps to one opponent frame. The expanded opponent
  // is the one the player manually opened, else the first with an offered board
  // interaction (auto-expanded so no offered action is ever hidden). Only meaningful
  // in the tile composition; ignored elsewhere by the layout.
  const orderedOpp = useMemo(() => (view ? orderedOpponentIds(view) : []), [view]);
  const forcedFocusId = useMemo<PlayerId | null>(() => {
    if (!view) return null;
    const withCandidates = opponentsWithBoardCandidates(view);
    return orderedOpp.find((id) => withCandidates.has(id)) ?? null;
  }, [view, orderedOpp]);
  const manualFocusId =
    focusedTileId !== null && orderedOpp.includes(focusedTileId) ? focusedTileId : null;
  const expandedOpponentId = manualFocusId ?? forcedFocusId;
  const focusIndex = expandedOpponentId !== null ? orderedOpp.indexOf(expandedOpponentId) : -1;

  // The carved shell (ADR 0023): chrome region rects + the scene geometry. Pure
  // function of the measured viewport, seat count, and (tile composition only) which
  // opponent is expanded — regions never reorder.
  const shell = useMemo(
    () => layout(viewport, playerCount, focusIndex >= 0 ? { opponent: focusIndex } : undefined),
    [viewport, playerCount, focusIndex],
  );
  const compact = shell.composition === 'compact';

  // Toggle a summary tile (issue #400): expand a collapsed opponent, or collapse the
  // manually-expanded one. After the tile ↔ panel swap, move DOM focus to the newly
  // relevant control so the focus order survives expansion (keyboard parity).
  const toggleTileFocus = (playerId: PlayerId): void => {
    if (expandedOpponentId === playerId && forcedFocusId !== playerId) {
      setPendingTileFocus(`tile-focus-${playerId}`);
      setFocusedTileId(null);
    } else {
      setPendingTileFocus(`tile-collapse-${playerId}`);
      setFocusedTileId(playerId);
    }
  };
  // The tile-focus controls handed to the panel chrome — present only in the tile
  // composition. `pinned` marks an expansion forced by an offered decision (its board
  // must stay reachable, so it shows no manual collapse control).
  const tileFocus: TileFocus | undefined = shell.summaryTiles
    ? {
        expandedId: expandedOpponentId,
        pinned: manualFocusId === null && forcedFocusId !== null,
        onToggle: toggleTileFocus,
      }
    : undefined;
  // Move focus onto the post-toggle control once it has mounted (issue #400).
  useEffect(() => {
    if (pendingTileFocus === null) return;
    const el = mainRef.current?.querySelector<HTMLElement>(`[data-testid="${pendingTileFocus}"]`);
    el?.focus();
    setPendingTileFocus(null);
  }, [pendingTileFocus, expandedOpponentId]);

  // The region geometry the spatial focus model navigates by (issue #301): map each
  // tagged shell region to its layout rect, so `focus.ts` orders/adjoins regions the
  // way the table is laid out (not by DOM source order).
  const focusGeometry = useMemo(() => {
    const geometry = new Map<string, Rect>();
    const ids: RegionId[] = ['topBar', 'canvas', 'rail', 'mePanel', 'promptStrip', 'dock'];
    for (const id of ids) geometry.set(id, shell.regions[id].rect);
    return geometry;
  }, [shell]);
  useEffect(() => {
    focusGeometryRef.current = focusGeometry;
  }, [focusGeometry]);
  const prompt = useMemo(() => selectPendingPrompt(view), [view]);
  // The server names the receiver directly in `view.you`; an older server may
  // omit it (empty), which we treat as "unknown".
  const localId = view?.you || undefined;

  // The active multi-select slot and whether it is answered in the decision sheet
  // rather than on the canvas: an `order` list, or a `select_from_zone` whose
  // candidates are not board cards (graveyard/library). A hand/battlefield
  // selection stays on the canvas (candidates highlight in place).
  const msSlot = multiSelect ? msActiveSlot(multiSelect) : null;
  // A per-attacker defender pick (issue #347): its candidates are defending *players*,
  // chosen from the player panels (like single-target player targeting), not the
  // board — so it is neither an on-canvas pick nor a sheet list.
  const defenderSlot = !!msSlot && msSlot.kind === 'defender';
  const sheetMode =
    !!msSlot &&
    !!view &&
    !defenderSlot &&
    (msSlot.kind === 'order' || !msSlot.candidates.some((id) => isOnCanvas(view, id)));

  const scene = useMemo(() => {
    if (!view) return null;
    // In targeting / on-canvas multi-select mode the active slot's server candidates
    // drive highlight/dim; a multi-select also marks the already-chosen candidates.
    // In sheet mode the picking happens in the DOM sheet, so the board stays
    // neutral (no candidates passed) rather than dimming every card.
    let targetingScene: TargetingScene | undefined;
    if (multiSelect && defenderSlot) {
      // Assigning an attacker's defender: the pick surface is the player panels, so
      // the board stays neutral except the attacker being routed, which rings.
      targetingScene = undefined;
    } else if (multiSelect && !sheetMode) {
      targetingScene = {
        candidates: msActiveCandidates(multiSelect),
        selected: msActiveChosen(multiSelect),
      };
    } else if (targeting) {
      const activeReq = activeRequirement(targeting);
      if (activeReq) targetingScene = { candidates: activeReq.candidates ?? [] };
    }
    // Outside a targeting/multi-select flow the selection ring shows for the selected
    // entity, or — failing that — the entity a log reference is highlighting (issue
    // #260). While assigning a defender, ring the attacker being routed so the player
    // sees which creature the current pick applies to.
    const attackerRing =
      multiSelect && defenderSlot ? (msActiveAttacker(multiSelect) ?? undefined) : undefined;
    const sel =
      targeting || multiSelect ? attackerRing : (selectedId ?? highlightedId ?? undefined);
    return buildTableScene(view, sel, shell.scene, targetingScene);
    // `artVersion` is a rebuild trigger, not an input read here: the scene builder
    // reads the art store directly (per-card `artKey`), and the version bump is how
    // an illustration finishing its background load re-renders the affected cards.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    view,
    selectedId,
    highlightedId,
    shell,
    targeting,
    multiSelect,
    sheetMode,
    defenderSlot,
    artVersion,
  ]);

  // Publish the derived scene on the test-only window hook (ADR 0011). A no-op in
  // production builds; the e2e suite reads it to assert what the canvas draws.
  useEffect(() => {
    publishScene(scene);
  }, [scene]);

  // Publish the raw view alongside the scene (ADR 0011; issue #145): read-only, so
  // a browser-driven scripted game can read the offered `valid_actions` to decide
  // its next move, then submit by clicking the real UI. A no-op in production.
  useEffect(() => {
    publishView(view);
  }, [view]);

  if (!view || !scene) {
    // Socket is open (App only mounts the table then) but no frame has arrived
    // yet. Show a live status plus a Disconnect action so this is never a dead
    // screen; it resolves the instant the first GameView lands.
    return (
      <main className={s.main} data-testid="table-waiting">
        <div className={s.waitingBar}>
          <span className={s.muted}>Connected — waiting for first game state…</span>
          <button
            type="button"
            className={s.button}
            onClick={disconnect}
            data-testid="disconnect-button"
          >
            Disconnect
          </button>
        </div>
      </main>
    );
  }

  // The inspect preview's target and intensity (issues #261/#321), resolved from the
  // latest view. Priority: a pinned panel wins; else a transient peek; else the
  // current selection surfaces its preview in the same consistent home. A pinned
  // target renders the dismissible modal; everything else renders a non-blocking
  // peek. A stale id simply shows nothing — the preview is pure render of the view.
  const previewId = inspectedId ?? peekId ?? selectedId;
  const inspectTarget = previewId !== null ? resolveInspect(view, previewId) : null;
  const inspectTransient = inspectedId === null;
  const closeInspect = (): void => {
    setInspectedId(null);
    setPeekId(null);
  };

  // The open zone browser's contents, resolved from the view's public piles (issue
  // #262). Graveyard/exile are public, so any player's pile is browsable straight
  // from the view — the client derives nothing. Absent/unknown piles browse empty.
  const openZone = (playerId: PlayerId, zone: BrowsableZone): void =>
    setBrowsing({ playerId, zone });
  const closeBrowser = (): void => setBrowsing(null);
  const browserData = browsing
    ? {
        title: `${playerName(view, browsing.playerId)} — ${browsing.zone === 'graveyard' ? 'Graveyard' : 'Exile'}`,
        cards:
          (browsing.zone === 'graveyard' ? view.graveyards : view.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;

  // The live keyboard bindings shown in the shortcut reference (issue #266): Pass is
  // marked available only when the action is actually offered and no pick is
  // in-flight, so the reference reflects the current view, not a static cheat-sheet.
  const passOffered =
    view.valid_actions.some((action) => action.type === 'pass_priority') &&
    targeting === null &&
    multiSelect === null;
  const shortcutBindings = buildShortcutBindings(passOffered);

  // Toggle the game-log highlight for an entity/player (issue #260): clicking a
  // reference lights up its object; clicking the same one again clears it. Purely
  // presentational — it opens no actions and derives no legality. Shared by the live
  // and game-over branches (the log lives in the rail in both).
  const highlight = (id: EntityId): void =>
    setHighlightedId((current) => (current === id ? null : id));

  // The rail sheet (compact composition): the stack or activity log as a
  // viewport-clamped sheet above the shell, opened from the top bar's chips.
  const railSheetOverlay = railSheet && (
    <div className={s.sheetBackdrop} data-testid={`rail-sheet-${railSheet}`}>
      <div className={s.sheetPanel}>
        <button
          type="button"
          className={s.sheetClose}
          aria-label="Close"
          data-testid="rail-sheet-close"
          onClick={() => setRailSheet(null)}
        >
          ×
        </button>
        <Rail
          view={view}
          targeting={
            targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
          }
          onInspect={setInspectedId}
          onHighlight={highlight}
          highlightedId={highlightedId}
        />
      </div>
    </div>
  );

  // The inspect popover, zone browser, sheets, and shortcut help share one render
  // across both the live and game-over branches.
  const overlays = (
    <>
      {railSheetOverlay}
      {browserData && (
        <ZoneBrowser
          title={browserData.title}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={closeBrowser}
        />
      )}
      {inspectTarget && (
        <CardInspect target={inspectTarget} onClose={closeInspect} transient={inspectTransient} />
      )}
      {showHelp && <ShortcutHelp bindings={shortcutBindings} onClose={() => setShowHelp(false)} />}
      {showArtSettings && <ArtSettings onClose={() => setShowArtSettings(false)} />}
      <RejectionToast nonce={rejectionNonce} />
    </>
  );

  const r = shell.regions;
  const handAccent =
    localId !== undefined ? identityAccent(view, localId) : 'var(--rune-border-strong)';

  // Game over (issue #141): a terminal view carries `result`. The whole screen is
  // pure render of that latest view — the DOM overlay names the verdict/reason and
  // the interactive prompt/action UI is suppressed (the server sends no actions
  // once the game is over). The final board + panels stay visible beneath,
  // read-only. Regions dock exactly where they do during play (ADR 0023: regions
  // never reorder between states).
  if (view.result) {
    return (
      <GameOverTable
        view={view}
        result={view.result}
        scene={scene}
        shell={shell}
        viewport={viewport}
        localId={localId}
        highlightedId={highlightedId}
        reducedMotion={reducedMotion}
        onOpenZone={openZone}
        onInspect={setInspectedId}
        onPeek={setPeekId}
        onHighlight={highlight}
        overlays={overlays}
      />
    );
  }

  // The selected entity's actions come straight from what the server offered —
  // never recomputed. Suppressed during targeting / multi-select (entity selection
  // is inactive then; the only interaction is picking candidates).
  const selecting = targeting !== null || multiSelect !== null;
  const selectedActions =
    selectedId === null || selecting
      ? []
      : (prompt?.subjectActions ?? []).filter((action) => action.subject?.includes(selectedId));
  const selectedCard = findCard(scene, selectedId);

  // The presentation mode (issue #267), derived purely from the current view +
  // whether a decision is being resolved — never from history. Drives only
  // presentation (emphasis; the region placement is mode-invariant).
  const mode: TableMode = selecting || demandsDecision(view) ? 'focus' : 'overview';

  // Direct activation (ADR 0025): one gesture vocabulary — click, tap, or
  // keyboard activate — that shortcuts the select→dock round trip wherever the
  // intent is unambiguous, on every input method identically:
  //
  // 1. A combat-declaration candidate ENTERS the declaration with itself
  //    pre-toggled on the first activation (reversible until Confirm).
  // 2. A sole offered action the server flagged as a mana ability (CR 605)
  //    fires on the first activation — tap the land, get the mana.
  // 3. Otherwise the first activation selects (inspect + dock, as ever), and
  //    activating the already-selected entity again fires its sole action —
  //    entering targeting mode if it has slots. Several actions keep the dock
  //    as the disambiguator (the repeat activation is then a no-op).
  //
  // Every fired action is one the server offered on this entity — the gesture
  // only changes how it is reached, never what is legal (AGENTS.md hard rule).
  const activateEntity = (id: EntityId): void => {
    const card = findCard(scene, id);
    if (!card) return;
    if (card.actions.length === 0 && card.declaration) {
      const session = beginMultiSelect(card.declaration);
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
    const sole = card.actions.length === 1 ? card.actions[0] : undefined;
    if (sole?.mana_ability) {
      fire(sole);
      return;
    }
    if (selectedId !== id) {
      setSelectedId(id);
      return;
    }
    if (sole) fire(sole);
  };

  const activeReq = targeting ? activeRequirement(targeting) : null;
  const targetingBanner: TargetingBanner | null =
    targeting && activeReq
      ? {
          label: targeting.action.label,
          prompt: activeReq.prompt,
          step: targeting.picks.length + 1,
          total: targeting.action.requirements?.length ?? 0,
        }
      : null;

  // The player-panel pick contract (issue #347): a single-target player *targeting*
  // slot, or a multiplayer per-attacker *defender* pick — both choose a player from
  // their panel header (or the identity panel), so they share the same affordance
  // and differ only in the pick handler. Absent when no player choice is active.
  const playerTargeting =
    multiSelect && defenderSlot
      ? { candidates: msActiveCandidates(multiSelect), onPick: pickDefender }
      : targeting
        ? { candidates: activeCandidates(targeting), onPick: pickTarget }
        : undefined;

  // The option decision (if any) renders in the decision sheet: its named choices
  // plus the count affordance that blocks submit while a paired count slot is
  // partially filled (e.g. a mulligan whose bottoming is not yet complete).
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

  return (
    <main
      ref={mainRef}
      className={s.shell}
      data-mode={mode}
      data-composition={shell.composition}
      style={shellBox(viewport.width, viewport.height)}
    >
      {/* Top bar: brand · turn/phase strip · combat status · menu (compact: turn
          pill + dots + stack/log chips). One fixed status home (ADR 0023). */}
      <div style={regionBox(r.topBar.rect)} data-focus-region="topBar">
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
      </div>

      {/* The card canvas: every player panel's card area plus the hand, in one
          carved region. The Pixi scene draws the cards; the panel chrome and the
          interactive overlay position over it from reported rects (ADR 0003). */}
      <div style={regionBox(r.canvas.rect)} className={s.regionCanvas} data-focus-region="canvas">
        <div style={sceneBox(scene.width, scene.height)}>
          <BattlefieldCanvas scene={scene} isolatedId={highlightedId ?? selectedId} />
          <PanelChrome
            view={view}
            scene={scene}
            onOpenZone={openZone}
            targeting={playerTargeting}
            highlightedId={highlightedId}
            focus={tileFocus}
            reducedMotion={reducedMotion}
          />
          <EntityOverlay
            scene={scene}
            selectedId={selectedId}
            targeting={selecting}
            multiSelect={multiSelect !== null}
            pointer={viewport.pointer}
            onSelect={activateEntity}
            onPickTarget={multiSelect ? toggleCandidate : pickTarget}
            onPeek={setPeekId}
            onPinInspect={setInspectedId}
            onPlay={fire}
            onPlayOnTarget={fireOnTarget}
          />
        </div>
      </div>

      {/* Stack & activity rail — a fixed carved column (full composition). On
          compact its content lives behind the top bar's chips as sheets. */}
      {!compact && (
        <div style={regionBox(r.rail.rect)} className={s.regionRail} data-focus-region="rail">
          <Rail
            view={view}
            targeting={
              targeting
                ? { candidates: activeCandidates(targeting), onPick: pickTarget }
                : undefined
            }
            onInspect={setInspectedId}
            onHighlight={highlight}
            highlightedId={highlightedId}
          />
        </div>
      )}

      {/* Bottom shell (ADR 0023): identity panel · prompt strip + hand · action
          dock. Nothing may render over it. The hand panel box is chrome behind
          the canvas-drawn hand cards. */}
      <div
        style={
          {
            ...regionBox(r.handPanel.rect),
            pointerEvents: 'none',
            '--identity-accent': handAccent,
          } as React.CSSProperties
        }
        className={s.handPanelBox}
        data-testid="hand-panel"
        aria-hidden="true"
      />
      <div style={regionBox(r.mePanel.rect)} data-focus-region="mePanel">
        <MePanel
          view={view}
          localId={localId}
          condensed={compact}
          onOpenZone={openZone}
          targeting={playerTargeting}
          highlightedId={highlightedId}
        />
      </div>
      <div style={regionBox(r.promptStrip.rect)} data-focus-region="promptStrip">
        <PromptStrip
          view={view}
          prompt={prompt}
          targeting={targetingBanner}
          multiSelect={multiSelectBanner}
        />
      </div>
      <div style={regionBox(r.dock.rect)} className={s.regionDock} data-focus-region="dock">
        <ActionDock
          globalActions={
            // Concede is routed to the game menu (with a confirm step) so the
            // highest-stakes action never sits beside the most-pressed button.
            selecting
              ? []
              : (prompt?.globalActions ?? []).filter((action) => action.type !== 'concede')
          }
          selectedActions={selectedActions}
          selectedName={selectedCard?.name}
          onChoose={fire}
          onClearSelection={selectedId !== null ? () => setSelectedId(null) : undefined}
          onCancelTargeting={targeting ? cancelTargeting : undefined}
          multiSelect={multiSelectControls}
          waiting={prompt === null}
          deadline={selecting ? undefined : prompt?.deadline}
        />
      </div>
      <DecisionSheet
        view={view}
        multiSelect={multiSelect}
        sheetMode={sheetMode}
        msSlot={msSlot}
        onToggle={toggleCandidate}
        onMove={moveOrder}
        onChooseOption={chooseOption}
      />
      {overlays}
    </main>
  );
}
