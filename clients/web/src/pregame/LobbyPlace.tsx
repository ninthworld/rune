/**
 * The connected server lobby, and the pregame content router (issue #546; the
 * approved `docs/ui-concepts/rune-pregame-server-lobby.jpg` baseline).
 *
 * `LobbyContent` is what the shared stage renders once the socket is open: the
 * skeleton lobby before the first `LobbyView`, the Lobby place while room-less,
 * and the Room place once `lobby.room` is set. Which one shows is derived from
 * the store, never stored — the flow is reconstructable from one message plus
 * the socket status.
 *
 * The lobby itself is the baseline's arrangement, and its whole hierarchy is one
 * sentence: **open games are the focus, and selecting a table is what makes Join
 * the primary action.**
 *
 * - Arena — Open Games, the selectable tables, and beneath them the one blue
 *   primary for the current selection (Join a gathering table, Watch a running
 *   one) with Create Game as a prominent secondary *destination*.
 * - The create setup is that destination, not an embedded form: pressing Create
 *   Game replaces the arena's contents on the same stage, over the same
 *   environment, and Cancel comes straight back.
 * - Joining by id — the other half of the room's share line — is a collapsed
 *   disclosure, not a competing panel.
 * - Edges — the server plaque and its Switch top-centre, the identity strip
 *   under the wordmark, Decks bottom-left, chat collapsed on the right edge, and
 *   the #505 settings handle bottom-right.
 *
 * Before the first frame the same composition renders with a skeleton list and a
 * working way off the server: the shape a player is waiting for is the shape
 * they see, and there is never a dead screen.
 */
import { useEffect, useState } from 'react';
import { cx } from '../chrome/cx';
import {
  joinRoomCommand,
  setNameCommand,
  spectateRoomCommand,
  submitDeckCommand,
  type LobbyCommand,
  type LobbyView,
  type RoomSummary,
} from '../protocol';
import { useGameStore } from '../store';
import { ControlButton } from '../table/controls';
import { DeckBuilder } from '../DeckBuilder';
import { STARTER_DECKLISTS, decklistCounts } from '../decklists';
import { CreateGame, type CreatePrefill } from './CreateGame';
import { serverLabel } from './serverIdentity';
import { LastMatchRibbon } from './LastMatchRibbon';
import { ChatEdge, MenuFrame, ServerPlaque, SessionMenu } from './MenuFrame';
import { RoomDirectory } from './RoomDirectory';
import { RoomPlace } from './RoomPlace';
import { CrestChip } from './SeatPlaque';
import { GAME_SETUPS } from './gameSetups';
import { seatMonogram } from './seatIdentity';
import p from './styles';

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/**
 * The identity strip (issue #294), behaviour unchanged: offered only when the
 * server advertises `set_name`; with a name set but no `set_name` offered it
 * stays a read-only line. The input seeds from the server's current `name` — the
 * one load-bearing value — while what is being typed is ephemeral local form
 * state, re-seeded to server truth on change.
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

  // The local seat, when there is one: room-less there is no seat and therefore
  // no deterministic accent yet, so the chip renders unaccented.
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
          <span className={p.fit}>
            <ControlButton
              variant="utility"
              label={current.length > 0 ? 'Save' : 'Set name'}
              onPress={save}
              testId="set-name-button"
            />
          </span>
        </>
      ) : (
        <>
          <span className={p.muted}>Playing as</span>
          <span className={p.identityName} data-testid="display-name-current">
            {current}
          </span>
          {canSet && (
            <span className={p.fit}>
              <ControlButton
                variant="utility"
                label="Change"
                accessibleName="Change your display name"
                onPress={() => setEditing(true)}
                testId="change-name-button"
              />
            </span>
          )}
        </>
      )}
    </div>
  );
}

/** The join-by-id disclosure — the other half of the room's share line. */
function JoinById({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const [roomId, setRoomId] = useState('');
  const [joinError, setJoinError] = useState<string | null>(null);

  if (!can(view, 'join_room')) return null;

  const join = (): void => {
    const target = roomId.trim();
    if (target.length === 0) {
      setJoinError('Enter a room id to join.');
      return;
    }
    setJoinError(null);
    sendLobby(joinRoomCommand(target));
  };

  return (
    <details className={p.disclosure} data-testid="join-room">
      <summary className={p.disclosureSummary}>Join with an id</summary>
      <div className={p.disclosureBody}>
        <label className={p.seatOptionsField}>
          {/* The wording matches the room's share line word for word, so "send
              this id to a friend" and "paste the id you were sent" are visibly
              the same instruction. */}
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
        <span className={p.wide}>
          <ControlButton
            variant="secondary"
            label="Join room"
            onPress={join}
            testId="join-room-button"
          />
        </span>
      </div>
    </details>
  );
}

/**
 * The blue primary the current selection earns, or `undefined` for none.
 *
 * `valid_commands` remains the only source of interactivity: Join renders
 * because the server advertises `join_room`, Watch because it advertises
 * `spectate_room`. Selection only decides *which* of the advertised commands is
 * the one on offer — it never creates one.
 */
function selectionPrimary(
  view: LobbyView,
  room: RoomSummary | undefined,
  sendLobby: (command: LobbyCommand) => void,
): { label: string; testId: string; press: () => void } | undefined {
  if (room === undefined) return undefined;
  if (room.state === 'in_progress') {
    if (!can(view, 'spectate_room')) return undefined;
    return {
      label: 'Watch Game',
      testId: 'spectate-selected-button',
      press: () => sendLobby(spectateRoomCommand(room.room_id)),
    };
  }
  if (!can(view, 'join_room') || room.filled >= room.config.seats) return undefined;
  return {
    label: 'Join Game',
    testId: 'join-selected-button',
    press: () => sendLobby(joinRoomCommand(room.room_id)),
  };
}

/**
 * The Lobby place. `view` is `undefined` before the first frame, which renders
 * the same composition with a skeleton list.
 */
export function LobbyPlace({ view }: { view?: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const disconnect = useGameStore((state) => state.disconnect);
  const catalog = useGameStore((state) => state.catalog);
  const requestCatalog = useGameStore((state) => state.requestCatalog);
  const lobbyError = useGameStore((state) => state.lobbyError);
  const lastMatch = useGameStore((state) => state.lastMatch);
  const dismissLastMatch = useGameStore((state) => state.dismissLastMatch);
  const serverUrl = useGameStore((state) => state.serverUrl);
  const [selectedId, setSelectedId] = useState<string | undefined>(undefined);
  const [prefill, setPrefill] = useState<CreatePrefill | null>(null);
  const [creating, setCreating] = useState(false);
  const [builderOpen, setBuilderOpen] = useState(false);

  const canCreate = view !== undefined && can(view, 'create_room');
  // The selection is ephemeral and must survive the directory changing under it:
  // it is resolved against the live view every frame, never stored as a room.
  const selected = view?.directory.find((room) => room.room_id === selectedId);
  const primary = view !== undefined ? selectionPrimary(view, selected, sendLobby) : undefined;
  // With no directory to choose from, Create is the only way forward and takes
  // the blue; with tables on screen it is the secondary destination (§4.1 —
  // exactly one blue control per state, either way).
  const createIsPrimary = canCreate && (view?.directory.length ?? 0) === 0;

  const openCreate = (setup?: { setupId: string; seats?: number }): void => {
    if (setup !== undefined) {
      setPrefill((previous) => ({ ...setup, nonce: (previous?.nonce ?? 0) + 1 }));
    }
    setCreating(true);
  };

  const openBuilder = (): void => {
    if (catalog === null) requestCatalog();
    setBuilderOpen(true);
  };

  return (
    <MenuFrame
      label="Lobby"
      testId={view === undefined ? 'lobby-waiting' : 'lobby-screen'}
      topStart={view !== undefined ? <IdentityRow view={view} /> : undefined}
      top={
        <ServerPlaque
          name={serverLabel(serverUrl)}
          action={
            <span className={p.fit}>
              <ControlButton
                variant="utility"
                label="Switch"
                accessibleName="Leave this server and choose another"
                onPress={disconnect}
                testId="lobby-disconnect-button"
              />
            </span>
          }
        />
      }
      edge={<ChatEdge />}
      footStart={
        <span className={p.fit}>
          <ControlButton
            variant="utility"
            label="Decks"
            accessibleName="Open the deck builder"
            onPress={openBuilder}
            testId="open-deck-builder-button"
          />
        </span>
      }
      footEnd={<SessionMenu onDisconnect={disconnect} />}
    >
      {lobbyError !== null && (
        <span className={cx(p.error, p.rejected)} role="alert" data-testid="lobby-error">
          {lobbyError}
        </span>
      )}

      {/* The last-match ribbon. Ephemeral: everything else renders identically,
          and stays fully functional, with it absent. */}
      {lastMatch !== null && (
        <LastMatchRibbon
          summary={lastMatch}
          onPlayAgain={() => {
            openCreate({
              setupId: lastMatch.gameSetup ?? GAME_SETUPS[0]!.id,
              seats: lastMatch.seats,
            });
            dismissLastMatch();
          }}
          onDismiss={dismissLastMatch}
        />
      )}

      {creating && canCreate ? (
        <CreateGame prefill={prefill} onCancel={() => setCreating(false)} />
      ) : (
        <>
          <RoomDirectory view={view} selectedId={selectedId} onSelect={setSelectedId} />
          <div className={p.controlColumn}>
            {primary !== undefined && (
              <ControlButton
                variant="primary"
                label={primary.label}
                onPress={primary.press}
                testId={primary.testId}
              />
            )}
            {canCreate && (
              <span className={createIsPrimary ? p.fit : p.wide}>
                <ControlButton
                  variant={createIsPrimary ? 'primaryCompact' : 'secondary'}
                  label="Create Game"
                  onPress={() => openCreate()}
                  testId="open-create-game-button"
                />
              </span>
            )}
            {view !== undefined && <JoinById view={view} />}
          </div>
        </>
      )}

      {builderOpen && (
        <DeckBuilder
          catalog={catalog}
          initialCounts={decklistCounts(STARTER_DECKLISTS[0]!)}
          error={lobbyError}
          // From the lobby the builder is a LIBRARY: a player who has not taken
          // a seat cannot `submit_deck`, and the server says so by leaving the
          // command out of `valid_commands` — it answers `NotSeated`. Offering
          // Submit here sent a command the server never advertised, which is the
          // one thing the client is not allowed to decide for itself. The gate is
          // `valid_commands` rather than "are we in a room", so the control
          // follows the server if a future setup ever seats a player earlier.
          onSubmit={
            view !== undefined && can(view, 'submit_deck')
              ? (cards, commander) => sendLobby(submitDeckCommand(cards, commander))
              : undefined
          }
          onClose={() => setBuilderOpen(false)}
        />
      )}
    </MenuFrame>
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
