/**
 * The in-game menu (React DOM, ADR 0003): a small always-present affordance at the
 * shell's top-right that opens a restrained drawer of session-level actions —
 * keyboard shortcuts and, when the server offers it, concede.
 *
 * Concede deliberately lives here and NOT in the action tray: it is the
 * highest-stakes action in the game and must never sit one slip away from "Pass
 * priority" (the most-pressed button). It stays a server-offered `valid_actions[]`
 * entry — the menu only relocates the affordance and adds a confirm step; the
 * client still computes no legality (a menu with no offered concede simply shows
 * none). Menu and confirm state are ephemeral presentation, never load-bearing
 * across messages (the reconnect/replay invariant).
 */
import { useEffect, useState } from 'react';
import type { ValidAction } from '../protocol';
import { cx } from '../chrome/cx';
import { SymbolText } from '../chrome/symbols';
import s from './chrome.module.css';

interface Props {
  /** The server-offered concede action, if any (`type: "concede"`). */
  concede?: ValidAction;
  /** Submit a chosen action (the store reads its id + token). */
  onChoose: (action: ValidAction) => void;
  /** Open the keyboard-shortcut reference overlay (issue #266). */
  onShowShortcuts: () => void;
  /** Open the display settings overlay (issue #505); absent hides the item. */
  onShowSettings?: () => void;
  /** Open the card-art settings overlay (ADR 0024); absent hides the item. */
  onShowArtSettings?: () => void;
  /**
   * Controlled open state. Supplied, this component renders the drawer only and
   * the caller owns the handle — see the note in the body.
   */
  open?: boolean;
  /** Report a requested open/close while controlled. */
  onOpenChange?: (open: boolean) => void;
}

export function GameMenu({
  concede,
  onChoose,
  onShowShortcuts,
  onShowSettings,
  onShowArtSettings,
  open: controlledOpen,
  onOpenChange,
}: Props) {
  // Controlled when the caller owns the handle. Under ADR 0032 the control
  // cluster's circular icon IS the menu handle (control-language D5), so the
  // drawer's trigger lives there and this component renders the drawer only —
  // which is also how §15's C7 duplication ("one of them should go") is
  // resolved: the cluster's icon stays, this component's own button does not
  // render beside it.
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const controlled = controlledOpen !== undefined;
  const open = controlled ? controlledOpen : uncontrolledOpen;
  const setOpen = (next: boolean): void => {
    if (controlled) onOpenChange?.(next);
    else setUncontrolledOpen(next);
  };
  // Concede arms a confirm step; it disarms whenever the drawer closes.
  const [confirming, setConfirming] = useState(false);

  const close = (): void => {
    setOpen(false);
    setConfirming(false);
  };

  // Escape closes the drawer (keyboard parity with the scrim click).
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent): void => {
      // Not `close()`: it closes over `setOpen`, which is rebuilt every render
      // now that the drawer can be controlled, and depending on it would
      // re-bind this listener on every render for no behavioural gain.
      if (event.key !== 'Escape') return;
      if (controlled) onOpenChange?.(false);
      else setUncontrolledOpen(false);
      setConfirming(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, controlled, onOpenChange]);

  return (
    <div className={s.gameMenu}>
      {!controlled && (
        <button
          type="button"
          className={s.menuButton}
          aria-label="Game menu"
          aria-expanded={open}
          data-testid="game-menu-button"
          onClick={() => (open ? close() : setOpen(true))}
        >
          ☰
        </button>
      )}
      {open && (
        <>
          {/* Click-away scrim: closes without acting. Sits under the drawer. */}
          <button
            type="button"
            className={s.menuScrim}
            aria-label="Close menu"
            data-testid="game-menu-scrim"
            onClick={close}
          />
          <div className={s.menuDrawer} role="menu" data-testid="game-menu">
            <button
              type="button"
              role="menuitem"
              className={s.menuItem}
              data-testid="menu-shortcuts"
              onClick={() => {
                close();
                onShowShortcuts();
              }}
            >
              Keyboard shortcuts
            </button>
            {onShowSettings && (
              <button
                type="button"
                role="menuitem"
                className={s.menuItem}
                data-testid="menu-settings"
                onClick={() => {
                  close();
                  onShowSettings();
                }}
              >
                Display settings
              </button>
            )}
            {onShowArtSettings && (
              <button
                type="button"
                role="menuitem"
                className={s.menuItem}
                data-testid="menu-card-art"
                onClick={() => {
                  close();
                  onShowArtSettings();
                }}
              >
                Card art
              </button>
            )}
            {concede &&
              (confirming ? (
                <div className={s.menuConfirm} data-testid="menu-concede-confirm">
                  <span className={s.menuConfirmLabel}>Concede the game?</span>
                  <button
                    type="button"
                    className={cx(s.menuItem, s.menuItemDanger)}
                    data-testid="menu-concede-yes"
                    onClick={() => {
                      close();
                      onChoose(concede);
                    }}
                  >
                    Concede
                  </button>
                  <button
                    type="button"
                    className={s.menuItem}
                    data-testid="menu-concede-no"
                    onClick={() => setConfirming(false)}
                  >
                    Keep playing
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  className={cx(s.menuItem, s.menuItemDanger)}
                  data-testid="menu-concede"
                  onClick={() => setConfirming(true)}
                >
                  {/* A server action label, so it goes through the one symbol
                      vocabulary like every other (issue #462). */}
                  <SymbolText text={concede.label} />
                </button>
              ))}
          </div>
        </>
      )}
    </div>
  );
}
