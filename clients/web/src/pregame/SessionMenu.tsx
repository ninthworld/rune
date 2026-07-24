/**
 * The pregame session menu (issue #506 / #505; `front-door-and-lobby.md` §6).
 *
 * Present in the **Lobby** and the **Room**, closing P7's gap: from the moment
 * the socket opens until the game starts, display settings, card-art settings,
 * and Disconnect were unreachable. This is a chrome menu of **client-session
 * actions only** — it never holds a lobby command, so it introduces no
 * interactivity that `valid_commands` did not advertise.
 *
 * Menu and overlay state are ephemeral presentation, never load-bearing across
 * messages.
 */
import { useEffect, useState } from 'react';
import { ArtSettings } from '../table/ArtSettings';
import { PresentationSettings } from '../table/PresentationSettings';
import p from './styles';

export interface SessionMenuProps {
  /** Close the socket and return to the front door (a client-session action). */
  onDisconnect: () => void;
}

export function SessionMenu({ onDisconnect }: SessionMenuProps) {
  const [open, setOpen] = useState(false);
  const [showDisplay, setShowDisplay] = useState(false);
  const [showArt, setShowArt] = useState(false);

  // Escape closes the drawer (keyboard parity with the click-away scrim).
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  return (
    <div className={p.menu}>
      <button
        type="button"
        className={p.button}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((wasOpen) => !wasOpen)}
        data-testid="session-menu-button"
      >
        Menu
      </button>
      {open && (
        <>
          <button
            type="button"
            className={p.menuScrim}
            aria-label="Close menu"
            onClick={() => setOpen(false)}
            data-testid="session-menu-scrim"
          />
          <div className={p.menuDrawer} role="menu" data-testid="session-menu">
            <button
              type="button"
              role="menuitem"
              className={p.menuItem}
              onClick={() => {
                setOpen(false);
                setShowDisplay(true);
              }}
              data-testid="session-menu-display-settings"
            >
              Display settings
            </button>
            <button
              type="button"
              role="menuitem"
              className={p.menuItem}
              onClick={() => {
                setOpen(false);
                setShowArt(true);
              }}
              data-testid="session-menu-card-art"
            >
              Card art settings
            </button>
            <button
              type="button"
              role="menuitem"
              className={p.menuItem}
              onClick={() => {
                setOpen(false);
                onDisconnect();
              }}
              data-testid="session-menu-disconnect"
            >
              Disconnect
            </button>
          </div>
        </>
      )}
      {showDisplay && <PresentationSettings onClose={() => setShowDisplay(false)} />}
      {showArt && <ArtSettings onClose={() => setShowArt(false)} />}
    </div>
  );
}
