/**
 * The **menu frame** — the anatomy every pregame place is composed on (issue
 * #546, under [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * The four approved baselines (`docs/ui-concepts/rune-pregame-*.jpg`) draw one
 * picture four times: a large open illustrated arena, and small controls anchored
 * to its edges — the wordmark top-left, the server plaque top-centre, the deck
 * destination bottom-left, the step-back control bottom-centre, the settings
 * handle bottom-right, chat collapsed on the right edge. That is the same
 * anatomy ADR 0032 gave the match, which is the point: between matches the
 * player should still be standing in the game's world, not in an application
 * shell that happens to launch it.
 *
 * This module owns that frame and the surfaces that sit on its edges. It owns no
 * lobby behaviour at all: every slot is filled by the place that mounts it, and
 * every *control* rendered anywhere in the pregame flow is a `ControlButton` or
 * `IconButton` from `table/controls` — the `control-language.md` §3 family,
 * imported rather than re-drawn. There is deliberately no second button here.
 */
import { useEffect, useState, type CSSProperties, type ReactNode } from 'react';
import { cx } from '../chrome/cx';
import { RuneMark } from '../chrome/RuneMark';
import { IconButton } from '../table/controls';
import { ArtSettings } from '../table/ArtSettings';
import { PresentationSettings } from '../table/PresentationSettings';
import p from './styles';

/**
 * The one non-button surface these screens draw: a chamfered slate plate inside
 * a narrow warm frame.
 *
 * It is the same two-clipped-box construction as the control family's plate and
 * the phase plaque's (`control-language.md` §3.1 lists both plaques as the
 * family's non-button surfaces), because a menu plaque that is built differently
 * from a match plaque *is* a second visual system, however similar it looks on
 * the day it ships.
 */
export interface PlaqueProps {
  children: ReactNode;
  /** Draws the blue rim + bloom. Callers must also carry a non-colour channel. */
  selected?: boolean;
  /** Extra class on the drawn face (a place's own padding/direction). */
  faceClass?: string;
  /** Extra class on the frame box (a place's own width). */
  className?: string;
  /** Inline custom properties — the seat accent, the ring position. */
  style?: CSSProperties;
  testId?: string;
}

export function Plaque({ children, selected, faceClass, className, style, testId }: PlaqueProps) {
  return (
    <div
      className={cx(p.plaque, selected && p.plaqueOn, className)}
      style={style}
      data-testid={testId}
    >
      <div className={cx(p.plaqueFace, faceClass)}>{children}</div>
    </div>
  );
}

/** The corner lockup: the procedural mark plus the display-face wordmark. */
export function Lockup({ heading = false }: { heading?: boolean }) {
  return (
    <div className={p.lockup}>
      <RuneMark size={26} />
      {heading ? (
        <h1 className={p.lockupWord}>RUNE</h1>
      ) : (
        <span className={p.lockupWord} aria-hidden="true">
          RUNE
        </span>
      )}
    </div>
  );
}

/**
 * The server plaque. The default server is already chosen — RUNE is a dumb
 * client and an ordinary player configures nothing — so this states which server
 * they are on and, where one is offered, carries the quiet way off it.
 */
export function ServerPlaque({ name, action }: { name: string; action?: ReactNode }) {
  return (
    <Plaque testId="server-plaque">
      <span className={p.serverGem} aria-hidden="true" />
      <span className={p.serverName} data-testid="server-name">
        {name}
      </span>
      {action}
    </Plaque>
  );
}

/**
 * Chat, as the baselines draw it: a collapsed tab on the right edge with room to
 * become functional later.
 *
 * **It carries no messages, because the protocol has none.** There is no chat
 * command, no chat frame, and no server-side room chat (`protocol/lobby.ts`), so
 * the expanded surface says exactly that rather than presenting an input that
 * would silently drop what a player typed. Nothing here is load-bearing and the
 * arena is identical with the tab collapsed, which is the state it starts in.
 */
export function ChatEdge() {
  const [open, setOpen] = useState(false);
  return (
    <div className={p.chatTab}>
      <div className={p.fit}>
        <IconButton
          glyph="✉"
          label={open ? 'Hide chat' : 'Show chat'}
          onPress={() => setOpen((wasOpen) => !wasOpen)}
          expanded={open}
          controls="pregame-chat"
          testId="chat-toggle"
        />
      </div>
      {open && (
        <div className={p.chatPanel} id="pregame-chat" data-testid="chat-panel">
          <span className={p.kicker}>Chat</span>
          <span>This server does not carry chat yet.</span>
        </div>
      )}
    </div>
  );
}

export interface SessionMenuProps {
  /**
   * Close the socket and return to the front door (a client-session action).
   * Omitted before a socket exists — the front door's own handle offers settings
   * only.
   */
  onDisconnect?: () => void;
  /** Test id for the handle; the front door keeps its shipped hook. */
  testId?: string;
}

/**
 * The settings handle (issue #505's device-local path, preserved verbatim) as
 * the bottom-right icon button every baseline draws.
 *
 * Client-session actions only — it never holds a lobby command, so it introduces
 * no interactivity `valid_commands` did not advertise. Menu and overlay state
 * are ephemeral presentation, never load-bearing across messages.
 */
export function SessionMenu({ onDisconnect, testId = 'session-menu-button' }: SessionMenuProps) {
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
      <IconButton
        glyph="⚙"
        label="Settings"
        onPress={() => setOpen((wasOpen) => !wasOpen)}
        expanded={open}
        controls="pregame-session-menu"
        testId={testId}
      />
      {open && (
        <>
          <button
            type="button"
            className={p.menuScrim}
            aria-label="Close menu"
            onClick={() => setOpen(false)}
            data-testid="session-menu-scrim"
          />
          <div
            className={p.menuDrawer}
            id="pregame-session-menu"
            role="menu"
            data-testid="session-menu"
          >
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
            {onDisconnect !== undefined && (
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
            )}
          </div>
        </>
      )}
      {showDisplay && <PresentationSettings onClose={() => setShowDisplay(false)} />}
      {showArt && <ArtSettings onClose={() => setShowArt(false)} />}
    </div>
  );
}

export interface MenuFrameProps {
  /** The arena — the place's own composition. Free to be almost nothing. */
  children: ReactNode;
  /** Accessible name for the place. */
  label: string;
  /** Test id for the place's root. */
  testId?: string;
  /** Top-left. `false` on the front door, where the wordmark is the arena. */
  lockup?: boolean;
  /** Top-centre: the server plaque, and anything stacked under it. */
  top?: ReactNode;
  /** Under the lockup: the identity strip, when there is one. */
  topStart?: ReactNode;
  /** Right edge, vertically centred: chat. */
  edge?: ReactNode;
  /** Bottom-left: the deck destination. */
  footStart?: ReactNode;
  /** Bottom-centre: the step-back control (Leave). */
  foot?: ReactNode;
  /** Bottom-right: the settings handle. Always present. */
  footEnd: ReactNode;
}

/**
 * Compose one place on the shared edge anatomy. Layout only — it derives
 * nothing, stores nothing, and every slot it does not receive simply is not
 * drawn, which is what keeps a place's chrome to what that place actually
 * offers.
 */
export function MenuFrame({
  children,
  label,
  testId,
  lockup = true,
  top,
  topStart,
  edge,
  footStart,
  foot,
  footEnd,
}: MenuFrameProps) {
  return (
    <section className={p.frame} aria-label={label} data-testid={testId}>
      {(lockup || topStart !== undefined) && (
        <div className={p.frameTopStart}>
          {lockup && <Lockup heading />}
          {topStart}
        </div>
      )}
      {top !== undefined && <div className={p.frameTop}>{top}</div>}
      <div className={p.frameArena}>{children}</div>
      {edge !== undefined && <div className={p.frameEnd}>{edge}</div>}
      {footStart !== undefined && <div className={p.frameFootStart}>{footStart}</div>}
      {foot !== undefined && <div className={p.frameFoot}>{foot}</div>}
      <div className={p.frameFootEnd}>{footEnd}</div>
    </section>
  );
}
