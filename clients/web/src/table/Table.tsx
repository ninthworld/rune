/**
 * The playable table: the single React tree that reconstructs the whole UI from
 * the store's latest {@link GameView} (plus its derived pending prompt).
 *
 * - Pixi canvas draws battlefield bands + hand (ADR 0003).
 * - DOM islands render the prompt banner, player tiles, the interactive overlay,
 *   and the action bar.
 * - Interactivity is driven entirely by `valid_actions[]`; choosing an action
 *   sends `ChooseAction` through the store, and the UI then rebuilds purely from
 *   the resulting GameView. The only client-side state is ephemeral selection
 *   (nothing load-bearing across messages — the reconnect/replay invariant).
 *
 * Targeting mode (ADR 0009 §Client): choosing an action that carries
 * `requirements` enters a data-driven targeting flow. The client walks the
 * requirement slots as a prompt queue, offering only the server-listed candidates
 * (highlighted; everything else dimmed and inert), then submits the whole answer
 * atomically with the action's content-binding token — one `ChooseAction`, never
 * a multi-message handshake. The in-progress selection is ephemeral and discarded
 * on the next view, so the UI stays reconstructable from one GameView + prompt.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { EntityId, GameView, PlayerId, ValidAction } from '../protocol';
import { selectPendingPrompt, useGameStore } from '../store';
import { playerName } from '../playerNames';
import { publishScene, publishView } from '../testHooks';
import { ActionTray } from './ActionTray';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { TableGeography, type BrowsableZone } from './TableGeography';
import { CardInspect, type InspectTarget } from './CardInspect';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { PhaseIndicator, type TableMode } from './PhaseIndicator';
import { OpponentHud, LocalDock } from './PlayerHud';
import { PromptBanner } from './PromptBanner';
import { RejectionToast } from './RejectionToast';
import { ShortcutHelp, type Binding } from './ShortcutHelp';
import { Rail } from './Rail';
import { ZoneBrowser } from './ZoneBrowser';
import {
  buildTableScene,
  DEFAULT_VIEWPORT_WIDTH,
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
import { promptOverlayBox, regionBox, sceneBox, shellBox, trayBox } from './styles';
import { cx } from '../chrome/cx';
import s from './chrome.module.css';

/**
 * The measured viewport (width, height, pointer precision) the shell lays out
 * from, tracking the window so the whole table re-lays-out live on resize (the
 * layout itself stays a pure function — this only feeds it the live geometry).
 * Falls back to the {@link DEFAULT_VIEWPORT_WIDTH}-shaped default where there is no
 * `window` (SSR/tests). Pointer precision is a capability, not a device (detected
 * via a media query, absent → `fine`), per ui-requirements §Input capability model.
 */
function detectPointer(): Viewport['pointer'] {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'fine';
  return window.matchMedia('(pointer: coarse)').matches ? 'coarse' : 'fine';
}

function useViewport(): Required<Viewport> {
  const read = (): Required<Viewport> =>
    typeof window === 'undefined'
      ? { width: DEFAULT_VIEWPORT_WIDTH, height: 800, pointer: 'fine' }
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
 * The union of the given entities' card rects in scene coordinates (issue #298), or
 * `null` when none of them is a rendered card. This is how the anchored prompt overlay
 * finds "where its subjects are" — straight from the scene's REPORTED RECTS (ADR 0003),
 * never by reaching into Pixi. Ids that are not on the canvas (a player portrait, a
 * non-visible zone) simply contribute nothing.
 */
function sceneBoundsOf(scene: TableScene, ids: EntityId[]): Rect | null {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const id of ids) {
    const card = findCard(scene, id);
    if (!card) continue;
    minX = Math.min(minX, card.rect.x);
    minY = Math.min(minY, card.rect.y);
    maxX = Math.max(maxX, card.rect.x + card.rect.w);
    maxY = Math.max(maxY, card.rect.y + card.rect.h);
  }
  if (minX === Infinity) return null;
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}

/**
 * A display-name lookup across every zone whose cards the view exposes (hand,
 * battlefield, graveyards, exile). Used to label the prompt surface's rows for a
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
      return { kind: 'card', card: perm.card, tapped: perm.tapped, counters: perm.counters };
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
  // rings on the canvas and a player's tile lights up. Purely presentational — it opens
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
  // Whether the keyboard shortcut reference overlay is open (issue #266). Ephemeral
  // UI, not game state — toggled with `?`.
  const [showHelp, setShowHelp] = useState(false);
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
  // (and any open inspect popover) is what keeps the whole selection UI
  // reconstructable from one GameView + prompt (no load-bearing state across messages).
  useEffect(() => {
    setTargeting(null);
    setMultiSelect(null);
    setInspectedId(null);
    setPeekId(null);
    setBrowsing(null);
    setHighlightedId(null);
  }, [view]);

  // Escape abandons the topmost ephemeral surface, mirroring the targeting-mode
  // Cancel affordance (keyboard parity). An open inspect popover closes first (it
  // sits above everything, including an open zone browser), then the zone browser,
  // then a multi-select, then a targeting session; a plain view ignores the key.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return;
      if (showHelp) setShowHelp(false);
      else if (inspectedId !== null) setInspectedId(null);
      else if (peekId !== null) setPeekId(null);
      else if (browsing) setBrowsing(null);
      else if (multiSelect) setMultiSelect(null);
      else if (targeting) setTargeting(null);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [showHelp, inspectedId, peekId, browsing, multiSelect, targeting]);

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
  // The number of seats the HUD strip reflows for: the receiver plus its opponents.
  const playerCount = view ? view.opponents.length + 1 : 2;
  // The shell region rects, from the measured geometry (issue #295). Geometry is
  // mode-invariant, so the placeholder mode here never moves a region — the live
  // `mode` (derived below) only drives density/emphasis via `data-mode`. The
  // battlefield rect's width is the wrap budget the scene consumes, so the board
  // never scrolls horizontally.
  const shell = useMemo(() => layout(viewport, 'overview', playerCount), [viewport, playerCount]);
  const battlefieldW = shell.regions.battlefield.rect.w;

  // The region geometry the spatial focus model navigates by (issue #301): map each
  // tagged shell region to its layout rect, so `focus.ts` orders/adjoins regions the
  // way the table is laid out (not by DOM source order). Includes every interactive
  // surface the keyboard must reach — the opponent HUD, the local dock (#296), the
  // board, the rail (#299), the action tray, and the staged prompt overlay (#298),
  // whose anchored surface is registered as a synthetic `overlay` rect just above the
  // tray so pressing Down from the board reaches its option/order controls. Held in a
  // ref (below) so the always-on key listener reads the latest rects.
  const focusGeometry = useMemo(() => {
    const geometry = new Map<string, Rect>();
    const ids: RegionId[] = [
      'indicator',
      'opponentHud',
      'localDock',
      'battlefield',
      'rail',
      'tray',
    ];
    for (const id of ids) geometry.set(id, shell.regions[id].rect);
    // The prompt overlay is not a layout region (it anchors to the decision's subjects
    // at render time); give it a stable rect just above the tray so region ordering is
    // deterministic even where the DOM reports no box (jsdom/SSR).
    const tray = shell.regions.tray.rect;
    const bf = shell.regions.battlefield.rect;
    geometry.set('overlay', { x: bf.x, y: tray.y - 1, w: bf.w, h: 1 });
    return geometry;
  }, [shell]);
  useEffect(() => {
    focusGeometryRef.current = focusGeometry;
  }, [focusGeometry]);
  const prompt = useMemo(() => selectPendingPrompt(view), [view]);
  // The server names the receiver directly in `view.you`; an older server may
  // omit it (empty), which we treat as "unknown".
  const localId = view?.you || undefined;

  // The active multi-select slot and whether it is answered in the DOM prompt
  // surface rather than on the canvas: an `order` list, or a `select_from_zone`
  // whose candidates are not board cards (graveyard/library). A hand/battlefield
  // selection stays on the canvas (candidates highlight in place).
  const msSlot = multiSelect ? msActiveSlot(multiSelect) : null;
  const overlayMode =
    !!msSlot &&
    !!view &&
    (msSlot.kind === 'order' || !msSlot.candidates.some((id) => isOnCanvas(view, id)));

  const scene = useMemo(() => {
    if (!view) return null;
    // In targeting / on-canvas multi-select mode the active slot's server candidates
    // drive highlight/dim; a multi-select also marks the already-chosen candidates.
    // In overlay mode the picking happens in the DOM surface, so the board stays
    // neutral (no candidates passed) rather than dimming every card.
    let targetingScene: TargetingScene | undefined;
    if (multiSelect && !overlayMode) {
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
    // #260); a permanent thus rings whether it was picked on the board or in the log.
    const sel = targeting || multiSelect ? undefined : (selectedId ?? highlightedId ?? undefined);
    return buildTableScene(view, sel, battlefieldW, targetingScene);
  }, [view, selectedId, highlightedId, battlefieldW, targeting, multiSelect, overlayMode]);

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

  // The inspect popover, zone browser, and shortcut help share one render across both
  // the live and game-over branches; extracted here so each branch mounts the same
  // overlays.
  const overlays = (
    <>
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
      <RejectionToast nonce={rejectionNonce} />
    </>
  );

  // Toggle the game-log highlight for an entity/player (issue #260): clicking a
  // reference lights up its object; clicking the same one again clears it. Purely
  // presentational — it opens no actions and derives no legality. Shared by the live
  // and game-over branches (the log lives in the rail in both).
  const highlight = (id: EntityId): void =>
    setHighlightedId((current) => (current === id ? null : id));

  // Game over (issue #141): a terminal view carries `result`. The whole screen is
  // pure render of that latest view — the DOM overlay names the verdict/reason and
  // the interactive prompt/action UI is suppressed (the server sends no actions
  // once the game is over). The final board + tiles stay visible beneath, read-only
  // (no EntityOverlay, so nothing is selectable/targetable). Nothing is load-bearing
  // across messages, so a reconnect that replays this view shows the same screen.
  if (view.result) {
    const r = shell.regions;
    return (
      <main
        className={s.shell}
        data-testid="table-game-over"
        data-mode="overview"
        style={shellBox(viewport.width, viewport.height)}
      >
        {/* The final board renders full-bleed in overview treatment beneath the
            overlay; every region docks in exactly the place it does during play
            (regions never reorder between states). */}
        <div className={s.regionIndicator} style={regionBox(r.indicator.rect)}>
          <PhaseIndicator view={view} mode="overview" localId={localId} />
        </div>
        <div className={s.regionHud} style={regionBox(r.opponentHud.rect)}>
          <OpponentHud view={view} highlightedId={highlightedId} />
        </div>
        <div className={s.regionLocalDock} style={regionBox(r.localDock.rect)}>
          <LocalDock view={view} localId={localId} highlightedId={highlightedId} />
        </div>
        <div className={s.regionBattlefield} style={regionBox(r.battlefield.rect)}>
          <div style={sceneBox(scene.width, scene.height)}>
            <BattlefieldCanvas scene={scene} />
            {/* Labeled lanes + zone piles stay on the final board. Card actions are
                gone (no EntityOverlay select), but graveyard/exile stay browsable
                here exactly as they do on the tiles above (issue #262). */}
            <TableGeography scene={scene} onOpenZone={openZone} />
            {/*
             * Read-only game-over board: no select/target interaction (the server
             * offers no actions once the game is over), but every card stays
             * inspectable, so the overlay renders inspect handles only (issue #261).
             */}
            <EntityOverlay
              scene={scene}
              selectedId={null}
              targeting={false}
              pointer={viewport.pointer}
              onSelect={() => {}}
              onChoose={() => {}}
              onPickTarget={() => {}}
              onPeek={setPeekId}
              onPinInspect={setInspectedId}
            />
          </div>
        </div>
        {/* Stack & activity rail — right edge. On the terminal frame it is read-only
            (no targeting), but the stack stays inspectable and the rail collapses to
            its badge on narrow geometry exactly as during play (issue #299). */}
        <Rail
          view={view}
          rect={r.rail.rect}
          collapsed={shell.railCollapsed}
          onInspect={setInspectedId}
          onHighlight={highlight}
          highlightedId={highlightedId}
        />
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
  // whether a decision is being resolved — never from history. Focus when the
  // player has drilled into a targeting/multi-select flow, or when the view itself
  // poses a forced decision (so a fresh mid-prompt mount is already in focus);
  // overview otherwise. Drives only presentation (the ribbon emphasis + a light
  // de-emphasis of the standings chrome).
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

  // Pick a target for the active slot. When the last slot is filled, assemble and
  // submit the whole answer atomically (action token + one choice per slot).
  const pickTarget = (entityId: EntityId): void => {
    if (!targeting) return;
    const advanced = pick(targeting, entityId);
    const targets = assembleTargets(advanced);
    if (targets !== null) {
      choose(advanced.action, targets);
      setTargeting(null);
    } else {
      setTargeting(advanced);
    }
  };

  // Toggle a candidate into (or out of) the active multi-select slot. Nothing is
  // submitted until the player confirms (or picks an option).
  const toggleCandidate = (entityId: EntityId): void => {
    if (!multiSelect) return;
    setMultiSelect(msToggle(multiSelect, entityId));
  };

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

  // Submit an option decision (the banner's modal picker, e.g. mulligan keep/take-
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
  const toggleSelect = (id: EntityId): void =>
    setSelectedId((current) => (current === id ? null : id));

  const activeReq = targeting ? activeRequirement(targeting) : null;
  const targetingBanner =
    targeting && activeReq
      ? {
          label: targeting.action.label,
          prompt: activeReq.prompt,
          step: targeting.picks.length + 1,
          total: targeting.action.requirements?.length ?? 0,
        }
      : null;

  // The option decision (if any) renders as the banner's modal picker: its named
  // choices plus the count affordance that blocks submit while a paired count slot
  // is partially filled (e.g. a mulligan whose bottoming is not yet complete).
  const multiSelectBanner =
    multiSelect && (msSlot || hasOptions(multiSelect))
      ? {
          label: multiSelect.action.label,
          prompt: msSlot?.prompt ?? multiSelect.options[0]?.prompt ?? '',
          step: multiSelect.active + 1,
          total: multiSelect.slots.length,
          chosen: msActiveChosen(multiSelect).length,
          required: msSlot?.kind === 'count' ? msSlot.count : undefined,
          slotKind: msSlot?.kind,
          options: hasOptions(multiSelect)
            ? (multiSelect.options[0]?.options ?? []).map((option) => ({
                id: option.id,
                label: option.label,
              }))
            : undefined,
          optionPrompt: hasOptions(multiSelect) ? multiSelect.options[0]?.prompt : undefined,
          optionsEnabled: optionsSubmittable(multiSelect),
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

  // The DOM prompt surface's rows for an overlay-mode slot: an `order` list (items
  // in current order) or a non-canvas `select_from_zone` (candidates with chosen).
  const surfaceChosen = multiSelect ? msActiveChosen(multiSelect) : [];
  const surfaceItems =
    overlayMode && msSlot
      ? (msSlot.kind === 'order' ? surfaceChosen : msSlot.candidates).map((id) => ({
          id,
          label: cardNameOf(view, id),
          chosen: surfaceChosen.includes(id),
        }))
      : [];

  const r = shell.regions;
  const bf = r.battlefield.rect;

  // The anchored prompt overlay (issue #298): while a decision is being resolved it
  // stages as a focused surface positioned relative to the SUBJECTS the decision
  // concerns — the active slot's candidate cards, the source card, or (failing any
  // on-canvas subject) centered above the tray. Anchoring reads the scene's reported
  // rects (ADR 0003), translated from scene space into the battlefield region; it
  // never reaches into Pixi and needs no targeting arrows on the canvas.
  let anchorIds: EntityId[] = [];
  if (targeting) {
    const req = activeRequirement(targeting);
    const cands = (req?.candidates ?? []).filter((id) => isOnCanvas(view, id));
    anchorIds = cands.length
      ? cands
      : (targeting.action.subject ?? []).filter((id) => isOnCanvas(view, id));
  } else if (multiSelect) {
    if (!overlayMode && msSlot) {
      anchorIds = msSlot.candidates.filter((id) => isOnCanvas(view, id));
    }
    if (anchorIds.length === 0) {
      anchorIds = (multiSelect.action.subject ?? []).filter((id) => isOnCanvas(view, id));
    }
  }
  const bounds = sceneBoundsOf(scene, anchorIds);
  const gap = 10;
  const clampY = (value: number): number => Math.max(bf.y + 8, Math.min(value, bf.y + bf.h - 8));
  // Grow upward from just above the subjects when they sit lower on the board, downward
  // from just below them when they sit near the top — so the overlay never runs off an
  // edge. With no on-canvas subject (a non-visible zone / abstract order), centre it
  // above the tray.
  const promptAnchor = bounds
    ? bounds.y > bf.h * 0.35
      ? {
          centerX: bf.x + bounds.x + bounds.w / 2,
          y: clampY(bf.y + bounds.y - gap),
          place: 'above' as const,
        }
      : {
          centerX: bf.x + bounds.x + bounds.w / 2,
          y: clampY(bf.y + bounds.y + bounds.h + gap),
          place: 'below' as const,
        }
    : { centerX: bf.x + bf.w / 2, y: r.tray.rect.y - gap, place: 'above' as const };

  return (
    <main
      ref={mainRef}
      className={s.shell}
      data-mode={mode}
      style={shellBox(viewport.width, viewport.height)}
    >
      {/* Turn/phase indicator — top (issue #297 redesigns its internals). Tagged as a
          focus region (issue #301) so keyboard navigation reaches its controls. */}
      <div
        className={s.regionIndicator}
        style={regionBox(r.indicator.rect)}
        data-focus-region="indicator"
      >
        <PhaseIndicator view={view} mode={mode} localId={localId} onSetStops={setStops} />
      </div>
      {/*
       * Opponent HUD strip — top: identity + life prominent, hand/statuses secondary
       * (issue #296). It reflows purely by opponent count and never pulls the local
       * player up (they live in the dock below). In focus mode the standings chrome is
       * lightly de-emphasized so attention lands on the pending decision — presentation
       * only, derived from `mode` (issue #267).
       */}
      <div
        className={cx(s.regionHud, mode === 'focus' && s.focusDimmed)}
        style={regionBox(r.opponentHud.rect)}
        data-focus-region="opponentHud"
      >
        <OpponentHud
          view={view}
          highlightedId={highlightedId}
          targeting={
            targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
          }
        />
      </div>
      {/* Local player dock — bottom-left: identity, life, floating mana (issue #296).
          A self-target candidate is pickable here with the same ring/dim contract. */}
      <div
        className={cx(s.regionLocalDock, mode === 'focus' && s.focusDimmed)}
        style={regionBox(r.localDock.rect)}
        data-focus-region="localDock"
      >
        <LocalDock
          view={view}
          localId={localId}
          highlightedId={highlightedId}
          targeting={
            targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
          }
        />
      </div>
      {/* Battlefield — the center, owning most of the viewport. The Pixi scene sizes
          to this region's width, so it never scrolls horizontally. */}
      <div
        className={s.regionBattlefield}
        style={regionBox(r.battlefield.rect)}
        data-focus-region="battlefield"
      >
        <div style={sceneBox(scene.width, scene.height)}>
          <BattlefieldCanvas scene={scene} />
          {/* Labeled, bounded player lanes + zone piles (issue #278), anchored to the
              scene's band/hand rects and stacked under the interactive overlay so it
              never intercepts a card click. */}
          <TableGeography scene={scene} onOpenZone={openZone} />
          <EntityOverlay
            scene={scene}
            selectedId={selectedId}
            targeting={selecting}
            multiSelect={multiSelect !== null}
            pointer={viewport.pointer}
            onSelect={toggleSelect}
            onChoose={fire}
            onPickTarget={multiSelect ? toggleCandidate : pickTarget}
            onPeek={setPeekId}
            onPinInspect={setInspectedId}
          />
        </div>
      </div>
      {/* Stack & activity rail — right edge: a collapsible panel that auto-expands
          while the stack is populated and collapses to a count badge on narrow
          geometry, reserving a slot for the game log (issue #299 / #260). Wrapped in a
          `rail` focus region (issue #301) so keyboard navigation reaches its controls;
          the wrapper adds no layout (the Rail positions itself absolutely). */}
      <div data-focus-region="rail">
        <Rail
          view={view}
          rect={r.rail.rect}
          collapsed={shell.railCollapsed}
          targeting={
            targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
          }
          onInspect={setInspectedId}
          onHighlight={highlight}
          highlightedId={highlightedId}
        />
      </div>
      {/* Anchored prompt overlay (issue #298): a focused decision surface staged over
          the board, positioned (inline, from reported rects) relative to the decision's
          subjects. Present only while a decision is being resolved; the banner, option
          picker, deadline countdown, and the order/zone prompt surface all ride it. */}
      {selecting && (
        <div
          className={s.promptOverlay}
          data-testid="prompt-overlay"
          data-focus-region="overlay"
          data-placement={promptAnchor.place}
          style={promptOverlayBox(promptAnchor, bf)}
        >
          <PromptBanner
            view={view}
            prompt={prompt}
            targeting={targetingBanner}
            multiSelect={multiSelectBanner}
            onOption={chooseOption}
          />
          {overlayMode && msSlot && (
            <PromptSurface
              mode={msSlot.kind === 'order' ? 'order' : 'select'}
              prompt={msSlot.prompt}
              zone={msSlot.zone}
              items={surfaceItems}
              onToggle={toggleCandidate}
              onMove={moveOrder}
            />
          )}
        </div>
      )}
      {/* Floating action tray above the hand (issue #298): global actions + the selected
          entity's echo in the neutral state; the decision controls (multi-select
          confirm/advance/cancel, targeting cancel) while a decision is staged. It reads
          "waiting" quietly when the server offers nothing. */}
      <div
        className={s.regionTray}
        style={trayBox(r.tray.rect, viewport.height)}
        data-focus-region="tray"
      >
        <ActionTray
          globalActions={selecting ? [] : (prompt?.globalActions ?? [])}
          selectedActions={selectedActions}
          selectedName={selectedCard?.name}
          onChoose={fire}
          onCancelTargeting={targeting ? cancelTargeting : undefined}
          multiSelect={multiSelectControls}
          waiting={prompt === null}
          deadline={selecting ? undefined : prompt?.deadline}
        />
      </div>
      {overlays}
    </main>
  );
}
