import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { LobbyScreen } from './LobbyScreen';
import { STARTER_DECKLISTS, decklistSize } from './decklists';
import { useGameStore, type SocketFactory } from './store';
import {
  LOBBY_DIRECTORY_JSON,
  LOBBY_ROOMLESS_JSON,
  LOBBY_ROOM_ALL_READY_JSON,
  LOBBY_ROOM_COMMANDER_JSON,
  LOBBY_ROOM_DECKED_JSON,
  LOBBY_ROOM_HOST_CAN_ADD_AI_JSON,
  LOBBY_ROOM_UNDECKED_JSON,
  LOBBY_ROOM_WITH_AI_JSON,
} from './lobby-view.fixture';
import { CATALOG_JSON, CATALOG_VIEW } from './catalog-view.fixture';

/** A manually-driven WebSocket stand-in that records the frames sent to it. */
class FakeSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.onclose?.({});
  }
  emitOpen(): void {
    this.onopen?.({});
  }
  emitMessage(data: string): void {
    this.onmessage?.({ data });
  }
}

function recordingFactory(): { factory: SocketFactory; sockets: FakeSocket[] } {
  const sockets: FakeSocket[] = [];
  const factory: SocketFactory = () => {
    const s = new FakeSocket();
    sockets.push(s);
    return s as unknown as WebSocket;
  };
  return { factory, sockets };
}

/**
 * Connect the real store through a fake socket, open it, and push one lobby frame,
 * then render the lobby screen. Returns the socket so a test can assert what the
 * clicks send back (the last frame; the socket also carries the `Hello`).
 */
function mountLobby(frameJson: string): FakeSocket {
  const { factory, sockets } = recordingFactory();
  act(() =>
    useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
  );
  act(() => sockets[0].emitOpen());
  act(() => sockets[0].emitMessage(frameJson));
  render(<LobbyScreen />);
  return sockets[0];
}

/** The last frame the client sent, parsed. */
function lastSent(socket: FakeSocket): unknown {
  return JSON.parse(socket.sent[socket.sent.length - 1]);
}

/**
 * A catalog frame (issue #367/#394) advertising the given formats' deck rules. The
 * commander affordance keys off a format's advertised `requires_commander` flag, so a
 * test emits this to make that fact available rather than relying on a format name.
 */
function catalogFrame(formats: Array<Record<string, unknown>>): string {
  return JSON.stringify({ catalog_version: 1, formats });
}

/** The commander format advertised with its deck rules, including the #394 flags. */
const COMMANDER_FORMAT_FACTS = {
  game_setup: 'commander',
  min_deck_size: 100,
  max_deck_size: 100,
  max_copies: 1,
  basic_land_exempt: true,
  requires_commander: true,
  enforce_color_identity: true,
  min_seats: 2,
  max_seats: 4,
};

/** The 1v1 duel format advertised with no commander requirement (the flag elided). */
const DUEL_FORMAT_FACTS = {
  game_setup: '1v1',
  min_deck_size: 40,
  max_copies: 4,
  basic_land_exempt: true,
  min_seats: 2,
  max_seats: 2,
};

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({
    status: 'idle',
    view: null,
    lobby: null,
    lobbyError: null,
    catalog: null,
  });
});

/** Open the create-table setup, which #546 made a destination, not a form. */
function openCreate(): void {
  fireEvent.click(screen.getByTestId('open-create-game-button'));
}

/** Pick a deck in the local seat's dropdown by its option id. */
function pickDeck(id: string): void {
  fireEvent.change(screen.getByTestId('deck-select'), { target: { value: id } });
}

describe('LobbyScreen (issue #114)', () => {
  it('offers create and join-by-id when room-less; create sends the configured setup', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);

    expect(screen.getByTestId('join-room')).toBeDefined();
    openCreate();
    expect(screen.getByTestId('create-room')).toBeDefined();

    // Step the seat count up from the 1v1 default of 2.
    for (let i = 0; i < 4; i += 1) fireEvent.click(screen.getByTestId('seat-count-increase'));
    fireEvent.click(screen.getByTestId('create-room-button'));

    expect(lastSent(socket)).toEqual({
      type: 'create_room',
      // A table the host did not name carries no `name` (issue #546); `visibility`
      // rides every config, defaulting to the public listing every room already had.
      config: { seats: 6, game_setup: '1v1', visibility: 'public' },
    });
  });

  it('picking a format pre-fills its designed seat count', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    openCreate();

    // The free-for-all format pre-fills 4 seats without a separate seat pick.
    fireEvent.change(screen.getByTestId('game-setup-select'), { target: { value: 'ffa-4' } });
    expect(screen.getByTestId('seat-count').textContent).toBe('4');
    fireEvent.click(screen.getByTestId('create-room-button'));

    expect(lastSent(socket)).toEqual({
      type: 'create_room',
      config: { seats: 4, game_setup: 'ffa-4', visibility: 'public' },
    });
  });

  it('validates an empty room id locally (no send) and joins with a real id', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    const sentBefore = socket.sent.length;

    // Empty id: a local, non-fatal validation error and nothing sent.
    fireEvent.click(screen.getByTestId('join-room-button'));
    expect(screen.getByTestId('join-room-error')).toBeDefined();
    expect(socket.sent).toHaveLength(sentBefore);

    // A real id joins.
    fireEvent.change(screen.getByTestId('join-room-input'), { target: { value: '  r:7f3  ' } });
    fireEvent.click(screen.getByTestId('join-room-button'));
    expect(lastSent(socket)).toEqual({ type: 'join_room', room_id: 'r:7f3' });
  });

  it('renders a room: the seat ring, the invite id, and only advertised commands', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);

    // Seat 0 is mine; seat 1 is open and carries the invite contextually.
    expect(screen.getByTestId('seat-0')).toBeDefined();
    expect(screen.getByTestId('seat-1').textContent).toContain('Open seat');
    fireEvent.click(screen.getByTestId('seat-1-options-button'));
    expect(screen.getByTestId('room-id').textContent).toBe('r:7f3');

    // valid_commands = [submit_deck, ready, leave]: ready shown, unready hidden.
    expect(screen.getByTestId('ready-button')).toBeDefined();
    expect(screen.queryByTestId('unready-button')).toBeNull();
    expect(screen.getByTestId('leave-room-button')).toBeDefined();
  });

  it('submits a bundled deck as a non-empty card list', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('submit-deck-button'));

    const sent = lastSent(socket) as { type: string; cards: string[] };
    expect(sent.type).toBe('submit_deck');
    expect(Array.isArray(sent.cards)).toBe(true);
    expect(sent.cards.length).toBeGreaterThan(0);
  });

  it('readies up', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('ready-button'));
    expect(lastSent(socket)).toEqual({ type: 'ready', ready: true });
  });

  it('shows per-seat deck and ready state for a full room', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    for (const seat of [0, 1]) {
      // The seat states its deck in words and its readiness in a word.
      expect(screen.getByTestId(`seat-${seat}-deck`).textContent).toBe('Deck submitted');
      expect(screen.getByTestId(`seat-${seat}-ready`).textContent).toBe('Ready');
    }
    // Both ready: only unready/leave remain.
    expect(screen.queryByTestId('ready-button')).toBeNull();
    expect(screen.getByTestId('unready-button')).toBeDefined();
  });

  it('copies the room id to the clipboard from the open seat’s options', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('seat-1-options-button'));
    await act(async () => {
      fireEvent.click(screen.getByTestId('copy-room-id-button'));
    });

    expect(writeText).toHaveBeenCalledWith('r:7f3');
    expect(screen.getByTestId('copy-room-id-button').textContent).toBe('Copied');
  });

  it('shows the empty room-directory state when there are no open games (issue #280)', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    expect(screen.getByTestId('room-directory')).toBeDefined();
    expect(screen.getByTestId('room-directory-empty').textContent).toContain('No open games');
    // No rows are rendered.
    expect(screen.queryByTestId('room-directory-list')).toBeNull();
  });

  it('lists open games: a joinable gathering room and an un-joinable in-progress one', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);

    // r0 is gathering with an open seat: selecting it offers Join.
    expect(screen.getByTestId('room-row-r0')).toBeDefined();
    expect(screen.getByTestId('room-r0-occupancy').textContent).toContain('1/2 filled');
    expect(screen.queryByTestId('room-r0-in-progress')).toBeNull();
    fireEvent.click(screen.getByTestId('room-row-r0'));
    expect(screen.getByTestId('join-selected-button')).toBeDefined();

    // r1 is in progress: visible, and never joinable.
    expect(screen.getByTestId('room-row-r1')).toBeDefined();
    expect(screen.getByTestId('room-r1-in-progress')).toBeDefined();
    fireEvent.click(screen.getByTestId('room-row-r1'));
    expect(screen.queryByTestId('join-selected-button')).toBeNull();
  });

  it('joins the selected table from the lobby’s one primary (issue #280)', () => {
    const socket = mountLobby(LOBBY_DIRECTORY_JSON);
    fireEvent.click(screen.getByTestId('room-row-r0'));
    fireEvent.click(screen.getByTestId('join-selected-button'));
    expect(lastSent(socket)).toEqual({ type: 'join_room', room_id: 'r0' });
  });

  it('offers no Join at all when join_room is not advertised', () => {
    // A directory can ride any view, but Join only renders when the server offers
    // `join_room` (valid_commands gates interactivity). Strip it and selecting a
    // gathering room promotes nothing.
    const noJoin = JSON.stringify({
      ...JSON.parse(LOBBY_DIRECTORY_JSON),
      valid_commands: ['create_room'],
    });
    mountLobby(noJoin);
    fireEvent.click(screen.getByTestId('room-row-r0'));
    expect(screen.queryByTestId('join-selected-button')).toBeNull();
  });

  it('names roster seats: a seat-derived fallback plus a You tag for the local seat (#300)', () => {
    // #294 has not landed, so occupied seats read as the seat-derived "Player N"
    // fallback; the local seat additionally carries a "You" tag.
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    const seat0 = screen.getByTestId('seat-0');
    const seat1 = screen.getByTestId('seat-1');
    expect(seat0.textContent).toContain('Player 1');
    expect(seat0.textContent).toContain('You');
    expect(seat1.textContent).toContain('Player 2');
    // The opaque player id is never shown as the primary name.
    expect(seat1.textContent).not.toContain('p2');
  });

  it('renders RUNE identity procedurally and leads with open games (#300, #546)', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);
    // Procedural motif: an inline SVG mark and the wordmark, never an image
    // asset. The rule binds RUNE's IDENTITY; the environment backdrop behind it
    // does ship plates (ADR 0031, issue #555), so the check excludes it.
    expect(screen.getByRole('heading', { name: 'RUNE' })).toBeDefined();
    expect(document.querySelector('svg')).not.toBeNull();
    for (const img of document.querySelectorAll('img')) {
      expect(img.closest('[data-testid="scene-environment"]')).not.toBeNull();
    }

    // Open games lead, and Create Game follows them as a destination rather than
    // as a form competing for the same space (#546 acceptance).
    const directory = screen.getByTestId('room-directory');
    const create = screen.getByTestId('open-create-game-button');
    expect(
      directory.compareDocumentPosition(create) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('renders a non-fatal lobby error with the form still interactive', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    act(() =>
      useGameStore.setState({
        lobbyError: 'Could not join that room — it may be full or unknown.',
      }),
    );

    expect(screen.getByTestId('lobby-error').textContent).toContain('full or unknown');
    // The join form is still on screen to retry.
    expect(screen.getByTestId('join-room-button')).toBeDefined();
  });

  it('offers every starter deck in one dropdown, with the first selected by default', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    // One dropdown on the local seat — never the whole library at once (#546).
    for (const deck of STARTER_DECKLISTS) {
      expect(screen.getByTestId(`deck-option-${deck.id}`)).toBeDefined();
    }
    expect((screen.getByTestId('deck-select') as HTMLSelectElement).value).toBe(
      STARTER_DECKLISTS[0].id,
    );
  });

  it('submits the deck picked in the dropdown (the dropdown drives the list)', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    const picked = STARTER_DECKLISTS[STARTER_DECKLISTS.length - 1];

    pickDeck(picked.id);
    expect((screen.getByTestId('deck-select') as HTMLSelectElement).value).toBe(picked.id);

    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; cards: string[] };
    expect(sent.type).toBe('submit_deck');
    expect(sent.cards).toHaveLength(decklistSize(picked));
  });

  it('designates and submits a commander in the commander format (issue #372)', () => {
    const socket = mountLobby(LOBBY_ROOM_COMMANDER_JSON);
    // The advertised format requires a commander (issue #394): that fact, not the id,
    // gates the affordance.
    act(() => socket.emitMessage(catalogFrame([COMMANDER_FORMAT_FACTS])));
    // Pick the bundled commander deck; its designated commander is surfaced.
    pickDeck('green-command');
    expect(screen.getByTestId('designated-commander').textContent).toContain('Jedit Ojanen');

    // Submitting carries the commander identity alongside the 100-card list.
    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; cards: string[]; commander?: string };
    expect(sent.type).toBe('submit_deck');
    expect(sent.commander).toBe('jedit_ojanen');
    expect(sent.cards).toHaveLength(100);
    // The commander stays one of the deck's cards (CR 903.3), not removed from the list.
    expect(sent.cards).toContain('jedit_ojanen');
  });

  it('does not designate or send a commander outside the commander format (issue #372)', () => {
    // The same commander deck picked in a 1v1 room sends no commander, and no
    // designation chrome appears — the gate is the advertised format rule, not the deck.
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    act(() => socket.emitMessage(catalogFrame([DUEL_FORMAT_FACTS])));
    pickDeck('green-command');
    expect(screen.queryByTestId('designated-commander')).toBeNull();

    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; commander?: string };
    expect(sent.type).toBe('submit_deck');
    expect(sent.commander).toBeUndefined();
  });

  it('gates the commander affordance on the advertised flag, not the format name (#394)', () => {
    // A room whose game_setup is literally "commander" but whose advertised format does
    // NOT require a commander must neither designate nor send one: the client keys off
    // the `requires_commander` fact, never the format id.
    const socket = mountLobby(LOBBY_ROOM_COMMANDER_JSON);
    act(() =>
      socket.emitMessage(catalogFrame([{ ...COMMANDER_FORMAT_FACTS, requires_commander: false }])),
    );
    pickDeck('green-command');
    expect(screen.queryByTestId('designated-commander')).toBeNull();

    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; commander?: string };
    expect(sent.type).toBe('submit_deck');
    expect(sent.commander).toBeUndefined();
  });

  it('summarizes the table on its own plaque (format, seats, occupancy, ready)', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    const status = screen.getByTestId('room-status').textContent ?? '';
    expect(status).toContain('1v1 Duel');
    expect(status).toContain('2 seats');
    expect(status).toContain('2/2 filled');
    expect(status).toContain('2 ready');
  });

  it('offers a display-name field when set_name is advertised, sending set_name (issue #294)', () => {
    const socket = mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        valid_commands: ['set_name', 'create_room', 'join_room'],
      }),
    );
    fireEvent.change(screen.getByTestId('display-name-input'), { target: { value: '  Alice  ' } });
    fireEvent.click(screen.getByTestId('set-name-button'));
    expect(lastSent(socket)).toEqual({ type: 'set_name', name: 'Alice' });
  });

  it('hides the display-name field when set_name is not advertised', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    expect(screen.queryByTestId('display-name')).toBeNull();
  });

  it('shows a set name as a compact "Playing as" strip with an inline editor', () => {
    const socket = mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        name: 'Alice',
        valid_commands: ['set_name', 'create_room', 'join_room'],
      }),
    );
    // At rest: the name reads as a fact, not a form.
    expect(screen.getByTestId('display-name-current').textContent).toBe('Alice');
    expect(screen.queryByTestId('display-name-input')).toBeNull();

    // Change opens the inline editor seeded with the server's name; Save sends it.
    fireEvent.click(screen.getByTestId('change-name-button'));
    const input = screen.getByTestId('display-name-input') as HTMLInputElement;
    expect(input.value).toBe('Alice');
    fireEvent.change(input, { target: { value: 'Alia' } });
    fireEvent.click(screen.getByTestId('set-name-button'));
    expect(lastSent(socket)).toEqual({ type: 'set_name', name: 'Alia' });
  });

  it('designates and sends the deck commander in a commander-format room (issue #372)', () => {
    const socket = mountLobby(LOBBY_ROOM_COMMANDER_JSON);
    act(() => socket.emitMessage(catalogFrame([COMMANDER_FORMAT_FACTS])));

    // Pick the bundled commander deck; its designated commander surfaces near submit.
    pickDeck('green-command');
    expect(screen.getByTestId('designated-commander').textContent).toContain('Jedit Ojanen');

    // Submitting carries the commander identity alongside the 100-card list.
    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; cards: string[]; commander?: string };
    expect(sent.type).toBe('submit_deck');
    expect(sent.commander).toBe('jedit_ojanen');
    // The commander stays one of the deck's cards (CR 903.3), not removed from the list.
    expect(sent.cards).toContain('jedit_ojanen');
  });

  it('does not send a commander in a non-commander room, and shows no designation (#372)', () => {
    const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
    act(() => socket.emitMessage(catalogFrame([DUEL_FORMAT_FACTS])));

    // Even picking the commander-capable deck: the 1v1 room never designates a commander.
    pickDeck('green-command');
    expect(screen.queryByTestId('designated-commander')).toBeNull();

    fireEvent.click(screen.getByTestId('submit-deck-button'));
    const sent = lastSent(socket) as { type: string; commander?: string };
    expect(sent.type).toBe('submit_deck');
    expect(sent.commander).toBeUndefined();
  });

  it('offers the commander game-setup when creating a room (issue #372)', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    openCreate();
    fireEvent.change(screen.getByTestId('game-setup-select'), { target: { value: 'commander' } });
    fireEvent.click(screen.getByTestId('create-room-button'));
    expect(lastSent(socket)).toEqual({
      type: 'create_room',
      config: { seats: 4, game_setup: 'commander', visibility: 'public' },
    });
  });

  describe('deck builder (issue #368)', () => {
    it('opens the builder from the seat panel, requesting the catalog if absent', () => {
      const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
      // No catalog yet: opening requests it over the #367 command.
      fireEvent.click(screen.getByTestId('open-deck-builder-button'));
      expect(lastSent(socket)).toEqual({ type: 'request_catalog' });
      expect(screen.getByTestId('deck-builder')).toBeDefined();
      // Before the catalog arrives, a loading state (never a dead screen).
      expect(screen.getByTestId('deck-builder-loading')).toBeDefined();

      // The catalog frame arrives; every supported card is now browsable.
      act(() => socket.emitMessage(CATALOG_JSON));
      for (const card of CATALOG_VIEW.cards) {
        expect(screen.getByTestId(`deck-builder-card-${card.functional_id}`)).toBeDefined();
      }
    });

    it('builds a deck from a seat and submits it through submit_deck', () => {
      const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
      fireEvent.click(screen.getByTestId('open-deck-builder-button'));
      act(() => socket.emitMessage(CATALOG_JSON));

      // A card carries its server-computed rules text for browsing (inspect).
      fireEvent.click(screen.getByTestId('deck-builder-inspect-shock'));
      expect(screen.getByTestId('card-inspect').textContent).toContain('2 damage');
      fireEvent.click(screen.getByTestId('card-inspect-close'));

      // Start from an empty deck, assemble an arbitrary list, and submit it through
      // the existing gate (the builder opens seeded from the picked starter).
      fireEvent.click(screen.getByTestId('deck-builder-clear'));
      fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
      fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));
      fireEvent.click(screen.getByTestId('deck-builder-submit'));

      const sent = lastSent(socket) as { type: string; cards: string[] };
      expect(sent.type).toBe('submit_deck');
      expect([...sent.cards].sort()).toEqual(['serra_angel', 'shock']);
    });

    it('surfaces a rejection over the builder and preserves the built list', () => {
      const socket = mountLobby(LOBBY_ROOM_UNDECKED_JSON);
      fireEvent.click(screen.getByTestId('open-deck-builder-button'));
      act(() => socket.emitMessage(CATALOG_JSON));

      fireEvent.click(screen.getByTestId('deck-builder-clear'));
      fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
      fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
      fireEvent.click(screen.getByTestId('deck-builder-submit'));

      // The server rejects: it re-sends the still-undecked view (ADR 0012).
      act(() => socket.emitMessage(LOBBY_ROOM_UNDECKED_JSON));

      // The rejection surfaces over the modal, and the built list is preserved.
      expect(screen.getByTestId('deck-builder-error').textContent).toContain('rejected');
      expect(screen.getByTestId('deck-builder')).toBeDefined();
      expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('2');
    });

    it('shows the room format’s advertised rules while building', () => {
      const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
      fireEvent.click(screen.getByTestId('open-deck-builder-button'));
      act(() => socket.emitMessage(CATALOG_JSON));
      // The room's game_setup is '1v1' → the strict format's rules show.
      const rules = screen.getByTestId('deck-builder-format');
      expect(rules.textContent).toContain('Minimum 40 cards');
      expect(rules.textContent).toContain('Up to 4 copies');
    });

    it('still submits a starter deck one-tap, exactly as before', () => {
      const socket = mountLobby(LOBBY_ROOM_DECKED_JSON);
      // The starter tiles + Submit path is unchanged by the builder.
      fireEvent.click(screen.getByTestId('submit-deck-button'));
      const sent = lastSent(socket) as { type: string; cards: string[] };
      expect(sent.type).toBe('submit_deck');
      expect(sent.cards.length).toBeGreaterThan(0);
    });
  });

  it('labels seats by display name, falling back to "Player N" (issue #294)', () => {
    mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        name: 'Alice',
        room: {
          room_id: 'r0',
          config: { seats: 2, game_setup: '1v1' },
          seats: [
            { seat: 0, occupied_by: 'p1', name: 'Alice' },
            { seat: 1, occupied_by: 'p2' },
          ],
        },
        valid_commands: ['set_name', 'submit_deck', 'leave'],
      }),
    );
    // The local named seat reads "Alice" with a "You" marker (#300 chip); the
    // unnamed peer falls back to "Player 2".
    expect(screen.getByTestId('seat-0').textContent).toContain('Alice');
    expect(screen.getByTestId('seat-0').textContent).toContain('You');
    expect(screen.getByTestId('seat-1').textContent).toContain('Player 2');
  });

  describe('AI opponents (issue #415)', () => {
    /** A catalog frame advertising a format plus the seatable AI kinds. */
    function catalogWithAi(): string {
      return JSON.stringify({
        catalog_version: 1,
        formats: [DUEL_FORMAT_FACTS],
        ai_opponents: [
          { id: 'random', name: 'Random', description: 'Plays a random legal action.' },
        ],
      });
    }

    it('offers the host AI seating inside the open seat, sending seat, kind, and deck', () => {
      const socket = mountLobby(LOBBY_ROOM_HOST_CAN_ADD_AI_JSON);
      // Contextual (#546): it lives inside the seat it fills, so it does not
      // exist until that seat's options are opened…
      fireEvent.click(screen.getByTestId('seat-1-options-button'));
      expect(screen.queryByTestId('ai-seating')).toBeNull();
      // …and only once the catalog's advertised AI kinds arrive.
      act(() => socket.emitMessage(catalogWithAi()));

      expect(screen.getByTestId('ai-seating')).toBeTruthy();
      expect(screen.getByTestId('ai-kind-random')).toBeTruthy();
      fireEvent.click(screen.getByTestId('add-ai-button'));

      const sent = lastSent(socket) as {
        type: string;
        seat: number;
        kind: string;
        cards: string[];
      };
      expect(sent.type).toBe('add_ai');
      expect(sent.seat).toBe(1);
      expect(sent.kind).toBe('random');
      expect(sent.cards.length).toBeGreaterThan(0);
    });

    it('does not offer AI seating when the server does not advertise add_ai', () => {
      const socket = mountLobby(LOBBY_ROOM_DECKED_JSON); // no add_ai in valid_commands
      act(() => socket.emitMessage(catalogWithAi()));
      fireEvent.click(screen.getByTestId('seat-1-options-button'));
      // The invite is still there; seating an opponent is not.
      expect(screen.getByTestId('room-id')).toBeTruthy();
      expect(screen.queryByTestId('ai-seating')).toBeNull();
    });

    it('renders an AI seat in the roster and lets the host remove it', () => {
      const socket = mountLobby(LOBBY_ROOM_WITH_AI_JSON);
      // The AI seat shows as filled, tagged AI, decked and ready — no human occupant.
      const aiSeat = screen.getByTestId('seat-1');
      expect(aiSeat.textContent).toContain('Random');
      expect(screen.getByTestId('seat-1-ai')).toBeTruthy();
      expect(screen.getByTestId('seat-1-deck').textContent).toBe('Deck submitted');
      expect(screen.getByTestId('seat-1-ready').textContent).toBe('Ready');
      // The room reads full: 2/2 filled counts the AI.
      expect(screen.getByTestId('room-status').textContent).toContain('2/2 filled');

      // Removing it sends remove_ai for that seat.
      fireEvent.click(screen.getByTestId('remove-ai-1-button'));
      expect(lastSent(socket)).toEqual({ type: 'remove_ai', seat: 1 });
    });
  });
});
