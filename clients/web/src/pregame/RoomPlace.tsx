/**
 * The ready room (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-game-lobby.jpg` baseline).
 *
 * **The arena is the seating diagram.** #506 drew the room as a roster panel, an
 * AI-seating panel, a deck panel, and a pinned ready bar; the baseline draws the
 * table you are about to sit at — seats standing around the ring, the local seat
 * at the bottom where that player's hand will be, and the room's state in the
 * middle. Everything else is at the edges or behind the seat that owns it:
 *
 * - **Top** — the server plaque, then the table's own plaque: its name, and the
 *   `format · seats · visibility` line under it.
 * - **The ring** — one `SeatPlaque` per seat, positioned by `seatRing.ts`.
 *   Occupied seats show identity, readiness, and their deck state; the local
 *   seat expands for its deck dropdown and quiet edit/import shortcuts; an empty
 *   seat carries its own invite/add-AI options.
 * - **The middle** — the gate in words, from {@link readyGate}, plus the ready
 *   count. This is where "you are not ready because your deck is not in" is
 *   said, and it is recomputed from every frame so it can never disagree with
 *   the seats around it.
 * - **Under the local seat** — the one blue primary: Submit deck, then Ready.
 * - **Edges** — Decks bottom-left, Leave bottom-centre, chat on the right edge,
 *   the #505 settings handle bottom-right.
 *
 * ## Where the baseline and the shipped contract disagree
 *
 * The baseline draws each occupied seat's chosen deck by NAME, and gives the
 * host an **Edit Table** control. The wire supports neither: a `SeatView` is
 * `{ seat, occupied_by?, name?, decked?, ready?, ai? }` — the decklist is
 * deliberately redacted, so no seat's deck name is knowable, not even the local
 * player's after a reload — and there is no `update_room` command, so a created
 * room's configuration is immutable. Both are reported rather than faked: a deck
 * name the client made up, or an Edit Table that silently did nothing, would be
 * worse than their absence.
 *
 * Everything is derived from the latest `LobbyView`; local state is ephemeral
 * form input only (the picked deck, a "Copied" flash, the open builder). **No
 * legality is computed here** — `valid_commands` gates every affordance and the
 * server validates every submitted deck.
 */
import { useEffect, useState, type CSSProperties } from 'react';
import { DeckBuilder } from '../DeckBuilder';
import {
  addAiCommand,
  leaveCommand,
  readyCommand,
  removeAiCommand,
  submitDeckCommand,
  type LobbyView,
} from '../protocol';
import { STARTER_DECKLISTS } from '../decklists';
import { useGameStore } from '../store';
import { cx } from '../chrome/cx';
import { ControlButton } from '../table/controls';
import { deckOptionById, deckOptions, optionCounts, useSavedDecks } from './deckChoice';
import { serverLabel } from './serverIdentity';
import { ChatEdge, MenuFrame, Plaque, ServerPlaque, SessionMenu } from './MenuFrame';
import { SeatPlaque } from './SeatPlaque';
import { setupLabel } from './gameSetups';
import { readyGate } from './readyGate';
import { seatRing } from './seatRing';
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
  const serverUrl = useGameStore((state) => state.serverUrl);
  const saved = useSavedDecks();
  const room = view.room;
  const [deckId, setDeckId] = useState(STARTER_DECKLISTS[0]!.id);
  const [copied, setCopied] = useState(false);
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
  const filled = room.seats.filter(seatFilled).length;
  const ready = room.seats.filter((seat) => seat.ready === true).length;
  const total = room.seats.length;

  // The room's advertised format rules, matched from the catalog by the room's
  // `game_setup`. Absent until the catalog arrives; the room then omits them.
  const roomFormat = catalog?.formats.find(
    (format) => format.game_setup === room.config.game_setup,
  );
  // Whether this room's format requires a designated commander is learned from
  // the advertised metadata (issue #394), never a hardcoded format name.
  const requiresCommander = roomFormat?.requires_commander === true;

  const options = deckOptions(saved);
  const picked = deckOptionById(options, deckId);
  const canSubmit = can(view, 'submit_deck');

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
    if (picked === undefined) return;
    sendLobby(submitDeckCommand(picked.cards, requiresCommander ? picked.commander : undefined));
  };

  const openBuilder = (): void => {
    if (catalog === null) requestCatalog();
    setBuilderOpen(true);
  };

  const gate = readyGate(view);
  const slots = seatRing(total, mySeat?.seat);

  // The one blue primary of this state (§4.1): the gate names which advertised
  // command it is, and it is the ONLY blue control the room draws.
  const primary =
    gate?.gold === 'submit_deck'
      ? { label: 'Submit Deck', testId: 'submit-deck-button', press: submitDeck }
      : gate?.gold === 'ready'
        ? { label: 'Ready', testId: 'ready-button', press: () => sendLobby(readyCommand(true)) }
        : undefined;

  return (
    <MenuFrame
      label="Room"
      testId="lobby-screen"
      top={
        <>
          <ServerPlaque name={serverLabel(serverUrl)} />
          <Plaque faceClass={p.centreFace} testId="room-plaque">
            <h2 className={p.arenaTitle} data-place-heading tabIndex={-1}>
              {setupLabel(room.config.game_setup)}
            </h2>
            <span className={p.plaqueMeta} data-testid="room-status">
              {setupLabel(room.config.game_setup)} · {total} seats · Public · {filled}/{total}{' '}
              filled · {ready} ready
            </span>
          </Plaque>
        </>
      }
      edge={<ChatEdge />}
      footStart={
        <span className={p.fit}>
          <ControlButton
            variant="utility"
            label="Decks"
            accessibleName="Open the deck builder"
            onPress={openBuilder}
            testId="open-decks-button"
          />
        </span>
      }
      foot={
        can(view, 'leave') ? (
          <span className={p.fit}>
            <ControlButton
              variant="secondary"
              label="Leave"
              accessibleName="Leave this table"
              onPress={() => sendLobby(leaveCommand())}
              testId="leave-room-button"
            />
          </span>
        ) : undefined
      }
      footEnd={<SessionMenu onDisconnect={disconnect} />}
    >
      {lobbyError !== null && (
        <span className={cx(p.error, p.rejected)} role="alert" data-testid="lobby-error">
          {lobbyError}
        </span>
      )}

      <div className={p.ring} data-testid="seat-ring">
        {slots.map((slot) => {
          const seat = room.seats[slot.seat]!;
          const local = slot.local;
          return (
            <div
              key={slot.seat}
              className={p.ringSeat}
              style={{ '--seat-x': slot.x, '--seat-y': slot.y } as CSSProperties}
            >
              <SeatPlaque
                seat={seat}
                local={local}
                deckChoice={
                  local && canSubmit
                    ? {
                        options,
                        selectedId: picked?.id ?? deckId,
                        onSelect: setDeckId,
                        onEdit: openBuilder,
                        onImport: openBuilder,
                      }
                    : undefined
                }
                seatOptions={
                  seatFilled(seat)
                    ? undefined
                    : {
                        roomId: room.room_id,
                        onCopyRoomId: copyRoomId,
                        copied,
                        aiOptions: catalog?.ai_opponents ?? [],
                        deckOptions: options,
                        onAddAi: can(view, 'add_ai')
                          ? (seatIndex, kind, chosenDeckId) => {
                              const deck = deckOptionById(options, chosenDeckId);
                              if (deck === undefined) return;
                              sendLobby(
                                addAiCommand(
                                  seatIndex,
                                  kind,
                                  deck.cards,
                                  requiresCommander ? deck.commander : undefined,
                                ),
                              );
                            }
                          : undefined,
                      }
                }
                onRemoveAi={
                  can(view, 'remove_ai') ? () => sendLobby(removeAiCommand(seat.seat)) : undefined
                }
              />
              {/* The primary lives under the local seat, where the baseline puts
                  READY — the decision belongs to that seat, not to a bar. */}
              {local && (
                <div className={p.controlColumn}>
                  {primary !== undefined && (
                    <ControlButton
                      variant="primary"
                      label={primary.label}
                      onPress={primary.press}
                      testId={primary.testId}
                    />
                  )}
                  {/* A resubmit stays offered while advertised, but it is never
                      the blue one — that is the gate's job to decide. */}
                  {canSubmit && gate?.gold !== 'submit_deck' && (
                    <span className={p.wide}>
                      <ControlButton
                        variant="secondary"
                        label="Resubmit deck"
                        onPress={submitDeck}
                        testId="submit-deck-button"
                      />
                    </span>
                  )}
                  {gate?.unready === true && (
                    <span className={p.wide}>
                      <ControlButton
                        variant="secondary"
                        label="Not ready"
                        onPress={() => sendLobby(readyCommand(false))}
                        testId="unready-button"
                      />
                    </span>
                  )}
                  {requiresCommander && picked?.commanderLabel !== undefined && (
                    <span className={p.muted} data-testid="designated-commander">
                      Commander: {picked.commanderLabel}
                    </span>
                  )}
                </div>
              )}
            </div>
          );
        })}

        {gate !== null && (
          <div className={p.ringCentre}>
            <Plaque faceClass={p.centreFace} testId="ready-gate">
              <span
                className={p.centreGate}
                data-testid={gate.ready && !gate.starting ? 'ready-waiting' : undefined}
              >
                {gate.sentence}
              </span>
              <span className={p.centreCount}>
                {ready} of {total} ready
              </span>
            </Plaque>
          </div>
        )}
      </div>

      {builderOpen && (
        <DeckBuilder
          catalog={catalog}
          format={roomFormat}
          initialCounts={optionCounts(picked)}
          // Seed the designation from the picked deck's commander in a commander
          // format, so opening the builder keeps a legal starting point to edit.
          initialCommander={requiresCommander ? picked?.commander : undefined}
          error={lobbyError}
          // Same gate as the seat's own deck choice: a seat that has readied is
          // no longer offered `submit_deck`, so the builder is a library there
          // too rather than drawing a control the server would refuse.
          onSubmit={
            canSubmit
              ? (cards, commander) => sendLobby(submitDeckCommand(cards, commander))
              : undefined
          }
          onClose={() => setBuilderOpen(false)}
        />
      )}
    </MenuFrame>
  );
}
