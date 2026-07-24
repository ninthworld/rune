/**
 * The Lobby place and the pregame content router (issue #506;
 * `front-door-and-lobby.md` §5.2, §5.7).
 *
 * `LobbyContent` is what the shared stage renders once the socket is open: the
 * skeleton lobby before the first `LobbyView`, the Lobby place while room-less,
 * and the Room place once `lobby.room` is set. Which one shows is derived from
 * the store, never stored — the flow is reconstructable from one message plus
 * the socket status.
 *
 * The Lobby place itself:
 *
 * - **Header bar** — the compact lockup left; right, the session menu (§6) and
 *   Disconnect. Disconnect stops being the only thing in the header (P7).
 * - **Identity strip** — "Playing as <name>" with its inline editor, unchanged
 *   behavior, now wearing the local player's crest chip so the name and the
 *   color are learned together.
 * - **Left / primary** — the room directory, ahead of everything (P5).
 * - **Right / secondary** — the Start-a-game card (Create / Join in one card).
 * - **Above the directory** — the last-match ribbon when a finished game left
 *   one behind (§5.5). The lobby renders identically without it.
 *
 * Before the first frame (P8) the same composition renders with a skeleton
 * directory plus a working Disconnect — the shape a player is waiting for is the
 * shape they see, and there is never a dead screen.
 */
import { useEffect, useState } from 'react';
import { cx } from '../chrome/cx';
import { setNameCommand, type LobbyView } from '../protocol';
import { useGameStore } from '../store';
import { LastMatchRibbon } from './LastMatchRibbon';
import { PregameHeader } from './PregameHeader';
import { RoomDirectory } from './RoomDirectory';
import { RoomPlace } from './RoomPlace';
import { CrestChip } from './Roster';
import { StartGameCard, type StartRequest } from './StartGameCard';
import { GAME_SETUPS } from './gameSetups';
import { seatMonogram } from './seatIdentity';
import p from './styles';

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/**
 * The identity strip (issue #294), behavior unchanged: offered only when the
 * server advertises `set_name`; with a name set but no `set_name` offered it
 * stays a read-only line. The input seeds from the server's current `name` —
 * the one load-bearing value — while what is being typed is ephemeral local
 * form state, re-seeded to server truth on change.
 */
export function IdentityRow({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const canSet = can(view, 'set_name');
  const current = view.name ?? '';
  const [draft, setDraft] = useState(current);
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    setDraft(current);
    setEditing(false);
  }, [current]);

  const save = (): void => {
    const next = draft.trim();
    if (next.length === 0) return;
    sendLobby(setNameCommand(next));
    setEditing(false);
  };

  if (!canSet && current.length === 0) return null;
  const formOpen = canSet && (editing || current.length === 0);

  // The local seat, when there is one: room-less, no seat and therefore no
  // deterministic accent exists yet, so the chip renders unaccented.
  const mySeat = view.room?.seats.find((seat) => seat.occupied_by === view.you);

  return (
    <div className={p.identityRow} data-testid="display-name">
      <CrestChip
        monogram={
          mySeat !== undefined
            ? seatMonogram(mySeat)
            : ([...current.trim()][0] ?? '?').toUpperCase()
        }
        seat={mySeat?.seat}
        local
        testId="identity-crest"
      />
      {formOpen ? (
        <>
          <input
            className={p.identityInput}
            type="text"
            autoComplete="off"
            spellCheck={false}
            maxLength={32}
            placeholder="Your display name"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') save();
            }}
            data-testid="display-name-input"
            aria-label="Display name"
          />
          <button type="button" className={p.button} onClick={save} data-testid="set-name-button">
            {current.length > 0 ? 'Save' : 'Set name'}
          </button>
        </>
      ) : (
        <>
          <span className={p.identityLabel}>Playing as</span>
          <span className={p.identityName} data-testid="display-name-current">
            {current}
          </span>
          {canSet && (
            <button
              type="button"
              className={p.quiet}
              onClick={() => setEditing(true)}
              data-testid="change-name-button"
            >
              Change
            </button>
          )}
        </>
      )}
    </div>
  );
}

/** The lobby error, in the visual system's rejected language (§5.7). */
function LobbyError({ message }: { message: string }) {
  return (
    <span className={cx(p.error, p.rejected)} role="alert" data-testid="lobby-error">
      {message}
    </span>
  );
}

/**
 * The Lobby place: find a game or start one. `view` is `undefined` before the
 * first frame, which renders the same composition with a skeleton directory.
 */
export function LobbyPlace({ view }: { view?: LobbyView }) {
  const disconnect = useGameStore((state) => state.disconnect);
  const lobbyError = useGameStore((state) => state.lobbyError);
  const lastMatch = useGameStore((state) => state.lastMatch);
  const dismissLastMatch = useGameStore((state) => state.dismissLastMatch);
  const [request, setRequest] = useState<StartRequest | null>(null);

  const raise = (next: Omit<StartRequest, 'nonce'>): void => {
    setRequest((previous) => ({ ...next, nonce: (previous?.nonce ?? 0) + 1 }));
  };

  return (
    <div
      className={p.lobby}
      data-testid={view === undefined ? 'lobby-waiting' : 'lobby-screen'}
      aria-label="Lobby"
    >
      <div className={p.lobbyBody}>
        <PregameHeader onDisconnect={disconnect} />
        {view !== undefined && <IdentityRow view={view} />}
        {lobbyError !== null && <LobbyError message={lobbyError} />}

        {/* The last-match ribbon (§5.5). Ephemeral: everything below renders
            identically, and stays fully functional, with it absent. */}
        {lastMatch !== null && (
          <LastMatchRibbon
            summary={lastMatch}
            onPlayAgain={() => {
              raise({
                mode: 'create',
                setupId: lastMatch.gameSetup ?? GAME_SETUPS[0]!.id,
                seats: lastMatch.seats,
              });
              dismissLastMatch();
            }}
            onDismiss={dismissLastMatch}
          />
        )}

        <div className={p.columns}>
          <div className={p.column}>
            <RoomDirectory view={view} onCreateHere={() => raise({ mode: 'create' })} />
          </div>
          <div className={p.column}>
            {view !== undefined && <StartGameCard view={view} request={request} />}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * The pregame content the shared stage renders once the socket is open: the
 * skeleton lobby, the Lobby place, or the Room place. Nothing here is stored —
 * it is a read of the latest `LobbyView`.
 */
export function LobbyContent() {
  const lobby = useGameStore((state) => state.lobby);
  if (lobby === null) return <LobbyPlace />;
  if (lobby.room === undefined) return <LobbyPlace view={lobby} />;
  return <RoomPlace view={lobby} />;
}
