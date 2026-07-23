/**
 * Keyboard parity for core play (issue #266) plus Escape dismissal (keyboard parity
 * with targeting-mode Cancel). Two always-on `window` key listeners the table shell
 * installs; every binding maps to an interaction the pointer path already has — the
 * keyboard introduces no new game semantics (AGENTS.md hard rule), and each handler
 * only ever acts on what is actually on screen / in `valid_actions`.
 *
 * The listeners read a bag of the table's live ephemeral state and setters. This is
 * pure wiring lifted out of the composition root — the dependency arrays match the
 * inlined effects exactly, so registration/teardown timing is unchanged.
 */
import { useEffect } from 'react';
import type { EntityId, GameView, PlayerId, TargetChoice, ValidAction } from '../../protocol';
import type { BrowsableZone } from '../PanelChrome';
import type { RailSheet } from '../TopBar';
import type { Rect } from '../scene';
import type { TargetingSession } from '../targeting';
import type { MultiSelectSession } from '../multiSelect';
import { collectFocusRegions, nextFocus, type FocusDir } from '../focus';

export interface TableKeyboardParams {
  view: GameView | null;
  choose: (action: ValidAction, targets?: TargetChoice[]) => void;
  targeting: TargetingSession | null;
  multiSelect: MultiSelectSession | null;
  showHelp: boolean;
  showArtSettings: boolean;
  inspectedId: EntityId | null;
  peekId: EntityId | null;
  browsing: { playerId: PlayerId; zone: BrowsableZone } | null;
  railSheet: RailSheet | null;
  focusedTileId: PlayerId | null;
  mainRef: React.RefObject<HTMLElement>;
  focusGeometryRef: React.MutableRefObject<Map<string, Rect>>;
  setSelectedId: React.Dispatch<React.SetStateAction<EntityId | null>>;
  setTargeting: React.Dispatch<React.SetStateAction<TargetingSession | null>>;
  setMultiSelect: React.Dispatch<React.SetStateAction<MultiSelectSession | null>>;
  setInspectedId: React.Dispatch<React.SetStateAction<EntityId | null>>;
  setPeekId: React.Dispatch<React.SetStateAction<EntityId | null>>;
  setBrowsing: React.Dispatch<
    React.SetStateAction<{ playerId: PlayerId; zone: BrowsableZone } | null>
  >;
  setRailSheet: React.Dispatch<React.SetStateAction<RailSheet | null>>;
  setFocusedTileId: React.Dispatch<React.SetStateAction<PlayerId | null>>;
  setShowHelp: React.Dispatch<React.SetStateAction<boolean>>;
  setShowArtSettings: React.Dispatch<React.SetStateAction<boolean>>;
}

export function useTableKeyboard(params: TableKeyboardParams): void {
  const {
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
  } = params;

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
    // Setters are stable; the reads listed are the exact triggers the inlined effect used.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    // Refs and setters are stable; these are the exact triggers the inlined effect used.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, choose, targeting, multiSelect, showHelp]);
}
