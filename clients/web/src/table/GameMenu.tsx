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
import s from './chrome.module.css';

interface Props {
  /** The server-offered concede action, if any (`type: "concede"`). */
  concede?: ValidAction;
  /** Submit a chosen action (the store reads its id + token). */
  onChoose: (action: ValidAction) => void;
  /** Open the keyboard-shortcut reference overlay (issue #266). */
  onShowShortcuts: () => void;
}

export function GameMenu({ concede, onChoose, onShowShortcuts }: Props) {
  const [open, setOpen] = useState(false);
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
      if (event.key === 'Escape') close();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  return (
    <div className={s.gameMenu}>
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
                  {concede.label}
                </button>
              ))}
          </div>
        </>
      )}
    </div>
  );
}
