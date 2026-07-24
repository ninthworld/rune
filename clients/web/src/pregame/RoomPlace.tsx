/**
 * The Room place (issue #506; `front-door-and-lobby.md` §5.3).
 *
 * Two columns at ≥ 1180 px, one below, with a persistent bottom **ready bar**:
 *
 * - **Room header** — the setup name, the live `filled/total seats · N ready`
 *   line, the room-id chip with Copy plus one line of share instruction (P6),
 *   and Leave room, kept visually apart from the gold.
 * - **Left — the roster** (`Roster`), seat accents and crest chips, with the
 *   host-only AI seating beneath it (it is a roster operation).
 * - **Right — the deck** (`DeckPicker`), the starter tiles plus Build a deck.
 * - **Ready bar** (`ReadyBar`) — the gate in words and the one gold control,
 *   pinned so it is never at the end of a scroll (P3/P4).
 *
 * Everything is derived from the latest `LobbyView`; local state is ephemeral
 * form input only (the picked deck, a "Copied" flash, the open builder). No
 * legality is computed here — `valid_commands` gates every affordance.
 */
import { useEffect, useState } from 'react';
import { DeckBuilder } from '../DeckBuilder';
import {
  addAiCommand,
  leaveCommand,
  readyCommand,
  removeAiCommand,
  submitDeckCommand,
  type LobbyView,
} from '../protocol';
import { STARTER_DECKLISTS, decklistCards, decklistById, decklistCounts } from '../decklists';
import { useGameStore } from '../store';
import { useReducedMotion } from '../table/hooks/useReducedMotion';
import { usePresentationSettings } from '../table/settings/usePresentationSettings';
import { cx } from '../chrome/cx';
import { AiSeatingCard } from './AiSeatingCard';
import { DeckGrid } from './DeckPicker';
import { commanderName } from './deckPresentation';
import { PregameHeader } from './PregameHeader';
import { ReadyBar } from './ReadyBar';
import { Roster } from './Roster';
import { setupLabel } from './gameSetups';
import { readyGate } from './readyGate';
import { seatFilled } from './seatIdentity';
import p from './styles';

/** Whether a command kind is currently offered to this connection. */
function can(view: LobbyView, command: string): boolean {
  return view.valid_commands.includes(command);
}

export function RoomPlace({ view }: { view: LobbyView }) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const catalog = useGameStore((state) => state.catalog);
  const requestCatalog = useGameStore((state) => state.requestCatalog);
  const lobbyError = useGameStore((state) => state.lobbyError);
  const disconnect = useGameStore((state) => state.disconnect);
  const settings = usePresentationSettings();
  const reducedMotion = useReducedMotion(settings.motion);
  const room = view.room;
  const [deckId, setDeckId] = useState(STARTER_DECKLISTS[0]!.id);
  const [copied, setCopied] = useState(false);
  // Whether the deck-builder modal (#368) is open — ephemeral UI state.
  const [builderOpen, setBuilderOpen] = useState(false);

  // The "Copied" flash is transient chrome; clear it shortly after it shows.
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);

  // Ensure the advertised format metadata is available while seated: the room
  // keys the commander affordance off the catalog's `requires_commander` flag
  // (issue #394). Idempotent; the reply just populates it.
  useEffect(() => {
    if (catalog === null) requestCatalog();
  }, [catalog, requestCatalog]);

  if (room === undefined) return null;
  const mySeat = room.seats.find((seat) => seat.occupied_by === view.you);
  const decked = mySeat?.decked === true;

  // Presentation-only counts read straight off the view (no legality computed).
  // An AI seat (issue #415) counts as filled.
  const filled = room.seats.filter(seatFilled).length;
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

  // The room's advertised format rules, matched from the catalog by the room's
  // `game_setup`. Absent until the catalog arrives; the panel then omits them.
  const roomFormat = catalog?.formats.find(
    (format) => format.game_setup === room.config.game_setup,
  );

  // Whether this room's format requires a designated commander is learned from
  // the advertised metadata (issue #394), never a hardcoded format name.
  const requiresCommander = roomFormat?.requires_commander === true;
  const selectedDeck = decklistById(deckId);
  const designatedCommander =
    requiresCommander && selectedDeck ? commanderName(selectedDeck) : undefined;

  const submitDeck = (): void => {
    const deck = decklistById(deckId);
    if (deck === undefined) return;
    const commander = requiresCommander ? deck.commander : undefined;
    sendLobby(submitDeckCommand(decklistCards(deck), commander));
  };

  // Open the deck builder (#368), ensuring the wire-carried card pool is
  // present: request the catalog once (idempotent).
  const openBuilder = (): void => {
    if (catalog === null) requestCatalog();
    setBuilderOpen(true);
  };

  const gate = readyGate(view);

  return (
    <div className={p.room} data-testid="lobby-screen" aria-label="Room">
      <div className={p.roomBody}>
        <PregameHeader onDisconnect={disconnect} />
        {lobbyError !== null && (
          <span className={cx(p.error, p.rejected)} role="alert" data-testid="lobby-error">
            {lobbyError}
          </span>
        )}
        <section className={p.panel} aria-label="Room" data-testid="room-panel">
          <div className={p.roomHeader}>
            <div className={p.roomHeadText}>
              <h2 className={p.title} data-place-heading tabIndex={-1}>
                {setupLabel(room.config.game_setup)}
              </h2>
              <span className={p.roomStatus} data-testid="room-status">
                {filled}/{total} seats filled · {ready} ready
              </span>
            </div>
            <div className={p.roomHeadActions}>
              <span className={p.codeChip}>
                <code className={p.codeText} data-testid="room-id">
                  {room.room_id}
                </code>
                <button
                  type="button"
                  className={p.button}
                  onClick={copyRoomId}
                  data-testid="copy-room-id-button"
                  aria-label="Copy room id"
                >
                  {copied ? 'Copied' : 'Copy'}
                </button>
              </span>
              {can(view, 'leave') && (
                <button
                  type="button"
                  className={p.button}
                  onClick={() => sendLobby(leaveCommand())}
                  data-testid="leave-room-button"
                >
                  Leave room
                </button>
              )}
            </div>
          </div>
          {/* P6: the invite is a room id, and the UI says so plainly — the same
              wording the Join field's label carries. No link scheme is invented;
              the protocol's join key IS a room id (a real join link is §9.4). */}
          <span className={p.shareLine} data-testid="room-share-line">
            Send this id to a friend — they paste it under Join.
          </span>
        </section>

        <div className={p.columns}>
          <div className={p.column}>
            <section className={p.panel} aria-label="Seats" data-testid="roster-panel">
              <h3 className={p.title}>Seats</h3>
              <Roster
                view={view}
                seats={room.seats}
                onRemoveAi={
                  can(view, 'remove_ai') ? (index) => sendLobby(removeAiCommand(index)) : undefined
                }
              />
            </section>

            {/* Host-only AI seating (issue #415): offered only when the server
                advertises `add_ai`, so host-ness is never inferred client-side. */}
            {can(view, 'add_ai') && (catalog?.ai_opponents.length ?? 0) > 0 && (
              <AiSeatingCard
                room={room}
                aiOptions={catalog?.ai_opponents ?? []}
                requiresCommander={requiresCommander}
                reducedMotion={reducedMotion}
                onAddAi={(seatIndex, kind, deck) =>
                  sendLobby(
                    addAiCommand(
                      seatIndex,
                      kind,
                      decklistCards(deck),
                      requiresCommander ? deck.commander : undefined,
                    ),
                  )
                }
              />
            )}
          </div>

          {can(view, 'submit_deck') && (
            <div className={p.column}>
              <section
                className={p.panel}
                aria-label="Choose a deck"
                data-testid="deck-select-section"
              >
                <h3 className={p.title}>Choose a deck</h3>
                <div className={p.deckColumn}>
                  <DeckGrid
                    decks={STARTER_DECKLISTS}
                    selectedId={deckId}
                    onSelect={setDeckId}
                    reducedMotion={reducedMotion}
                    label="Starter decks"
                  />
                  <div className={p.buttonRow}>
                    <button
                      type="button"
                      className={p.button}
                      onClick={openBuilder}
                      data-testid="open-deck-builder-button"
                    >
                      Build a deck
                    </button>
                  </div>
                </div>
              </section>
            </div>
          )}
        </div>
      </div>

      {gate !== null && (
        <ReadyBar
          gate={gate}
          decked={decked}
          canSubmit={can(view, 'submit_deck')}
          onSubmitDeck={submitDeck}
          onReady={() => sendLobby(readyCommand(true))}
          onUnready={() => sendLobby(readyCommand(false))}
          commanderLine={designatedCommander}
        />
      )}

      {builderOpen && (
        <DeckBuilder
          catalog={catalog}
          format={roomFormat}
          initialCounts={decklistCounts(decklistById(deckId) ?? STARTER_DECKLISTS[0]!)}
          // Seed the designation from the picked starter's commander in a
          // commander format, so opening the builder over a starter keeps a
          // legal starting point to edit (issue #396).
          initialCommander={
            requiresCommander
              ? (decklistById(deckId) ?? STARTER_DECKLISTS[0]!).commander
              : undefined
          }
          error={lobbyError}
          onSubmit={(cards, commander) => sendLobby(submitDeckCommand(cards, commander))}
          onClose={() => setBuilderOpen(false)}
        />
      )}
    </div>
  );
}
