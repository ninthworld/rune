/**
 * The front-door landing screen (issue #103; identity redesign #300; front-door
 * screens per `docs/design/ui-blueprint.md` open item 1).
 *
 * This is the only UI shown before the first {@link LobbyView} arrives. It is the
 * product's front door, not an IP-entry form: the brand lockup leads, one gold
 * **Play** affordance connects, and the server address is a default (from
 * `VITE_RUNE_SERVER_URL`) tucked behind a "Server settings" disclosure — an
 * advanced affordance, present for anyone pointing at a different server but never
 * the first thing a player must read.
 *
 * It drives the store's connection lifecycle directly and always keeps a possible
 * user input on screen so the app is never in a dead state:
 *
 * - `idle`   → Play (connects to the resolved address) + the settings disclosure.
 * - `connecting` → a connecting pulse + Cancel (aborts via `disconnect`).
 * - `closed` → a disconnected notice + Retry, with the settings disclosure opened
 *   so the address is right there to fix.
 *
 * The three states stay visually distinct via a colored status pill (idle / a live
 * "connecting" pulse / a "disconnected" alert) beneath the brand lockup, so
 * pre-game reads as the same product as the table (docs/design/ui-design-notes.md
 * §Identity). Identity is procedural geometry only — the {@link RuneMark} and the
 * display-face wordmark; no card image, official frame, symbol, or WotC branding.
 *
 * The "connected, waiting for first state" case (`status === 'open'`, no view yet)
 * is owned by {@link LobbyScreen}'s fallback, not here — App switches as soon as
 * the socket is open. This component holds no game state; it is chrome only.
 *
 * We connect with `autoReconnect: false`: this is a manual, user-driven screen, so
 * the UI must reflect the true socket state and let the player decide when to
 * Retry. Silent background reconnects would make the displayed status dishonest.
 */
import { useEffect, useState } from 'react';
import { useGameStore } from './store';
import { RuneMark } from './chrome/RuneMark';
import { cx } from './chrome/cx';
import s from './table/chrome.module.css';
import l from './screens.module.css';

/** Compile-time fallback when no `VITE_RUNE_SERVER_URL` is configured. */
export const DEFAULT_SERVER_URL = 'ws://localhost:9000';

/** Resolve the pre-filled server URL from the Vite env, else the fallback. */
function initialServerUrl(): string {
  return import.meta.env.VITE_RUNE_SERVER_URL ?? DEFAULT_SERVER_URL;
}

/** The RUNE brand lockup at landing scale: mark, wordmark, and tagline. */
function Brand() {
  return (
    <div className={l.brand}>
      <div className={l.brandRow}>
        <RuneMark size={56} className={l.mark} />
        <h1 className={l.wordmarkLanding}>RUNE</h1>
      </div>
      <p className={l.tagline}>Server-authoritative tabletop</p>
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
      className={l.advanced}
      open={open}
      onToggle={(event) => onToggle((event.target as HTMLDetailsElement).open)}
      data-testid="server-settings"
    >
      <summary className={l.advancedSummary}>Server settings</summary>
      <label className={s.field}>
        <span className={s.fieldLabel}>Server address</span>
        <input
          className={s.input}
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
      <span className={s.muted}>Play connects to this address.</span>
    </details>
  );
}

export function ConnectionScreen() {
  const status = useGameStore((state) => state.status);
  const connect = useGameStore((state) => state.connect);
  const disconnect = useGameStore((state) => state.disconnect);
  const [url, setUrl] = useState(initialServerUrl);
  const [settingsOpen, setSettingsOpen] = useState(false);

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
      <main className={l.screen}>
        <div className={l.motif} aria-hidden="true">
          <RuneMark size={520} />
        </div>
        <section className={l.landing} aria-label="Connecting" data-testid="connection-screen">
          <Brand />
          <span className={cx(l.state, l.stateConnecting)}>
            <span className={cx(l.dot, l.dotLive)} />
            Connecting
          </span>
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

  // `idle` and `closed` share the Play landing; only the framing differs. There
  // is no distinct 'error' status — an errored socket surfaces as a close, so we
  // treat `closed` as the retryable error/closed state (see store.ts).
  return (
    <main className={l.screen}>
      <div className={l.motif} aria-hidden="true">
        <RuneMark size={520} />
      </div>
      <section className={l.landing} aria-label="RUNE" data-testid="connection-screen">
        <Brand />
        {isClosed ? (
          <>
            <span className={cx(l.state, l.stateClosed)}>
              <span className={l.dot} />
              Disconnected
            </span>
            <span className={s.errorText} data-testid="connection-status" role="alert">
              Connection closed. Check the server address and try again.
            </span>
          </>
        ) : (
          <span className={cx(l.state, l.stateIdle)} data-testid="connection-status">
            <span className={l.dot} />
            Ready to play
          </span>
        )}
        <button type="button" className={l.play} onClick={attempt} data-testid="connect-button">
          {isClosed ? 'Retry' : 'Play'}
        </button>
        <ServerSettings
          url={url}
          open={settingsOpen}
          onToggle={setSettingsOpen}
          onChange={setUrl}
          onSubmit={attempt}
        />
      </section>
    </main>
  );
}
