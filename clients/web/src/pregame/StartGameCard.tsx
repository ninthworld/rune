/**
 * The Start-a-game card (issue #506; `front-door-and-lobby.md` §5.2) — the
 * Lobby place's secondary column.
 *
 * The shipped lobby stacked the directory, Create a room, and Join a friend as
 * three competing peers (P5). Here Create and Join-by-id become **one card with
 * two visible modes**, a segmented switch choosing between them, so the two
 * secondary paths stop competing and exactly one gold control is on screen.
 *
 * Both mode panels stay in the DOM — the inactive one `hidden`, so its form
 * state survives a mode flip and assistive tech is not told about a control the
 * player cannot see. Create keeps the game-type choice tiles and the segmented
 * seat picker (no dropdowns: every option is one visible press), and every
 * control clears 44 px and wraps at 125 % text.
 *
 * `valid_commands` remains the only source of interactivity: a mode is offered
 * only when the server advertises its command. Local form state (the mode, the
 * picked setup and seat count, the typed room id) is ephemeral input, never
 * load-bearing across messages.
 */
import { useEffect, useRef, useState } from 'react';
import { cx } from '../chrome/cx';
import { createRoomCommand, joinRoomCommand, type LobbyView } from '../protocol';
import { useGameStore } from '../store';
import { GAME_SETUPS, SEAT_COUNTS } from './gameSetups';
import p from './styles';

/** Which of the card's two paths is showing. */
export type StartMode = 'create' | 'join';

/**
 * An ephemeral request to show a mode and take focus — raised by the empty
 * directory's Create action and by the last-match ribbon's Play again (which
 * pre-fills the finished room's setup and seat count). The `nonce` makes a
 * repeat of the same request observable.
 */
export interface StartRequest {
  mode: StartMode;
  setupId?: string;
  seats?: number;
  nonce: number;
}

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

export function StartGameCard({
  view,
  request,
}: {
  view: LobbyView;
  request: StartRequest | null;
}) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const canCreate = can(view, 'create_room');
  const canJoin = can(view, 'join_room');

  const [mode, setMode] = useState<StartMode>(canCreate ? 'create' : 'join');
  const [setupId, setSetupId] = useState(GAME_SETUPS[0]!.id);
  const [seats, setSeats] = useState<number>(GAME_SETUPS[0]!.seats);
  const [roomId, setRoomId] = useState('');
  const [joinError, setJoinError] = useState<string | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const lastNonce = useRef(0);

  // Honor an incoming request: switch mode, seed the pre-fill, and move focus
  // to the card so the keyboard follows the eye.
  useEffect(() => {
    if (request === null || request.nonce === lastNonce.current) return;
    lastNonce.current = request.nonce;
    setMode(request.mode);
    if (request.setupId !== undefined) setSetupId(request.setupId);
    if (request.seats !== undefined) setSeats(request.seats);
    headingRef.current?.focus();
  }, [request]);

  const create = (): void => {
    sendLobby(createRoomCommand({ seats, game_setup: setupId }));
  };

  const join = (): void => {
    const target = roomId.trim();
    if (target.length === 0) {
      setJoinError('Enter a room id to join.');
      return;
    }
    setJoinError(null);
    sendLobby(joinRoomCommand(target));
  };

  // The card renders nothing when the server advertises neither path.
  if (!canCreate && !canJoin) return null;
  const active: StartMode = mode === 'create' && !canCreate ? 'join' : mode;

  return (
    <section className={p.panel} aria-label="Start a game" data-testid="start-game">
      <span className={p.kicker}>Or start your own</span>
      <h2 className={p.title} tabIndex={-1} ref={headingRef}>
        Start a game
      </h2>

      <div className={p.segmentRow} role="group" aria-label="Start a game">
        {canCreate && (
          <button
            type="button"
            className={cx(p.segment, active === 'create' && p.segmentOn)}
            aria-pressed={active === 'create'}
            onClick={() => setMode('create')}
            data-testid="start-mode-create"
          >
            Create
          </button>
        )}
        {canJoin && (
          <button
            type="button"
            className={cx(p.segment, active === 'join' && p.segmentOn)}
            aria-pressed={active === 'join'}
            onClick={() => setMode('join')}
            data-testid="start-mode-join"
          >
            Join with an id
          </button>
        )}
      </div>

      {canCreate && (
        <div hidden={active !== 'create'} data-testid="create-room">
          <div className={p.group} role="group" aria-label="Game type">
            <span className={p.fieldLabel}>Game type</span>
            <div className={p.choiceRow}>
              {GAME_SETUPS.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className={cx(p.choiceTile, option.id === setupId && p.segmentOn)}
                  aria-pressed={option.id === setupId}
                  onClick={() => {
                    setSetupId(option.id);
                    setSeats(option.seats);
                  }}
                  data-testid={`game-setup-${option.id}`}
                >
                  <span className={p.choiceName}>{option.label}</span>
                  <span className={p.choiceMeta}>{option.seats} players</span>
                </button>
              ))}
            </div>
          </div>
          <div className={p.group} role="group" aria-label="Seats">
            <span className={p.fieldLabel}>Seats</span>
            <div className={p.segmentRow}>
              {SEAT_COUNTS.map((count) => (
                <button
                  key={count}
                  type="button"
                  className={cx(p.segment, count === seats && p.segmentOn)}
                  aria-pressed={count === seats}
                  onClick={() => setSeats(count)}
                  data-testid={`seat-count-${count}`}
                >
                  {count}
                </button>
              ))}
            </div>
          </div>
          <div className={p.buttonRow}>
            <button
              type="button"
              className={p.gold}
              data-gold={active === 'create' ? 'true' : undefined}
              onClick={create}
              data-testid="create-room-button"
            >
              Create room
            </button>
          </div>
        </div>
      )}

      {canJoin && (
        <div hidden={active !== 'join'} data-testid="join-room">
          <label className={p.field}>
            {/* The wording here matches the room's share line word-for-word
                (§5.2 sharing), so "send this id to a friend" and "paste the id
                you were sent" are visibly the same instruction. */}
            <span className={p.fieldLabel}>Room id — paste the id a friend sent you</span>
            <input
              className={p.input}
              type="text"
              autoComplete="off"
              spellCheck={false}
              placeholder="Paste the id you were sent"
              value={roomId}
              onChange={(event) => setRoomId(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') join();
              }}
              data-testid="join-room-input"
              aria-label="Room id"
            />
          </label>
          {joinError !== null && (
            <span className={cx(p.error, p.rejected)} role="alert" data-testid="join-room-error">
              {joinError}
            </span>
          )}
          <div className={p.buttonRow}>
            <button
              type="button"
              className={p.gold}
              data-gold={active === 'join' ? 'true' : undefined}
              onClick={join}
              data-testid="join-room-button"
            >
              Join room
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
