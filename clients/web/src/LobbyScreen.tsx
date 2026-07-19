/**
 * The pre-game lobby screen (issue #114; identity redesign #300; front-door
 * polish per `docs/design/ui-blueprint.md` open item 1).
 *
 * The screen shown between the {@link ConnectionScreen} landing and the in-game
 * {@link Table}: after the socket opens, the store greets the server (`Hello`)
 * and this screen renders the resulting {@link LobbyView} — browse the room
 * directory (the primary "find a game" path), create a room or join one by id (the
 * secondary paths), pick a bundled starter deck from the deck tiles, submit it,
 * and ready up. When every seat is filled, decked, and ready the server constructs
 * the game and pushes the first `GameView`; the app then switches to the table.
 *
 * Identity (docs/design/ui-design-notes.md §Identity, §Front door): the pre-game
 * screens share the table's visual system — the {@link RuneMark} and display-face
 * wordmark in a carved panel over the table vignette, section titles in the
 * panel-name treatment, seats wearing the same deterministic identity accents as
 * the in-game player panels ({@link IDENTITY_ACCENTS}, indexed by seat), and gold
 * reserved for the one advance-the-game action (submit deck, then ready). Players
 * read as people: display names ride the protocol (issue #294);
 * {@link seatDisplayName} reads the seat's `name` field and falls back to a
 * seat-derived "Player N" when a seat has no chosen name. Geometry only — never a
 * card image or WotC branding.
 *
 * Hard rules (AGENTS.md, ADR 0012):
 * - **Reconstruct from one `LobbyView`.** Every control here is derived from the
 *   store's latest `LobbyView`; nothing about the lobby is load-bearing across
 *   messages. Local component state is ephemeral form input only (the seat count
 *   being typed, the deck tile picked, a "Copied" flash).
 * - **`valid_commands` is the only source of interactivity.** A create/join/deck/
 *   ready/leave affordance is offered only when the server advertised that command
 *   for this connection; the client computes no legality.
 * - **No card logic.** The bundled decklists are static names/ids (see
 *   `decklists.ts`); the deck tiles' land glyphs are read off that static data for
 *   display, and the server validates a submitted deck authoritatively.
 * - **Never a dead screen.** Before the first `LobbyView`, and on every error, an
 *   interactive control (Disconnect / retry-able form) is always on screen.
 */
import { useEffect, useState, type CSSProperties } from 'react';
import {
  STARTER_DECKLISTS,
  decklistCards,
  decklistById,
  decklistSize,
  type Decklist,
} from './decklists';
import {
  createRoomCommand,
  joinRoomCommand,
  spectateRoomCommand,
  leaveCommand,
  readyCommand,
  setNameCommand,
  submitDeckCommand,
  type LobbyView,
  type RoomSummary,
  type SeatView,
} from './protocol';
import { seatDisplayName } from './playerNames';
import { useGameStore } from './store';
import { cx } from './chrome/cx';
import { RuneMark } from './chrome/RuneMark';
import { Glyph } from './chrome/glyphs';
import type { GlyphName } from './chrome/glyphs';
import { IDENTITY_ACCENTS } from './table/identityAccents';
import { PALETTE } from './tokens';
import s from './table/chrome.module.css';
import l from './screens.module.css';

/** A game-setup option offered by the create-room form. */
interface GameSetupOption {
  /** The opaque `game_setup` id sent to the server (see ADR 0013 vocabulary). */
  readonly id: string;
  /** Display label. */
  readonly label: string;
  /** The seat count this setup is designed for (pre-fills the seat selector). */
  readonly seats: number;
}

/**
 * Game-setup options offered in the create form. The `game_setup` id is opaque to
 * the client and validated server-side; these are just the choices a player picks
 * from (the catalogue is owned by ADR 0013 / the server's format registry).
 */
const GAME_SETUPS: readonly GameSetupOption[] = [
  { id: '1v1', label: '1v1 Duel', seats: 2 },
  { id: 'ffa-4', label: 'Free-for-all (4)', seats: 4 },
];

/** The seat counts the lobby offers, matching the protocol's `2..=8` range. */
const SEAT_COUNTS = [2, 3, 4, 5, 6, 7, 8] as const;

/**
 * The basic-land glyphs a deck tile shows, in WUBRG order: display-only reads of
 * the static decklist (which basics it runs), tinted by the card-token frame hue —
 * the same "what colors" read the table's land chips give. No card logic: this
 * never touches cost, legality, or effect.
 */
const BASIC_LAND_GLYPHS: ReadonlyArray<{
  readonly name: string;
  readonly glyph: GlyphName;
  readonly hue: string;
}> = [
  { name: 'Plains', glyph: 'land-plains', hue: PALETTE.W },
  { name: 'Island', glyph: 'land-island', hue: PALETTE.U },
  { name: 'Swamp', glyph: 'land-swamp', hue: PALETTE.B },
  { name: 'Mountain', glyph: 'land-mountain', hue: PALETTE.R },
  { name: 'Forest', glyph: 'land-forest', hue: PALETTE.G },
];

/** The land glyphs (with hues) for the basics a decklist actually runs. */
function deckLandGlyphs(deck: Decklist): typeof BASIC_LAND_GLYPHS {
  return BASIC_LAND_GLYPHS.filter((land) => deck.entries.some((e) => e.name === land.name));
}

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

/**
 * A human label for an opaque `game_setup` id: the known options' display label,
 * falling back to the raw id (which is server-owned and forward-compatible — an
 * unknown setup still renders, never blank).
 */
function setupLabel(gameSetup: string): string {
  return GAME_SETUPS.find((option) => option.id === gameSetup)?.label ?? gameSetup;
}

/** The seat's identity accent as an inline custom property (see `.seat`). */
function seatAccentStyle(seatIndex: number): CSSProperties {
  return {
    '--seat-accent': IDENTITY_ACCENTS[seatIndex % IDENTITY_ACCENTS.length],
  } as CSSProperties;
}

/**
 * The lobby's brand header bar: the compact lockup left (the wordmark keeps the
 * accessible product name), session actions right. Disconnect lives here — a
 * session-level action, out of the find-a-game flow.
 */
function LobbyHeader({ onDisconnect }: { onDisconnect: () => void }) {
  return (
    <header className={l.lobbyHeader}>
      <div className={l.lobbyBrand}>
        <RuneMark size={28} className={l.mark} />
        <h1 className={l.lobbyWordmark}>RUNE</h1>
        <span className={l.lobbyTag}>Lobby</span>
      </div>
      <button
        type="button"
        className={s.button}
        onClick={onDisconnect}
        data-testid="lobby-disconnect-button"
      >
        Disconnect
      </button>
    </header>
  );
}

/**
 * The display-name field (issue #294): set or change the public name other players
 * read for this connection. Offered only when the server advertises `set_name`
 * (`valid_commands` is the sole source of interactivity). The input seeds from the
 * server's current `name` — the one load-bearing value — while what is being typed is
 * ephemeral local form state, cleared to the server's truth whenever it changes.
 */
function DisplayNameField({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const current = view.name ?? '';
  const [draft, setDraft] = useState(current);

  // Re-seed the draft when the server's accepted name changes (e.g. after a
  // reconnect re-sends it), so the field always reflects server truth at rest.
  useEffect(() => {
    setDraft(current);
  }, [current]);

  const save = (): void => {
    const next = draft.trim();
    if (next.length === 0) return;
    sendLobby(setNameCommand(next));
  };

  return (
    <section className={s.lobbySection} aria-label="Display name" data-testid="display-name">
      <h2 className={l.cardTitle}>Display name</h2>
      <div className={l.inlineForm}>
        <label className={l.inlineFormField}>
          <span className={s.fieldLabel}>How other players see you</span>
          <input
            className={s.input}
            type="text"
            autoComplete="off"
            spellCheck={false}
            maxLength={32}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') save();
            }}
            data-testid="display-name-input"
            aria-label="Display name"
          />
        </label>
        <button type="button" className={s.button} onClick={save} data-testid="set-name-button">
          {current.length > 0 ? 'Change name' : 'Set name'}
        </button>
      </div>
    </section>
  );
}

/** The pre-first-frame lobby fallback: a live status plus a Disconnect action. */
function LobbyWaiting({ onDisconnect }: { onDisconnect: () => void }) {
  return (
    <main className={l.screen}>
      <div className={l.motif} aria-hidden="true">
        <RuneMark size={520} />
      </div>
      <section className={l.lobbyShell} aria-label="Entering lobby" data-testid="lobby-waiting">
        <LobbyHeader onDisconnect={onDisconnect} />
        <span className={cx(l.state, l.stateConnecting)}>
          <span className={cx(l.dot, l.dotLive)} />
          Connected — entering the lobby
        </span>
      </section>
    </main>
  );
}

/**
 * One row of the room directory (issue #280). A `gathering` room with an open seat
 * shows a Join button (only when `join_room` is offered — `valid_commands` gates
 * interactivity); a full `gathering` room shows "Full"; an `in_progress` room is
 * visible but un-joinable. All of it is derived from the `RoomSummary` — no legality
 * computed here.
 */
function RoomDirectoryRow({
  room,
  canJoin,
  canSpectate,
  onJoin,
  onSpectate,
}: {
  room: RoomSummary;
  canJoin: boolean;
  canSpectate: boolean;
  onJoin: (roomId: string) => void;
  onSpectate: (roomId: string) => void;
}) {
  const total = room.config.seats;
  const started = room.state === 'in_progress';
  const full = room.filled >= total;

  // A started room is un-joinable but **spectatable** (issue #351): it shows an
  // "In progress" badge plus a Spectate button when the server offers `spectate_room`.
  // An open gathering room offers Join (gated by `join_room`); a full one shows Full.
  const action = started ? (
    <>
      <span className={s.seatBadge} data-testid={`room-${room.room_id}-in-progress`}>
        In progress
      </span>
      {canSpectate && (
        <button
          type="button"
          className={s.button}
          onClick={() => onSpectate(room.room_id)}
          data-testid={`spectate-directory-${room.room_id}`}
        >
          Spectate
        </button>
      )}
    </>
  ) : full ? (
    <span className={s.seatBadge} data-testid={`room-${room.room_id}-full`}>
      Full
    </span>
  ) : canJoin ? (
    <button
      type="button"
      className={s.button}
      onClick={() => onJoin(room.room_id)}
      data-testid={`join-directory-${room.room_id}`}
    >
      Join
    </button>
  ) : null;

  return (
    <li className={s.roomRow} data-testid={`room-row-${room.room_id}`}>
      <span className={s.roomRowInfo}>
        <span>
          {setupLabel(room.config.game_setup)} · {total} seats
        </span>
        <span className={s.muted} data-testid={`room-${room.room_id}-occupancy`}>
          {room.filled}/{total} filled
          {room.spectators > 0 && (
            <span data-testid={`room-${room.room_id}-spectators`}>
              {' '}
              · {room.spectators} watching
            </span>
          )}
        </span>
      </span>
      <span className={s.roomRowActions}>{action}</span>
    </li>
  );
}

/**
 * The room browser (issue #280) — the PRIMARY "find a game" path (issue #300): the
 * list of open games, plus an empty state. Accented ahead of the secondary
 * create/join paths.
 */
function RoomDirectory({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const canJoin = can(view, 'join_room');
  const canSpectate = can(view, 'spectate_room');
  const join = (roomId: string): void => {
    sendLobby(joinRoomCommand(roomId));
  };
  const spectate = (roomId: string): void => {
    sendLobby(spectateRoomCommand(roomId));
  };

  return (
    <section
      className={cx(s.lobbySection, l.primarySection)}
      aria-label="Open games"
      data-testid="room-directory"
    >
      <span className={l.kicker}>Find a game</span>
      <h2 className={l.cardTitle}>Open games</h2>
      {view.directory.length === 0 ? (
        <span className={s.roomListEmpty} data-testid="room-directory-empty">
          No open games — create one.
        </span>
      ) : (
        <ul className={s.roomList} data-testid="room-directory-list">
          {view.directory.map((room) => (
            <RoomDirectoryRow
              key={room.room_id}
              room={room}
              canJoin={canJoin}
              canSpectate={canSpectate}
              onJoin={join}
              onSpectate={spectate}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

/** The create-a-room / room-directory / join-by-id screen, shown when room-less. */
function RoomEntry({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const [setupId, setSetupId] = useState(GAME_SETUPS[0].id);
  const [seats, setSeats] = useState<number>(GAME_SETUPS[0].seats);
  const [roomId, setRoomId] = useState('');
  const [joinError, setJoinError] = useState<string | null>(null);

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

  return (
    <>
      {/* Primary path: browse and join an open game. */}
      <RoomDirectory view={view} />

      {/* Secondary paths: start your own room, or join a specific id you were sent. */}
      <div className={l.secondary}>
        {can(view, 'create_room') && (
          <section
            className={cx(s.lobbySection, l.secondaryCard)}
            aria-label="Create a room"
            data-testid="create-room"
          >
            <span className={l.kicker}>Or start your own</span>
            <h2 className={l.cardTitle}>Create a room</h2>
            <label className={s.field}>
              <span className={s.fieldLabel}>Game setup</span>
              <select
                className={s.select}
                value={setupId}
                data-testid="game-setup-select"
                onChange={(event) => {
                  const next = event.target.value;
                  setSetupId(next);
                  const found = GAME_SETUPS.find((option) => option.id === next);
                  if (found) setSeats(found.seats);
                }}
              >
                {GAME_SETUPS.map((option) => (
                  <option key={option.id} value={option.id}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label className={s.field}>
              <span className={s.fieldLabel}>Seats</span>
              <select
                className={s.select}
                value={seats}
                data-testid="seat-count-select"
                onChange={(event) => setSeats(Number(event.target.value))}
              >
                {SEAT_COUNTS.map((count) => (
                  <option key={count} value={count}>
                    {count}
                  </option>
                ))}
              </select>
            </label>
            <div className={s.buttonRow}>
              <button
                type="button"
                className={s.button}
                onClick={create}
                data-testid="create-room-button"
              >
                Create room
              </button>
            </div>
          </section>
        )}

        {can(view, 'join_room') && (
          <details className={cx(s.lobbySection, l.secondaryCard)} data-testid="join-room">
            <summary className={s.joinByIdSummary}>Join by room id</summary>
            <label className={s.field}>
              <span className={s.fieldLabel}>Room id</span>
              <input
                className={s.input}
                type="text"
                autoComplete="off"
                spellCheck={false}
                value={roomId}
                onChange={(event) => setRoomId(event.target.value)}
                data-testid="join-room-input"
                aria-label="Room id"
              />
            </label>
            {joinError !== null && (
              <span className={s.errorText} role="alert" data-testid="join-room-error">
                {joinError}
              </span>
            )}
            <div className={s.buttonRow}>
              <button
                type="button"
                className={s.button}
                onClick={join}
                data-testid="join-room-button"
              >
                Join room
              </button>
            </div>
          </details>
        )}
      </div>
    </>
  );
}

/**
 * One starter deck as a selectable tile: name in the display face, the deck's
 * basic-land glyphs in their frame hues, summary, and size. Selection is the blue
 * ring (`aria-pressed` carries it for assistive tech — never color alone).
 */
function DeckTile({
  deck,
  selected,
  onSelect,
}: {
  deck: Decklist;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className={cx(l.deckTile, selected && l.deckTileSelected)}
      aria-pressed={selected}
      onClick={() => onSelect(deck.id)}
      data-testid={`deck-tile-${deck.id}`}
    >
      <span className={l.deckTileHead}>
        <span className={l.deckName}>{deck.name}</span>
        <span className={l.deckGlyphs}>
          {deckLandGlyphs(deck).map((land) => (
            <span key={land.glyph} style={{ color: land.hue, display: 'inline-flex' }}>
              <Glyph name={land.glyph} size={16} label={land.name} />
            </span>
          ))}
        </span>
      </span>
      <span className={l.deckSummary}>{deck.summary}</span>
      <span className={l.deckMeta}>{decklistSize(deck)} cards</span>
    </button>
  );
}

/** One roster seat: identity accent, seat glyph, name, and glyph-coded state. */
function SeatRow({ view, seat }: { view: LobbyView; seat: SeatView }) {
  const occupied = seat.occupied_by !== undefined;
  const isLocal = occupied && seat.occupied_by === view.you;
  return (
    <li
      className={cx(l.seat, !occupied && l.seatOpen)}
      style={occupied ? seatAccentStyle(seat.seat) : undefined}
      data-testid={`seat-${seat.seat}`}
    >
      <Glyph name="seat" size={18} className={l.seatGlyph} />
      <span className={l.seatName}>{occupied ? seatDisplayName(seat) : 'Open seat'}</span>
      {isLocal && <span className={l.youTag}>You</span>}
      <span className={l.seatStatus}>
        {seat.decked === true && (
          <span className={l.stateChipOn} data-testid={`seat-${seat.seat}-decked`}>
            <Glyph name="zone-library" size={12} />
            Decked
          </span>
        )}
        {occupied &&
          (seat.ready === true ? (
            <span className={l.stateChipOn} data-testid={`seat-${seat.seat}-ready`}>
              <Glyph name="ready" size={12} />
              Ready
            </span>
          ) : (
            <span className={l.stateChip}>Not ready</span>
          ))}
        {!occupied && <span className={l.stateChip}>Open</span>}
      </span>
    </li>
  );
}

/** The room roster + deck/ready controls, shown once in a room. */
function RoomPanel({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const room = view.room;
  const [deckId, setDeckId] = useState(STARTER_DECKLISTS[0].id);
  const [copied, setCopied] = useState(false);

  // The "Copied" flash is transient chrome; clear it shortly after it shows so the
  // button returns to its idle label (nothing load-bearing).
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);

  if (room === undefined) return null;
  const mySeat = room.seats.find((seat) => seat.occupied_by === view.you);
  const decked = mySeat?.decked === true;

  // Presentation-only counts read straight off the view (no legality computed):
  // the room's one-line "where are we" summary.
  const filled = room.seats.filter((seat) => seat.occupied_by !== undefined).length;
  const ready = room.seats.filter((seat) => seat.ready === true).length;
  const total = room.seats.length;

  const copyRoomId = (): void => {
    const write = navigator.clipboard?.writeText?.(room.room_id);
    if (write && typeof write.then === 'function') {
      write.then(
        () => setCopied(true),
        () => setCopied(false),
      );
    } else {
      setCopied(true);
    }
  };

  const submitDeck = (): void => {
    const deck = decklistById(deckId);
    if (deck === undefined) return;
    sendLobby(submitDeckCommand(decklistCards(deck)));
  };

  return (
    <>
      <section className={s.lobbySection} aria-label="Room" data-testid="room-panel">
        <h2 className={l.cardTitle}>Room</h2>
        <span className={l.roomStatus} data-testid="room-status">
          {filled}/{total} seats filled · {ready} ready
        </span>
        <div className={s.roomIdRow}>
          <span className={s.fieldLabel}>Room id</span>
          <code className={s.roomIdCode} data-testid="room-id">
            {room.room_id}
          </code>
          <button
            type="button"
            className={s.button}
            onClick={copyRoomId}
            data-testid="copy-room-id-button"
            aria-label="Copy room id"
          >
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
        <span className={s.muted}>Share this id so another player can join.</span>

        <ul className={s.seatList} data-testid="seat-list">
          {room.seats.map((seat) => (
            <SeatRow key={seat.seat} view={view} seat={seat} />
          ))}
        </ul>
      </section>

      {can(view, 'submit_deck') && (
        <section
          className={s.lobbySection}
          aria-label="Choose a deck"
          data-testid="deck-select-section"
        >
          <h2 className={l.cardTitle}>Choose a deck</h2>
          <div className={l.deckGrid} role="group" aria-label="Starter decks">
            {STARTER_DECKLISTS.map((deck) => (
              <DeckTile
                key={deck.id}
                deck={deck}
                selected={deck.id === deckId}
                onSelect={setDeckId}
              />
            ))}
          </div>
          <div className={s.buttonRow}>
            <button
              type="button"
              className={cx(s.button, !decked && s.buttonPrimary)}
              onClick={submitDeck}
              data-testid="submit-deck-button"
            >
              {decked ? 'Resubmit deck' : 'Submit deck'}
            </button>
          </div>
        </section>
      )}

      {/* The ready bar: gold on the one advance-the-game action; Leave sits apart
          on the right so the room-exit never neighbors it. */}
      <div className={l.readyBar}>
        {can(view, 'ready') && (
          <button
            type="button"
            className={cx(s.button, decked && s.buttonPrimary)}
            onClick={() => sendLobby(readyCommand(true))}
            data-testid="ready-button"
          >
            Ready
          </button>
        )}
        {can(view, 'unready') && (
          <button
            type="button"
            className={s.button}
            onClick={() => sendLobby(readyCommand(false))}
            data-testid="unready-button"
          >
            Not ready
          </button>
        )}
        {can(view, 'leave') && (
          <button
            type="button"
            className={cx(s.button, l.readyBarSpacer)}
            onClick={() => sendLobby(leaveCommand())}
            data-testid="leave-room-button"
          >
            Leave room
          </button>
        )}
      </div>
    </>
  );
}

export function LobbyScreen() {
  const lobby = useGameStore((state) => state.lobby);
  const lobbyError = useGameStore((state) => state.lobbyError);
  const disconnect = useGameStore((state) => state.disconnect);

  // Socket is open but the first LobbyView has not arrived yet: keep an
  // interactive Disconnect on screen (never a dead screen).
  if (lobby === null) {
    return <LobbyWaiting onDisconnect={disconnect} />;
  }

  return (
    <main className={l.screen}>
      <div className={l.motif} aria-hidden="true">
        <RuneMark size={520} />
      </div>
      <section className={l.lobbyShell} aria-label="Lobby" data-testid="lobby-screen">
        <LobbyHeader onDisconnect={disconnect} />
        {lobbyError !== null && (
          <span className={s.errorText} role="alert" data-testid="lobby-error">
            {lobbyError}
          </span>
        )}
        {can(lobby, 'set_name') && <DisplayNameField view={lobby} />}
        {lobby.room === undefined ? <RoomEntry view={lobby} /> : <RoomPanel view={lobby} />}
      </section>
    </main>
  );
}
