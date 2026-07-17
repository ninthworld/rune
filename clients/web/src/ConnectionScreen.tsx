/**
 * The pre-game connection screen (issue #103).
 *
 * This is the only UI shown before the first {@link GameView} arrives. It drives
 * the store's connection lifecycle directly — the first production caller of
 * `store.connect()` — and always keeps a possible user input on screen so the app
 * is never in a dead state:
 *
 * - `idle`   → a server-URL input (pre-filled from `VITE_RUNE_SERVER_URL`) + Connect.
 * - `connecting` → a connecting indicator + Cancel (aborts via `disconnect`).
 * - `closed` → an error/closed notice + editable URL + Retry.
 *
 * The "connected, waiting for first state" case (`status === 'open'`, no view yet)
 * is owned by {@link Table}'s fallback, not here — App renders `Table` as soon as
 * the socket is open. This component holds no game state; it is chrome only.
 *
 * We connect with `autoReconnect: false`: this is a manual, user-driven screen, so
 * the UI must reflect the true socket state and let the player decide when to
 * Retry. Silent background reconnects would make the displayed status dishonest.
 */
import { useState } from 'react';
import { useGameStore } from './store';
import s from './table/chrome.module.css';

/** Compile-time fallback when no `VITE_RUNE_SERVER_URL` is configured. */
export const DEFAULT_SERVER_URL = 'ws://localhost:9000';

/** Resolve the pre-filled server URL from the Vite env, else the fallback. */
function initialServerUrl(): string {
  return import.meta.env.VITE_RUNE_SERVER_URL ?? DEFAULT_SERVER_URL;
}

export function ConnectionScreen() {
  const status = useGameStore((state) => state.status);
  const connect = useGameStore((state) => state.connect);
  const disconnect = useGameStore((state) => state.disconnect);
  const [url, setUrl] = useState(initialServerUrl);

  // Manual, user-driven flow: the Retry button is the only reconnect path, so the
  // displayed status always matches the real socket (see file header).
  const attempt = (): void => {
    const target = url.trim();
    if (target.length === 0) return;
    connect(target, { autoReconnect: false });
  };

  if (status === 'connecting') {
    return (
      <main className={s.connectMain}>
        <section className={s.connectPanel} aria-label="Connecting" data-testid="connection-screen">
          <h1 className={s.connectHeading}>Connecting…</h1>
          <span className={s.muted} data-testid="connection-status">
            Opening a connection to {url}
          </span>
          <div className={s.buttonRow}>
            <button type="button" className={s.button} onClick={disconnect}>
              Cancel
            </button>
          </div>
        </section>
      </main>
    );
  }

  // `idle` and `closed` share the URL-entry form; only the framing differs. There
  // is no distinct 'error' status — an errored socket surfaces as a close, so we
  // treat `closed` as the retryable error/closed state (see store.ts).
  const isClosed = status === 'closed';
  return (
    <main className={s.connectMain}>
      <section
        className={s.connectPanel}
        aria-label="Connect to a server"
        data-testid="connection-screen"
      >
        <h1 className={s.connectHeading}>RUNE</h1>
        {isClosed ? (
          <span className={s.errorText} data-testid="connection-status" role="alert">
            Connection closed. Check the server address and try again.
          </span>
        ) : (
          <span className={s.muted} data-testid="connection-status">
            Enter a server address to connect.
          </span>
        )}
        <label className={s.field}>
          <span className={s.fieldLabel}>Server address</span>
          <input
            className={s.input}
            type="text"
            inputMode="url"
            autoComplete="off"
            spellCheck={false}
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            data-testid="server-url"
            aria-label="Server address"
          />
        </label>
        <div className={s.buttonRow}>
          <button type="button" className={s.button} onClick={attempt} data-testid="connect-button">
            {isClosed ? 'Retry' : 'Connect'}
          </button>
        </div>
      </section>
    </main>
  );
}
