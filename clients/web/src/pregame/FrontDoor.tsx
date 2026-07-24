/**
 * The front door (issue #506; `front-door-and-lobby.md` §5.1) — the Play-first
 * landing, now a content column on the shared pregame stage rather than a carved
 * panel over the chrome vignette.
 *
 * Behavior is carried from the shipped `ConnectionScreen` verbatim; only the
 * dressing and one copy state changed:
 *
 * - `idle` → Play (connects to the resolved address) + the settings disclosure.
 * - `connecting` → a connecting pulse + Cancel (aborts via `disconnect`).
 * - `closed` → a disconnected notice + Retry, with the disclosure auto-opened so
 *   the address is right there to fix.
 * - **Reclaiming (P11)** — when `restoreSession()` is replaying a stored session
 *   token for this address, the connecting state reads *Reclaiming your seat*
 *   instead of *Opening a connection*, keeping the connecting treatment. Purely
 *   a copy/state change: the socket lifecycle is untouched.
 *
 * The three states stay visually distinct through the status pill, and each
 * keeps an interactive control on screen — never a dead screen. Identity is
 * procedural geometry only (the `RuneMark` and the display-face wordmark); no
 * card image, official frame, symbol, or WotC branding.
 *
 * We connect with `autoReconnect: false`: this is a manual, user-driven screen,
 * so the displayed status must always match the real socket.
 */
import { useEffect, useState } from 'react';
import { useGameStore } from '../store';
import { PresentationSettings } from '../table/PresentationSettings';
import { RuneMark } from '../chrome/RuneMark';
import { cx } from '../chrome/cx';
import p from './styles';

/** Compile-time fallback when no `VITE_RUNE_SERVER_URL` is configured. */
export const DEFAULT_SERVER_URL = 'ws://localhost:9000';

/** Resolve the pre-filled server URL from the Vite env, else the fallback. */
function initialServerUrl(): string {
  return import.meta.env.VITE_RUNE_SERVER_URL ?? DEFAULT_SERVER_URL;
}

/** The RUNE brand lockup at landing scale: mark, wordmark, and tagline. */
function Brand() {
  return (
    <div className={p.brand}>
      <div className={p.brandRow}>
        <RuneMark size={56} className={p.mark} />
        <h1 className={p.wordmark} data-place-heading tabIndex={-1}>
          RUNE
        </h1>
      </div>
      <p className={p.tagline}>Server-authoritative tabletop</p>
    </div>
  );
}

/**
 * The "Server settings" disclosure: the address input as an advanced affordance.
 * Controlled open state so a failed connection can open it (the address is the
 * likely fix); a user toggle stays in charge afterwards via `onToggle`.
 */
function ServerSettings({
  url,
  open,
  onToggle,
  onChange,
  onSubmit,
}: {
  url: string;
  open: boolean;
  onToggle: (open: boolean) => void;
  onChange: (url: string) => void;
  onSubmit: () => void;
}) {
  return (
    <details
      className={p.advanced}
      open={open}
      onToggle={(event) => onToggle((event.target as HTMLDetailsElement).open)}
      data-testid="server-settings"
    >
      <summary className={p.advancedSummary}>Server settings</summary>
      <label className={p.field}>
        <span className={p.fieldLabel}>Server address</span>
        <input
          className={p.input}
          type="text"
          inputMode="url"
          autoComplete="off"
          spellCheck={false}
          value={url}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') onSubmit();
          }}
          data-testid="server-url"
          aria-label="Server address"
        />
      </label>
      <span className={p.muted}>Play connects to this address.</span>
    </details>
  );
}

export function FrontDoor() {
  const status = useGameStore((state) => state.status);
  const reclaiming = useGameStore((state) => state.reclaimingSession);
  const connect = useGameStore((state) => state.connect);
  const disconnect = useGameStore((state) => state.disconnect);
  const [url, setUrl] = useState(initialServerUrl);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showDisplaySettings, setShowDisplaySettings] = useState(false);

  // A failed connection opens Server settings: the address is the likely fix,
  // so it should be on screen next to the Retry (never a dead end).
  const isClosed = status === 'closed';
  useEffect(() => {
    if (isClosed) setSettingsOpen(true);
  }, [isClosed]);

  // Manual, user-driven flow: Play/Retry is the only connect path, so the
  // displayed status always matches the real socket (see file header).
  const attempt = (): void => {
    const target = url.trim();
    if (target.length === 0) return;
    connect(target, { autoReconnect: false });
  };

  if (status === 'connecting') {
    return (
      <div className={p.frontDoor}>
        <section
          className={p.frontDoorColumn}
          aria-label="Connecting"
          data-testid="connection-screen"
        >
          <Brand />
          <span className={cx(p.statePill, p.stateConnecting)}>
            <span className={cx(p.dot, p.dotLive)} />
            {reclaiming ? 'Reclaiming' : 'Connecting'}
          </span>
          <span className={p.muted} data-testid="connection-status">
            {reclaiming ? `Reclaiming your seat at ${url}` : `Opening a connection to ${url}`}
          </span>
          <div className={p.buttonRow}>
            <button type="button" className={p.button} onClick={disconnect}>
              Cancel
            </button>
          </div>
        </section>
      </div>
    );
  }

  // `idle` and `closed` share the Play landing; only the framing differs. There
  // is no distinct 'error' status — an errored socket surfaces as a close, so we
  // treat `closed` as the retryable error/closed state (see store.ts).
  return (
    <div className={p.frontDoor}>
      <section className={p.frontDoorColumn} aria-label="RUNE" data-testid="connection-screen">
        <Brand />
        {isClosed ? (
          <>
            <span className={cx(p.statePill, p.stateClosed)}>
              <span className={p.dot} />
              Disconnected
            </span>
            <span className={p.error} data-testid="connection-status" role="alert">
              Connection closed. Check the server address and try again.
            </span>
          </>
        ) : (
          <span className={cx(p.statePill, p.stateIdle)} data-testid="connection-status">
            <span className={p.dot} />
            Ready to play
          </span>
        )}
        {/* The only gold on this place (§4.4). */}
        <button
          type="button"
          className={p.gold}
          data-gold="true"
          onClick={attempt}
          data-testid="connect-button"
        >
          {isClosed ? 'Retry' : 'Play'}
        </button>
        <ServerSettings
          url={url}
          open={settingsOpen}
          onToggle={setSettingsOpen}
          onChange={setUrl}
          onSubmit={attempt}
        />
        <button
          type="button"
          className={p.quiet}
          data-testid="front-door-settings"
          onClick={() => setShowDisplaySettings(true)}
        >
          Display settings
        </button>
      </section>
      {showDisplaySettings && (
        <PresentationSettings onClose={() => setShowDisplaySettings(false)} />
      )}
    </div>
  );
}
