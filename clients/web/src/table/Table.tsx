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
 */
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import type { EntityId, GameView, PlayerId, ValidAction } from '../protocol';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../card/art/artStore';
import { ArtSettings } from './ArtSettings';
import { selectPendingPrompt, useGameStore } from '../store';
import { playerName } from '../playerNames';
import { publishScene, publishView } from '../testHooks';
import { ActionDock } from './ActionDock';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { PanelChrome, type BrowsableZone, type TileFocus } from './PanelChrome';
import { CardInspect, type InspectTarget } from './CardInspect';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { MePanel } from './MePanel';
import { PromptStrip, type MultiSelectBanner, type TargetingBanner } from './PromptStrip';
import { RejectionToast } from './RejectionToast';
import { ShortcutHelp, type Binding } from './ShortcutHelp';
import { Rail } from './Rail';
import { TopBar, type RailSheet } from './TopBar';
import { ZoneBrowser } from './ZoneBrowser';
import {
  buildTableScene,
  orderedOpponentIds,
  type Rect,
  type RenderedCard,
  type TableScene,
  type TargetingScene,
} from './scene';
import {
  activeCandidates,
  activeRequirement,
  assembleTargets,
  beginTargeting,
  pick,
  requiresTargets,
  type TargetingSession,
} from './targeting';
import {
  activeAttacker as msActiveAttacker,
  activeCandidates as msActiveCandidates,
  activeChosen as msActiveChosen,
  activeSlot as msActiveSlot,
  advance as msAdvance,
  allSlotsSatisfied,
  assembleChoices,
  beginMultiSelect,
  hasOptions,
  isLastSlot,
  isMultiSelect,
  moveInActiveSlot as msMove,
  optionsSubmittable,
  toggle as msToggle,
  type MultiSelectSession,
} from './multiSelect';
import { PromptSurface } from './PromptSurface';
import { layout, type RegionId, type Viewport } from './layout';
import { collectFocusRegions, nextFocus, type FocusDir } from './focus';
import { regionBox, sceneBox, shellBox } from './styles';
import { identityAccent } from './identityAccents';
import s from './chrome.module.css';

/** The table's presentation mode (issue #267). */
export type TableMode = 'overview' | 'focus';

/**
 * The measured viewport (width, height, pointer precision) the shell lays out
 * from, tracking the window so the whole table re-lays-out live on resize (the
 * layout itself stays a pure function — this only feeds it the live geometry).
 * Pointer precision is a capability, not a device (detected via a media query,
 * absent → `fine`), per ui-requirements §Input capability model.
 */
function detectPointer(): Viewport['pointer'] {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'fine';
  return window.matchMedia('(pointer: coarse)').matches ? 'coarse' : 'fine';
}

function useViewport(): Required<Viewport> {
  const read = (): Required<Viewport> =>
    typeof window === 'undefined'
      ? { width: 1280, height: 800, pointer: 'fine' }
      : {
          width: window.innerWidth,
          height: window.innerHeight,
          pointer: detectPointer() ?? 'fine',
        };
  const [viewport, setViewport] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onResize = (): void => setViewport(read());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return viewport;
}

/**
 * Whether the environment asks for reduced motion (`prefers-reduced-motion`). Drives
 * the summary-tile expand/collapse snap (issue #400). Read via a media query, absent
 * → false (SSR/older jsdom), and kept live so a mid-session OS change is honored.
 */
function useReducedMotion(): boolean {
  const query = '(prefers-reduced-motion: reduce)';
  const read = (): boolean =>
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(query).matches
      : false;
  const [reduced, setReduced] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(query);
    const onChange = (): void => setReduced(mq.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);
  return reduced;
}

/**
 * The opponents (by id) whose *battlefield* carries an offered card-level
 * interaction in the current view (issue #400): a permanent that is the subject of
 * a `valid_action` or a candidate of one of its requirement slots. On the
 * phone-portrait summary-tile composition these boards must stay reachable, so the
 * first such opponent is expanded automatically — the collapse never hides an
 * offered action. Derived purely from the view (candidates on the receiver's own
 * board, hand, or non-board zones are handled elsewhere and excluded here).
 */
function opponentsWithBoardCandidates(view: GameView): Set<PlayerId> {
  const offered = new Set<EntityId>();
  for (const action of view.valid_actions) {
    for (const subjectId of action.subject ?? []) offered.add(subjectId);
    for (const req of action.requirements ?? []) {
      for (const candidate of req.candidates ?? []) offered.add(candidate);
    }
  }
  const localId = view.you || undefined;
  const result = new Set<PlayerId>();
  if (offered.size === 0) return result;
  for (const perm of view.battlefield) {
    if (perm.controller !== localId && offered.has(perm.id)) result.add(perm.controller);
  }
  return result;
}

/** Find a rendered card anywhere in the scene by entity id. */
function findCard(scene: TableScene, id: EntityId | null): RenderedCard | undefined {
  if (id === null) return undefined;
  for (const band of scene.bands) {
    const hit = band.cards.find((card) => card.entityId === id);
    if (hit) return hit;
  }
  return scene.hand.find((card) => card.entityId === id);
}

/**
 * A display-name lookup across every zone whose cards the view exposes (hand,
 * battlefield, graveyards, exile). Used to label the decision sheet's rows for a
 * `select_from_zone`/`order` over a non-canvas zone; an id with no known card
 * (e.g. a hidden library card or an abstract ordered trigger) falls back to its id.
 */
function cardNameOf(view: GameView, id: EntityId): string {
  for (const card of view.my_hand) if (card.id === id) return card.name;
  for (const perm of view.battlefield) if (perm.id === id) return perm.card.name;
  for (const pile of view.graveyards)
    for (const card of pile.cards) if (card.id === id) return card.name;
  for (const pile of view.exile)
    for (const card of pile.cards) if (card.id === id) return card.name;
  return id;
}

/** Whether an id is rendered as a canvas card (hand or battlefield) in this view. */
function isOnCanvas(view: GameView, id: EntityId): boolean {
  return view.my_hand.some((card) => card.id === id) || view.battlefield.some((p) => p.id === id);
}

/**
 * Whether the view itself poses a forced decision (issue #267): a subject-less
 * action carrying target requirements or non-target prompts — a mulligan, discard,
 * order, mode choice, or a combat declaration the server is asking the receiver to
 * resolve. These land the table in focus mode straight from the view (so a fresh
 * mount is in the right mode), independent of any in-progress client selection. A
 * subject action (e.g. casting a targeted spell from a card) is the player's
 * optional move, not a forced decision, so it does not by itself force focus.
 */
function demandsDecision(view: GameView): boolean {
  return view.valid_actions.some(
    (action) =>
      (action.subject === undefined || action.subject.length === 0) &&
      ((action.requirements?.length ?? 0) > 0 || (action.prompts?.length ?? 0) > 0),
  );
}

/**
 * Resolve an entity id to what the inspect popover should show (issue #261),
 * searching every zone whose objects the view carries: the receiver's hand, the
 * battlefield (a permanent contributes its current face plus dynamic state), the
 * public graveyard/exile piles, and the stack. Presentation-only lookup over data
 * already in the view — it derives nothing. Returns `null` for an id that is not
 * inspectable in this view (e.g. it left its zone on a fresh frame).
 */
function resolveInspect(view: GameView, id: EntityId): InspectTarget | null {
  for (const card of view.my_hand) if (card.id === id) return { kind: 'card', card };
  for (const perm of view.battlefield) {
    if (perm.id === id) {
      // Attachment relationship (issue #333), resolved from the view for both sides:
      // the host this permanent is attached to (if visible), and the attachments this
      // permanent hosts. Presentation-only lookup — the client derives no rules.
      const host =
        perm.attached_to !== undefined
          ? view.battlefield.find((p) => p.id === perm.attached_to)
          : undefined;
      const attachments = view.battlefield
        .filter((p) => p.attached_to === perm.id)
        .map((p) => ({ id: p.id, name: p.card.name }));
      return {
        kind: 'card',
        card: perm.card,
        tapped: perm.tapped,
        counters: perm.counters,
        attachedTo: host ? { id: host.id, name: host.card.name } : undefined,
        attachments: attachments.length > 0 ? attachments : undefined,
      };
    }
  }
  for (const pile of view.graveyards) {
    for (const card of pile.cards) if (card.id === id) return { kind: 'card', card };
  }
  for (const pile of view.exile) {
    for (const card of pile.cards) if (card.id === id) return { kind: 'card', card };
  }
  for (const item of view.stack) if (item.id === id) return { kind: 'stack', item };
  return null;
}

export function Table() {
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const setStops = useGameStore((state) => state.setStops);
  const disconnect = useGameStore((state) => state.disconnect);
  // The rejected-action trigger (issue #265): a counter the store bumps whenever the
  // server flags a view as answering a rejected action. Feeds the transient toast below;
  // purely ephemeral presentation, nothing the table reconstructs from.
  const rejectionNonce = useGameStore((state) => state.rejectionNonce);
  const [selectedId, setSelectedId] = useState<EntityId | null>(null);
  // The entity a game-log reference last highlighted, if any (issue #260): a permanent
  // rings on the canvas and a player's panel lights up. Purely presentational — it opens
  // no actions and derives nothing — and ephemeral like every other selection here
  // (dropped on the next view below), so the table stays reconstructable from one view.
  const [highlightedId, setHighlightedId] = useState<EntityId | null>(null);
  const [targeting, setTargeting] = useState<TargetingSession | null>(null);
  const [multiSelect, setMultiSelect] = useState<MultiSelectSession | null>(null);
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
  }, [view]);

  // Escape abandons the topmost ephemeral surface, mirroring the targeting-mode
  // Cancel affordance (keyboard parity). An open inspect popover closes first (it
  // sits above everything, including an open zone browser), then the zone browser
  // or rail sheet, then a multi-select, then a targeting session, then the current
  // selection; a plain view ignores the key.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return;
      if (showHelp) setShowHelp(false);
      else if (showArtSettings) setShowArtSettings(false);
      else if (inspectedId !== null) setInspectedId(null);
      else if (peekId !== null) setPeekId(null);
      else if (browsing) setBrowsing(null);
      else if (railSheet) setRailSheet(null);
      else if (multiSelect) setMultiSelect(null);
      else if (targeting) setTargeting(null);
      else if (focusedTileId !== null) setFocusedTileId(null);
      else setSelectedId(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [
    showHelp,
    showArtSettings,
    inspectedId,
    peekId,
    browsing,
    railSheet,
    multiSelect,
    targeting,
    focusedTileId,
  ]);

  // Keyboard parity for core play (issue #266). Every binding maps to an
  // interaction the pointer already has — no new game semantics, all client-side:
  //
  // - Arrows move focus among the table's controls (never trapped: plain DOM focus,
  //   Tab still works natively); Enter/Space activate the focused control, reusing
  //   its own click handler (select, target-pick, multi-select toggle, confirm, …).
  // - Enter with nothing focused confirms an enabled multi-select (the primary
  //   pending action). `P` passes priority when that action is offered and no
  //   selection is in progress. `I` inspects the focused card. `?` toggles help.
  //
  // Shortcuts are inert when no matching action exists — the handlers only ever act
  // on what is actually on screen / in `valid_actions`.
  useEffect(() => {
    const moveFocus = (dir: FocusDir, event: KeyboardEvent): void => {
      const root = mainRef.current;
      if (!root) return;
      const regions = collectFocusRegions(root, focusGeometryRef.current);
      const next = nextFocus(regions, document.activeElement, dir);
      if (!next) return;
      event.preventDefault();
      next.focus();
    };

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const targetTag = (event.target as HTMLElement | null)?.tagName;
      if (targetTag === 'INPUT' || targetTag === 'TEXTAREA' || targetTag === 'SELECT') return;

      // `?` toggles the shortcut reference regardless of context.
      if (event.key === '?') {
        event.preventDefault();
        setShowHelp((open) => !open);
        return;
      }
      // While the help overlay is open, other shortcuts are inert (Escape closes it,
      // handled above); the native focus ring keeps the overlay usable.
      if (showHelp || !view) return;

      const root = mainRef.current;
      const active = document.activeElement;
      const focusedButton =
        active instanceof HTMLButtonElement && root?.contains(active) ? active : null;

      switch (event.key) {
        case 'Enter':
        case ' ':
        case 'Spacebar': {
          if (focusedButton) {
            event.preventDefault();
            focusedButton.click();
            return;
          }
          // Nothing focused: activate the primary pending action — an enabled
          // multi-select confirm (the ubiquitous "commit this decision").
          const confirm = root?.querySelector<HTMLButtonElement>(
            '[data-testid="multiselect-confirm"]:not([disabled])',
          );
          if (confirm) {
            event.preventDefault();
            confirm.click();
          }
          return;
        }
        case 'ArrowRight':
          moveFocus('right', event);
          return;
        case 'ArrowDown':
          moveFocus('down', event);
          return;
        case 'ArrowLeft':
          moveFocus('left', event);
          return;
        case 'ArrowUp':
          moveFocus('up', event);
          return;
        case 'p':
        case 'P': {
          // Pass/decline: only when the action is offered and no target/multi-select
          // pick is mid-flight (Escape backs out of those first).
          const selecting = targeting !== null || multiSelect !== null;
          const pass = view.valid_actions.find((action) => action.type === 'pass_priority');
          if (pass && !selecting) {
            event.preventDefault();
            choose(pass);
          }
          return;
        }
        case 'i':
        case 'I': {
          const id = active instanceof HTMLElement ? active.getAttribute('data-entity') : null;
          if (id) {
            event.preventDefault();
            setInspectedId(id);
          }
          return;
        }
        default:
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [view, choose, targeting, multiSelect, showHelp]);

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
  const shortcutBindings: Binding[] = [
    {
      id: 'arrows',
      keys: '← → ↑ ↓',
      description: 'Move focus across regions and items',
      available: true,
    },
    {
      id: 'enter',
      keys: 'Enter',
      description: 'Activate focused control / confirm',
      available: true,
    },
    {
      id: 'space',
      keys: 'Space',
      description: 'Toggle / activate focused control',
      available: true,
    },
    { id: 'pass', keys: 'P', description: 'Pass priority', available: passOffered },
    { id: 'inspect', keys: 'I', description: 'Inspect the focused card', available: true },
    { id: 'escape', keys: 'Esc', description: 'Cancel or close', available: true },
    { id: 'toggle-help', keys: '?', description: 'Toggle this help', available: true },
  ];

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
      <main
        className={s.shell}
        data-testid="table-game-over"
        data-mode="overview"
        data-composition={shell.composition}
        style={shellBox(viewport.width, viewport.height)}
      >
        <div style={regionBox(r.topBar.rect)}>
          <TopBar view={view} mode="overview" localId={localId} compact={compact} />
        </div>
        <div style={regionBox(r.canvas.rect)} className={s.regionCanvas}>
          <div style={sceneBox(scene.width, scene.height)}>
            <BattlefieldCanvas scene={scene} isolatedId={highlightedId} />
            <PanelChrome
              view={view}
              scene={scene}
              onOpenZone={openZone}
              highlightedId={highlightedId}
              reducedMotion={reducedMotion}
            />
            {/* Read-only game-over board: no select/target interaction (the server
                offers no actions once the game is over), but every card stays
                inspectable, so the overlay renders inspect handles only (#261). */}
            <EntityOverlay
              scene={scene}
              selectedId={null}
              targeting={false}
              pointer={viewport.pointer}
              onSelect={() => {}}
              onPickTarget={() => {}}
              onPeek={setPeekId}
              onPinInspect={setInspectedId}
            />
          </div>
        </div>
        {!compact && (
          <div style={regionBox(r.rail.rect)} className={s.regionRail}>
            <Rail
              view={view}
              onInspect={setInspectedId}
              onHighlight={highlight}
              highlightedId={highlightedId}
            />
          </div>
        )}
        <div style={regionBox(r.mePanel.rect)}>
          <MePanel view={view} localId={localId} condensed={compact} onOpenZone={openZone} />
        </div>
        <GameOverOverlay result={view.result} you={view.you} names={view.player_names} />
        {overlays}
      </main>
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

  // Fire an action: a multi-select declaration (combat / bottoming) opens the
  // toggle-and-confirm flow; a single-target action opens targeting mode; a plain
  // action is submitted immediately (token echoed, no targets).
  const fire = (action: ValidAction): void => {
    if (isMultiSelect(action)) {
      setSelectedId(null);
      setTargeting(null);
      setMultiSelect(beginMultiSelect(action));
      return;
    }
    if (requiresTargets(action)) {
      setSelectedId(null);
      setMultiSelect(null);
      setTargeting(beginTargeting(action));
      return;
    }
    choose(action);
    setSelectedId(null);
  };

  // Fire a targeted action with its first target already chosen — the drag-to-play
  // drop on a candidate (blueprint §Interaction model): cast + first target in one
  // gesture. A single-slot spell submits atomically right here; a multi-slot one
  // continues in the ordinary targeting flow for its remaining slots. The dropped
  // target is always one of the server-enumerated slot-0 candidates (the overlay
  // only offers those), and `pick` re-checks it against the session's active slot.
  const fireOnTarget = (action: ValidAction, target: EntityId): void => {
    if (!requiresTargets(action)) {
      fire(action);
      return;
    }
    setSelectedId(null);
    setMultiSelect(null);
    const advanced = pick(beginTargeting(action), target);
    const targets = assembleTargets(advanced);
    if (targets !== null) {
      choose(advanced.action, targets);
      setTargeting(null);
    } else {
      setTargeting(advanced);
    }
  };

  // Pick a target for the active slot. When the last slot is filled, assemble and
  // submit the whole answer atomically (action token + one choice per slot).
  function pickTarget(entityId: EntityId): void {
    if (!targeting) return;
    const advanced = pick(targeting, entityId);
    const targets = assembleTargets(advanced);
    if (targets !== null) {
      choose(advanced.action, targets);
      setTargeting(null);
    } else {
      setTargeting(advanced);
    }
  }

  // Toggle a candidate into (or out of) the active multi-select slot. Nothing is
  // submitted until the player confirms (or picks an option).
  const toggleCandidate = (entityId: EntityId): void => {
    if (!multiSelect) return;
    setMultiSelect(msToggle(multiSelect, entityId));
  };

  // Assign a defending player to the attacker of the active `defender` slot (issue
  // #347), then advance to the next declared attacker awaiting a target. A defender is
  // a single choice, so the pick replaces any prior one; after the last attacker the
  // advance clamps and Confirm submits the whole declaration atomically.
  const pickDefender = (playerId: EntityId): void => {
    if (!multiSelect) return;
    setMultiSelect((prev) => (prev ? msAdvance(msToggle(prev, playerId)) : prev));
  };

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

  // Advance to the next walked slot (per-attacker blocker assignment).
  const advanceSlot = (): void => {
    if (!multiSelect) return;
    setMultiSelect(msAdvance(multiSelect));
  };

  // Confirm the whole selection atomically (used when there is no option prompt).
  const confirmMultiSelect = (): void => {
    if (!multiSelect) return;
    choose(multiSelect.action, assembleChoices(multiSelect));
    setMultiSelect(null);
  };

  // Move an item one step within the active `order` slot (issue #157). Nothing is
  // submitted until the player confirms — reordering only edits the pending answer.
  const moveOrder = (entityId: EntityId, direction: -1 | 1): void => {
    if (!multiSelect) return;
    setMultiSelect(msMove(multiSelect, entityId, direction));
  };

  // Submit an option decision (the sheet's modal picker, e.g. mulligan keep/take-
  // another) together with the current per-slot selection (e.g. the bottomed cards)
  // in one atomic answer, keyed by the option slot the server posed.
  const chooseOption = (optionId: string): void => {
    if (!multiSelect) return;
    const optionSlot = multiSelect.options[0];
    const extra = optionSlot ? [{ slot: optionSlot.slot, chosen: [optionId] }] : [];
    choose(multiSelect.action, assembleChoices(multiSelect, extra));
    setMultiSelect(null);
  };

  const cancelTargeting = (): void => setTargeting(null);
  const cancelMultiSelect = (): void => setMultiSelect(null);

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

  // The decision sheet's rows for a sheet-mode slot: an `order` list (items
  // in current order) or a non-canvas `select_from_zone` (candidates with chosen).
  const surfaceChosen = multiSelect ? msActiveChosen(multiSelect) : [];
  const surfaceItems =
    sheetMode && msSlot
      ? (msSlot.kind === 'order' ? surfaceChosen : msSlot.candidates).map((id) => ({
          id,
          label: cardNameOf(view, id),
          chosen: surfaceChosen.includes(id),
        }))
      : [];

  // The option picker's named choices (issue #157), rendered in the decision sheet
  // — one of the only layers permitted to cover the shell, viewport-clamped.
  const optionControls = multiSelect && hasOptions(multiSelect) ? multiSelect.options[0] : null;
  const decisionSheet =
    (sheetMode && msSlot) || optionControls ? (
      <div className={s.sheetBackdrop} data-testid="decision-sheet">
        <div className={s.sheetPanel}>
          {sheetMode && msSlot && (
            <PromptSurface
              mode={msSlot.kind === 'order' ? 'order' : 'select'}
              prompt={msSlot.prompt}
              zone={msSlot.zone}
              items={surfaceItems}
              onToggle={toggleCandidate}
              onMove={moveOrder}
            />
          )}
          {multiSelect && optionControls && (
            <div className={s.sheetOptions} data-testid="multiselect-options">
              {optionControls.prompt !== undefined && <span>{optionControls.prompt}</span>}
              {(optionControls.options ?? []).map((option) => (
                <button
                  key={option.id}
                  type="button"
                  onClick={() => chooseOption(option.id)}
                  disabled={!optionsSubmittable(multiSelect)}
                  data-testid={`multiselect-option-${option.id}`}
                  className={s.optionButton}
                >
                  {option.label}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    ) : null;

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
      {decisionSheet}
      {overlays}
    </main>
  );
}
