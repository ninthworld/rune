/**
 * The front door — the server-connection place (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-server-connection.jpg` baseline).
 *
 * The baseline is the product's whole thesis in one screen: RUNE is a dumb
 * client, so the first thing a player sees is the wordmark, the server they are
 * already pointed at, and **one blue action**. Changing server is a quiet word
 * underneath it, and the device-local settings handle sits at the corner. There
 * is no form to fill in before playing.
 *
 * Behaviour is carried from the shipped screen verbatim — only the dressing and
 * the primary's word changed:
 *
 * - `idle` → Connect (to the resolved address) + the change-server disclosure.
 * - `connecting` → a connecting line + Cancel (aborts via `disconnect`).
 * - `closed` → a disconnected alert + Retry, with the disclosure auto-opened so
 *   the address is right there to fix.
 * - **Reclaiming (P11)** — while `restoreSession()` replays a stored token for
 *   this address the connecting line reads *Reclaiming your seat*. Purely a copy
 *   change: the socket lifecycle is untouched.
 *
 * Each state keeps an interactive control on screen — never a dead screen — and
 * identity stays procedural geometry only (`RuneMark` plus the display-face
 * wordmark); no card image, official frame, symbol, or WotC branding.
 *
 * We connect with `autoReconnect: false`: this is a manual, user-driven screen,
 * so the displayed status must always match the real socket.
 */
import { useEffect, useState } from 'react';
import { useGameStore } from '../store';
import { RuneMark } from '../chrome/RuneMark';
import { ControlButton } from '../table/controls';
import { MenuFrame, Plaque, SessionMenu } from './MenuFrame';
import { initialServerUrl, serverLabel } from './serverIdentity';
import p from './styles';

export function FrontDoor() {
  const status = useGameStore((state) => state.status);
  const reclaiming = useGameStore((state) => state.reclaimingSession);
  // A pending last-match record while connecting means #452's postgame exit is
  // in flight: it gives up the seat and REOPENS the server, so the lobby landing
  // is reached across a reconnect rather than handed off in-session. Derived,
  // not stored: no record, no claim.
  const returning = useGameStore((state) => state.lastMatch !== null);
  const connect = useGameStore((state) => state.connect);
  const disconnect = useGameStore((state) => state.disconnect);
  const [url, setUrl] = useState(initialServerUrl);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // A failed connection opens the change-server disclosure: the address is the
  // likely fix, so it belongs on screen next to Retry (never a dead end).
  const isClosed = status === 'closed';
  useEffect(() => {
    if (isClosed) setSettingsOpen(true);
  }, [isClosed]);

  // Manual, user-driven flow: Connect/Retry is the only connect path, so the
  // displayed status always matches the real socket (see the file header).
  const attempt = (): void => {
    const target = url.trim();
    if (target.length === 0) return;
    connect(target, { autoReconnect: false });
  };

  const connecting = status === 'connecting';

  return (
    <MenuFrame
      label="RUNE"
      testId="connection-screen"
      lockup={false}
      footEnd={<SessionMenu testId="front-door-settings" />}
    >
      <div className={p.doorColumn}>
        <RuneMark size={64} className={p.doorMark} />
        <h1 className={p.doorWordmark} data-place-heading tabIndex={-1}>
          RUNE
        </h1>

        <Plaque testId="server-plaque">
          <span className={p.serverGem} aria-hidden="true" />
          <span className={p.serverName} data-testid="server-name">
            {serverLabel(url)}
          </span>
        </Plaque>

        {connecting ? (
          <>
            <span className={p.doorStatus} data-testid="connection-status">
              {reclaiming
                ? `Reclaiming your seat at ${url}`
                : returning
                  ? 'Returning to the lobby…'
                  : `Opening a connection to ${url}`}
            </span>
            <ControlButton variant="cancel" label="Cancel" onPress={disconnect} />
          </>
        ) : (
          <>
            {isClosed ? (
              <span className={p.error} data-testid="connection-status" role="alert">
                Connection closed. Check the server address and try again.
              </span>
            ) : (
              <span className={p.doorStatus} data-testid="connection-status">
                Ready to play
              </span>
            )}
            {/* The one blue primary of this state (§4.1). */}
            <ControlButton
              variant="primary"
              label={isClosed ? 'Retry' : 'Connect'}
              onPress={attempt}
              testId="connect-button"
            />
            <details
              className={p.disclosure}
              open={settingsOpen}
              onToggle={(event) => setSettingsOpen((event.target as HTMLDetailsElement).open)}
              data-testid="server-settings"
            >
              <summary className={p.disclosureSummary}>Change server</summary>
              <div className={p.disclosureBody}>
                <label className={p.seatOptionsField}>
                  <span className={p.fieldLabel}>Server address</span>
                  <input
                    className={p.input}
                    type="text"
                    inputMode="url"
                    autoComplete="off"
                    spellCheck={false}
                    value={url}
                    onChange={(event) => setUrl(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') attempt();
                    }}
                    data-testid="server-url"
                    aria-label="Server address"
                  />
                </label>
                <span className={p.muted}>Connect uses this address.</span>
              </div>
            </details>
          </>
        )}
      </div>
    </MenuFrame>
  );
}
