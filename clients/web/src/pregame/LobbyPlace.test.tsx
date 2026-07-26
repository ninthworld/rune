/**
 * The lobby, the create setup, and the ready room as issue #546 composes them.
 *
 * The shipped `LobbyScreen.test.tsx` still owns the behavioural assertions —
 * what each control sends. This suite covers what the **baseline convergence**
 * adds and what a future restyle could silently undo: that open games are the
 * focus and selection is what promotes Join, that the create setup is a
 * destination rather than an embedded form, that exactly one blue primary is on
 * screen per state, that the room is a seat ring rather than a roster, that AI
 * and invite configuration only exist inside the seat that owns them, and that
 * the #505 settings path survived.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LobbyScreen } from '../LobbyScreen';
import { useGameStore, type SocketFactory } from '../store';
import {
  MemorySavedDeckDb,
  configureSavedDeckStore,
  countsToCards,
  deleteSavedDeck,
  resetSavedDeckStore,
  saveDeck,
} from '../deck/savedDeckStore';
import { SCENE_SEAT_ACCENTS } from '../sceneTokens';
import { LOCAL_PORTRAIT, OPPONENT_PORTRAITS } from '../table/seatPortraits';
import {
  LOBBY_DIRECTORY_JSON,
  LOBBY_ROOMLESS_JSON,
  LOBBY_ROOM_ALL_READY_JSON,
  LOBBY_ROOM_DECKED_JSON,
  LOBBY_ROOM_UNDECKED_JSON,
} from '../lobby-view.fixture';

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

/** Connect through a fake socket, push one frame, and render the lobby. */
function mountLobby(frameJson?: string): FakeSocket {
  const sockets: FakeSocket[] = [];
  const factory: SocketFactory = () => {
    const s = new FakeSocket();
    sockets.push(s);
    return s as unknown as WebSocket;
  };
  act(() =>
    useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
  );
  act(() => sockets[0]!.emitOpen());
  if (frameJson !== undefined) act(() => sockets[0]!.emitMessage(frameJson));
  render(<LobbyScreen />);
  return sockets[0]!;
}

/** Every blue primary currently drawn (`control-language.md` §4.1 says ≤ 1). */
function primaries(): NodeListOf<Element> {
  return document.querySelectorAll('[data-variant="primary"], [data-variant="primaryCompact"]');
}

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({
    status: 'idle',
    view: null,
    lobby: null,
    lobbyError: null,
    catalog: null,
    lastMatch: null,
  });
});

describe('Lobby — open games are the focus (#546)', () => {
  it('renders a skeleton list plus a working way off the server before the first view', () => {
    mountLobby();
    // The real composition, not a bare status sentence.
    expect(screen.getByTestId('lobby-waiting')).toBeDefined();
    expect(screen.getByTestId('room-directory')).toBeDefined();
    expect(screen.getByTestId('room-directory-skeleton')).toBeDefined();
    // Never a dead screen.
    expect(screen.getByTestId('lobby-disconnect-button')).toBeDefined();
  });

  it('promotes Join to the one blue primary only once a table is selected', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);

    // Nothing selected: no Join, and Create is not competing with one.
    expect(screen.queryByTestId('join-selected-button')).toBeNull();
    expect(screen.getByTestId('open-create-game-button').getAttribute('data-variant')).toBe(
      'secondary',
    );

    fireEvent.click(screen.getByTestId('room-row-r0'));
    expect(screen.getByTestId('room-row-r0').getAttribute('aria-pressed')).toBe('true');
    const join = screen.getByTestId('join-selected-button');
    expect(join.getAttribute('data-variant')).toBe('primary');
    expect(primaries()).toHaveLength(1);
  });

  it('offers Watch, not Join, for a table whose game is already running', () => {
    // r1 is in progress. `spectate_room` is what makes watching possible; the
    // client derives nothing beyond which advertised command the selection names.
    const socket = mountLobby(
      JSON.stringify({
        ...JSON.parse(LOBBY_DIRECTORY_JSON),
        valid_commands: ['create_room', 'join_room', 'spectate_room'],
      }),
    );
    fireEvent.click(screen.getByTestId('room-row-r1'));
    expect(screen.queryByTestId('join-selected-button')).toBeNull();
    fireEvent.click(screen.getByTestId('spectate-selected-button'));
    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'spectate_room',
      room_id: 'r1',
    });
  });

  it('offers no primary for a selected table the server did not offer to join', () => {
    const noJoin = JSON.stringify({
      ...JSON.parse(LOBBY_DIRECTORY_JSON),
      valid_commands: ['create_room'],
    });
    mountLobby(noJoin);
    fireEvent.click(screen.getByTestId('room-row-r0'));
    // `valid_commands` is the only source of interactivity: selecting a row
    // cannot conjure a command the server never advertised.
    expect(screen.queryByTestId('join-selected-button')).toBeNull();
    expect(primaries()).toHaveLength(0);
  });

  it('makes Create the only forward action when there is nothing to join', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    expect(screen.getByTestId('room-directory-empty').textContent).toContain('No open games');
    expect(screen.queryByTestId('room-directory-list')).toBeNull();
    expect(screen.getByTestId('open-create-game-button').getAttribute('data-variant')).toBe(
      'primaryCompact',
    );
    expect(primaries()).toHaveLength(1);
  });

  it('keeps join-by-id collapsed, and its words matched to the room’s share line', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    const disclosure = screen.getByTestId('join-room') as HTMLDetailsElement;
    // Collapsed: progressive disclosure, not a competing panel.
    expect(disclosure.open).toBe(false);
    const joinLabel = disclosure.textContent ?? '';
    cleanup();

    // The share line lives on the seat that has room for a guest.
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('seat-1-options-button'));
    const shareLine = screen.getByTestId('seat-options').textContent ?? '';

    expect(shareLine).toContain('Send this id to a friend');
    expect(shareLine).toContain('paste it under Join');
    expect(joinLabel).toContain('paste the id a friend sent you');
  });

  it('shows occupancy as a shape as well as a number', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);
    const pips = screen.getByTestId('room-r0-pips');
    // One pip per seat; the number stays the reading channel beside it.
    expect(pips.children).toHaveLength(2);
    expect(screen.getByTestId('room-r0-occupancy').textContent).toContain('1/2 filled');
  });

  it('keeps the #505 settings path on the handle, with Disconnect beside it', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('session-menu-button'));
    expect(screen.getByTestId('session-menu-display-settings')).toBeDefined();
    expect(screen.getByTestId('session-menu-card-art')).toBeDefined();
    expect(screen.getByTestId('session-menu-disconnect')).toBeDefined();
  });

  it('keeps the same handle reachable inside a room', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('session-menu-button'));
    expect(screen.getByTestId('session-menu-display-settings')).toBeDefined();
    expect(screen.getByTestId('session-menu-card-art')).toBeDefined();
  });

  it('leaves chat collapsed, so it never consumes the arena', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);
    expect(screen.queryByTestId('chat-panel')).toBeNull();
    fireEvent.click(screen.getByTestId('chat-toggle'));
    expect(screen.getByTestId('chat-panel')).toBeDefined();
  });
});

describe('Create setup — a destination, not an embedded form (#546)', () => {
  it('is absent from the lobby until Create Game is pressed, and Cancel comes back', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);
    // The acceptance criterion: the server lobby does not embed the form.
    expect(screen.queryByTestId('create-room')).toBeNull();

    fireEvent.click(screen.getByTestId('open-create-game-button'));
    expect(screen.getByTestId('create-room')).toBeDefined();
    // …and the open-games list is not sitting behind it competing for attention.
    expect(screen.queryByTestId('room-directory')).toBeNull();

    fireEvent.click(screen.getByTestId('create-room-cancel'));
    expect(screen.queryByTestId('create-room')).toBeNull();
    expect(screen.getByTestId('room-directory')).toBeDefined();
  });

  it('shows only the essential decisions until Advanced is opened', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('open-create-game-button'));

    // The four rows the baseline draws, and Create Table as the one blue action.
    expect(screen.getByTestId('create-table-name')).toBeDefined();
    expect(screen.getByTestId('game-setup-select')).toBeDefined();
    expect(screen.getByTestId('seat-count')).toBeDefined();
    expect(screen.getByTestId('create-visibility')).toBeDefined();
    expect(primaries()).toHaveLength(1);

    // Advanced is collapsed, and everything further is inside it rather than on
    // the face — a `<details>` renders nothing of its body while closed.
    const advanced = screen.getByTestId('create-advanced') as HTMLDetailsElement;
    expect(advanced.open).toBe(false);
    const face = screen.getByTestId('create-room');
    expect(face.textContent).toContain('not overridable per table');
    expect(advanced.textContent).toContain('not overridable per table');
    // Nothing advanced has leaked out of the disclosure onto the plaque.
    expect(face.textContent!.replace(advanced.textContent!, '')).not.toContain(
      'not overridable per table',
    );
  });

  it('steps the seat count inside the protocol’s own 2..=8 range', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('open-create-game-button'));
    // The 1v1 default is 2 seats — the bottom of the range, so decrementing is
    // a no-op rather than an illegal config.
    expect(screen.getByTestId('seat-count').textContent).toBe('2');
    fireEvent.click(screen.getByTestId('seat-count-decrease'));
    expect(screen.getByTestId('seat-count').textContent).toBe('2');

    for (let i = 0; i < 10; i += 1) fireEvent.click(screen.getByTestId('seat-count-increase'));
    expect(screen.getByTestId('seat-count').textContent).toBe('8');

    fireEvent.click(screen.getByTestId('create-room-button'));
    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'create_room',
      // An unnamed table sends no `name` at all (issue #546): the client labels it by
      // its format, and the server is never handed display prose.
      config: { seats: 8, game_setup: '1v1', visibility: 'public' },
    });
  });
});

describe('Table configuration — one surface creates and edits (#546)', () => {
  /** A host's view of a named, private 4-seat table. */
  const NAMED_ROOM_JSON = JSON.stringify({
    session: 's:ab12',
    you: 'p1',
    room: {
      room_id: 'r:7f3',
      config: {
        seats: 4,
        game_setup: 'commander',
        name: 'Casual Commander',
        visibility: 'private',
      },
      seats: [{ seat: 0, occupied_by: 'p1' }, { seat: 1 }, { seat: 2 }, { seat: 3 }],
    },
    valid_commands: ['submit_deck', 'update_room', 'add_ai', 'leave'],
  });

  it('sends the name and visibility the host chose when creating a table', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('open-create-game-button'));

    fireEvent.change(screen.getByTestId('create-table-name'), {
      target: { value: '  Casual Commander  ' },
    });
    fireEvent.change(screen.getByTestId('create-visibility'), { target: { value: 'private' } });
    fireEvent.click(screen.getByTestId('create-room-button'));

    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'create_room',
      // Trimmed here as well as server-side, so what was typed and what the table is
      // called cannot drift over a stray space.
      config: { seats: 2, game_setup: '1v1', name: 'Casual Commander', visibility: 'private' },
    });
  });

  it('leaves an unnamed table unnamed rather than sending the format label as prose', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('open-create-game-button'));
    // The format label is the *placeholder*: what the table will be called if the host
    // names nothing. It is never sent, because the server invents no names either.
    expect(screen.getByTestId('create-table-name').getAttribute('placeholder')).toBe('1v1 Duel');
    fireEvent.change(screen.getByTestId('create-table-name'), { target: { value: '   ' } });
    fireEvent.click(screen.getByTestId('create-room-button'));
    const sent = JSON.parse(socket.sent[socket.sent.length - 1]!) as {
      config: { name?: string };
    };
    expect(sent.config.name).toBeUndefined();
  });

  it('shows the table by its name, with format, size, and visibility under it', () => {
    mountLobby(NAMED_ROOM_JSON);
    expect(screen.getByTestId('room-plaque').textContent).toContain('Casual Commander');
    const status = screen.getByTestId('room-status').textContent!;
    expect(status).toContain('Commander');
    expect(status).toContain('4 seats');
    // Visibility is a word, never a hue or an icon alone.
    expect(status).toContain('Private');
  });

  it('falls back to the format label for an unnamed table', () => {
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(screen.getByTestId('room-plaque').textContent).toContain('1v1 Duel');
    expect(screen.getByTestId('room-status').textContent).toContain('Public');
  });

  it('offers Edit Table only when the server advertises update_room', () => {
    // The host's own view advertises it…
    mountLobby(NAMED_ROOM_JSON);
    expect(screen.getByTestId('edit-table-button')).toBeDefined();
    cleanup();
    act(() => useGameStore.getState().disconnect());

    // …and a seat that is not the host is simply never offered it. The client never
    // decides host-ness for itself.
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(screen.queryByTestId('edit-table-button')).toBeNull();
  });

  it('reopens the create surface to edit, seeded from the table, and sends update_room', () => {
    const socket = mountLobby(NAMED_ROOM_JSON);
    fireEvent.click(screen.getByTestId('edit-table-button'));

    // The SAME surface, not a second edit interface: same plaque, same four fields.
    expect(screen.getByTestId('create-room').textContent).toContain('Edit Table');
    expect(screen.queryByTestId('seat-ring')).toBeNull();
    expect((screen.getByTestId('create-table-name') as HTMLInputElement).value).toBe(
      'Casual Commander',
    );
    expect((screen.getByTestId('game-setup-select') as HTMLSelectElement).value).toBe('commander');
    expect(screen.getByTestId('seat-count').textContent).toBe('4');
    expect((screen.getByTestId('create-visibility') as HTMLSelectElement).value).toBe('private');
    // Still exactly one blue action.
    expect(primaries()).toHaveLength(1);

    fireEvent.click(screen.getByTestId('seat-count-decrease'));
    fireEvent.change(screen.getByTestId('create-visibility'), { target: { value: 'public' } });
    fireEvent.click(screen.getByTestId('create-room-button'));

    // A whole config, exactly as `update_room` carries it — never a patch.
    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'update_room',
      config: {
        seats: 3,
        game_setup: 'commander',
        name: 'Casual Commander',
        visibility: 'public',
      },
    });
    // The arena comes back: whether the change took is the next view's to say.
    expect(screen.getByTestId('seat-ring')).toBeDefined();
  });

  it('abandons an edit without sending anything', () => {
    const socket = mountLobby(NAMED_ROOM_JSON);
    const before = socket.sent.length;
    fireEvent.click(screen.getByTestId('edit-table-button'));
    fireEvent.change(screen.getByTestId('create-table-name'), { target: { value: 'Nope' } });
    fireEvent.click(screen.getByTestId('create-room-cancel'));
    expect(socket.sent.length).toBe(before);
    expect(screen.getByTestId('seat-ring')).toBeDefined();
    expect(screen.getByTestId('room-plaque').textContent).toContain('Casual Commander');
  });

  it('names the table in the open-games list, so a name is not the host’s alone', () => {
    mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        directory: [
          {
            room_id: 'r0',
            config: { seats: 4, game_setup: 'commander', name: 'Casual Commander' },
            filled: 1,
            state: 'gathering',
          },
        ],
        valid_commands: ['create_room', 'join_room'],
      }),
    );
    const row = screen.getByTestId('room-row-r0');
    expect(row.textContent).toContain('Casual Commander');
    // The format still reads, on the row's second line.
    expect(row.textContent).toContain('Commander');
  });
});

describe('Ready room — the arena is the seating diagram (#546)', () => {
  it('places every seat on the ring with the local seat at the bottom', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    expect(screen.getByTestId('seat-ring')).toBeDefined();
    // Seat 0 is the local one, so it is anchored at bottom centre and seat 1
    // sits opposite it — the room reads as a table, not as a list.
    const local = screen.getByTestId('seat-0').parentElement!;
    const other = screen.getByTestId('seat-1').parentElement!;
    expect(local.style.getPropertyValue('--seat-x')).toBe('50%');
    expect(local.style.getPropertyValue('--seat-y')).toBe('84%');
    expect(other.style.getPropertyValue('--seat-y')).toBe('16%');
  });

  it('wears each seat’s SCENE_SEAT_ACCENTS accent and its non-colour channels', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    for (const seat of [0, 1]) {
      expect(screen.getByTestId(`seat-${seat}`).style.getPropertyValue('--pregame-accent')).toBe(
        SCENE_SEAT_ACCENTS[seat],
      );
    }
    const local = screen.getByTestId('seat-0');
    const crest = screen.getByTestId('seat-0-crest');
    expect(local.contains(crest)).toBe(true);
    // The face a seat wears here is the SAME plate it wears in the match, so a
    // player learns one identity and sits down behind it (`seatPortraits.ts`).
    expect(crest.querySelector('img')?.getAttribute('src')).toBe(LOCAL_PORTRAIT?.src);
    expect(screen.getByTestId('seat-1-crest').querySelector('img')?.getAttribute('src')).toBe(
      OPPONENT_PORTRAITS[1]?.src,
    );
    // Identity is never the plate alone: the name and the You tag are beside it.
    expect(local.textContent).toContain('You');
    expect(local.textContent).toContain('Player 1');
    // Readiness is a word, never a hue alone.
    expect(screen.getByTestId('seat-0-ready').textContent).toBe('Ready');
  });

  it('leaves an open seat dashed, unaccented, and carrying its own options', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    const open = screen.getByTestId('seat-1');
    expect(open.style.getPropertyValue('--pregame-accent')).toBe('');
    expect(open.textContent).toContain('Open seat');
    // Contextual: nothing about inviting or seating an AI exists until asked.
    expect(screen.queryByTestId('seat-options')).toBeNull();
    fireEvent.click(screen.getByTestId('seat-1-options-button'));
    expect(screen.getByTestId('seat-options')).toBeDefined();
    expect(screen.getByTestId('room-id').textContent).toBe('r:7f3');
  });

  it('expands only the local seat, for one deck dropdown and no deck library', () => {
    // Both seats occupied, and `submit_deck` advertised: the only thing that may
    // make a seat expand is that seat being MINE. Another seat's deck is not the
    // local player's to choose — and its contents are redacted anyway.
    mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        room: {
          room_id: 'r:7f3',
          config: { seats: 2, game_setup: '1v1' },
          seats: [
            { seat: 0, occupied_by: 'p1' },
            { seat: 1, occupied_by: 'p2', name: 'Bob', decked: true },
          ],
        },
        valid_commands: ['submit_deck', 'leave'],
      }),
    );
    const local = screen.getByTestId('seat-0');
    expect(local.contains(screen.getByTestId('deck-select'))).toBe(true);
    // Exactly one dropdown on the whole ring, and never the library as a grid.
    expect(document.querySelectorAll('[data-testid="deck-select"]')).toHaveLength(1);
    expect(local.contains(screen.getByTestId('open-deck-builder-button'))).toBe(true);
    expect(local.contains(screen.getByTestId('import-deck-button'))).toBe(true);
    // The other seat states its deck in words instead.
    expect(screen.getByTestId('seat-1-deck').textContent).toBe('Deck submitted');
  });

  it('states the gate in words in the middle of the ring, from the current view', () => {
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(screen.getByTestId('ready-gate').textContent).toContain('Choose and submit a deck');
    cleanup();

    mountLobby(LOBBY_ROOM_DECKED_JSON);
    expect(screen.getByTestId('ready-gate').textContent).toContain('Waiting for 1 more player');
    cleanup();

    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    expect(screen.getByTestId('ready-gate').textContent).toContain('Starting the game');
  });

  it('keeps the shipped ready-waiting hook while you wait on other seats', () => {
    mountLobby(
      JSON.stringify({
        session: 's:1',
        you: 'p1',
        room: {
          room_id: 'r:1',
          config: { seats: 2, game_setup: '1v1' },
          seats: [
            { seat: 0, occupied_by: 'p1', decked: true, ready: true },
            { seat: 1, occupied_by: 'p2', name: 'Bob', decked: true },
          ],
        },
        valid_commands: ['unready', 'leave'],
      }),
    );
    expect(screen.getByTestId('ready-waiting').textContent).toContain("You're ready");
    expect(screen.getByTestId('ready-waiting').textContent).toContain('Bob');
    expect(screen.getByTestId('unready-button')).toBeDefined();
  });

  it('spends exactly one blue primary per state, under the local seat', () => {
    // Undecked: Submit deck is the one blue control.
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(primaries()).toHaveLength(1);
    expect(screen.getByTestId('submit-deck-button').getAttribute('data-variant')).toBe('primary');
    expect(screen.getByTestId('seat-0').parentElement!.contains(primaries()[0]!)).toBe(true);
    cleanup();

    // Decked: Ready takes the blue and Resubmit goes secondary — never two.
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    expect(primaries()).toHaveLength(1);
    expect(screen.getByTestId('ready-button').getAttribute('data-variant')).toBe('primary');
    expect(screen.getByTestId('submit-deck-button').getAttribute('data-variant')).toBe('secondary');
    cleanup();

    // Everyone ready: the room is starting, so nothing is blue at all.
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    expect(primaries()).toHaveLength(0);
  });
});

describe('Lobby — the last-match ribbon', () => {
  it('renders the ribbon from ephemeral store state, outcome carried by the word', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    act(() =>
      useGameStore.getState().recordLastMatch({
        outcome: 'victory',
        opponents: ['Bob'],
        gameSetup: 'ffa-4',
        seats: 4,
      }),
    );

    const outcome = screen.getByTestId('last-match-outcome');
    // The WORD carries the meaning; the hue family only tints it.
    expect(outcome.textContent).toBe('Victory');
    expect(outcome.getAttribute('data-outcome')).toBe('victory');
    expect(screen.getByTestId('last-match-ribbon').textContent).toContain('Bob');
    expect(screen.getByTestId('last-match-ribbon').textContent).toContain('Free-for-all');
  });

  it('Play again opens the create setup pre-filled, and dismisses the ribbon', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);
    act(() =>
      useGameStore.getState().recordLastMatch({
        outcome: 'defeat',
        opponents: ['Bob'],
        gameSetup: 'commander',
        seats: 4,
      }),
    );

    fireEvent.click(screen.getByTestId('last-match-play-again'));
    // Honestly a NEW table, seeded with the finished configuration.
    expect((screen.getByTestId('game-setup-select') as HTMLSelectElement).value).toBe('commander');
    expect(screen.getByTestId('seat-count').textContent).toBe('4');
    // And the ribbon is spent.
    expect(screen.queryByTestId('last-match-ribbon')).toBeNull();

    fireEvent.click(screen.getByTestId('create-room-button'));
    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'create_room',
      config: { seats: 4, game_setup: 'commander', visibility: 'public' },
    });
  });

  it('dismisses explicitly, and joining any room supersedes it', () => {
    const socket = mountLobby(LOBBY_DIRECTORY_JSON);
    act(() =>
      useGameStore
        .getState()
        .recordLastMatch({ outcome: 'draw', opponents: [], gameSetup: '1v1', seats: 2 }),
    );
    fireEvent.click(screen.getByTestId('last-match-dismiss'));
    expect(screen.queryByTestId('last-match-ribbon')).toBeNull();

    act(() =>
      useGameStore
        .getState()
        .recordLastMatch({ outcome: 'draw', opponents: [], gameSetup: '1v1', seats: 2 }),
    );
    expect(screen.getByTestId('last-match-ribbon')).toBeDefined();
    fireEvent.click(screen.getByTestId('room-row-r0'));
    fireEvent.click(screen.getByTestId('join-selected-button'));
    expect(useGameStore.getState().lastMatch).toBeNull();
    expect(socket.sent.length).toBeGreaterThan(0);
  });

  it('lands on the ribbon after #452’s postgame exit, across the reconnect', () => {
    // The whole path, through the real store: a room, a finished game, the
    // game-over exit (which closes the bridged socket and reopens the server),
    // and the lobby landing on the far side of it. Nothing but the ribbon
    // crosses — the transition assumes no same-session hand-off.
    const sockets: FakeSocket[] = [];
    const factory: SocketFactory = () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s as unknown as WebSocket;
    };
    act(() =>
      useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
    );
    act(() => sockets[0]!.emitOpen());
    act(() => sockets[0]!.emitMessage(LOBBY_ROOM_DECKED_JSON));
    act(() =>
      sockets[0]!.emitMessage(
        JSON.stringify({
          you: 'p1',
          opponents: [{ player_id: 'p2', hand_size: 0, life: 0, library_size: 40 }],
          player_names: { p1: 'Alice', p2: 'Bob' },
          seat_order: ['p1', 'p2'],
          phase: 'end',
          valid_actions: [],
          result: { winner: 'p1', losers: ['p2'], reason: 'decked' },
        }),
      ),
    );

    // #452's exit, then the reopened session's first LobbyView.
    act(() => useGameStore.getState().leaveGame());
    act(() => sockets[1]!.emitOpen());
    act(() => sockets[1]!.emitMessage(LOBBY_DIRECTORY_JSON));
    render(<LobbyScreen />);

    expect(screen.getByTestId('lobby-screen')).toBeDefined();
    expect(screen.getByTestId('last-match-outcome').textContent).toBe('Victory');
    expect(screen.getByTestId('last-match-ribbon').textContent).toContain('Bob');
    // Play again is pre-filled from the finished room, which only the LobbyView
    // knew — proof the record was read before the teardown.
    fireEvent.click(screen.getByTestId('last-match-play-again'));
    expect((screen.getByTestId('game-setup-select') as HTMLSelectElement).value).toBe('1v1');
    expect(screen.getByTestId('seat-count').textContent).toBe('2');
  });

  it('renders the lobby identically and fully functional with the ribbon absent', () => {
    // The ribbon may never be the only place a piece of information exists, and
    // no control may depend on it.
    mountLobby(LOBBY_DIRECTORY_JSON);
    expect(screen.queryByTestId('last-match-ribbon')).toBeNull();
    const withoutRibbon = screen.getByTestId('room-directory').outerHTML;
    const primariesWithout = primaries().length;

    act(() =>
      useGameStore
        .getState()
        .recordLastMatch({ outcome: 'victory', opponents: ['Bob'], gameSetup: '1v1', seats: 2 }),
    );
    expect(screen.getByTestId('room-directory').outerHTML).toBe(withoutRibbon);
    expect(primaries()).toHaveLength(primariesWithout);
    expect(screen.getByTestId('room-row-r0')).toBeDefined();
  });
});

describe('Pregame controls — the shared control family, at the touch floor', () => {
  it('draws every pregame control from `table/controls`, never a second family', () => {
    // jsdom has no layout, so the floor is enforced where it is authored: every
    // control below is a `ControlButton`/`IconButton`, whose `.button` base is
    // floored at `--rune-control-hit` (44 px). The geometry itself is the
    // maintainer's browser check.
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    for (const testId of [
      'ready-button',
      'submit-deck-button',
      'leave-room-button',
      'open-deck-builder-button',
      'import-deck-button',
      'open-decks-button',
      'session-menu-button',
      'chat-toggle',
      'seat-1-options-button',
    ]) {
      const control = screen.getByTestId(testId);
      expect(control.tagName).toBe('BUTTON');
      // The family's own marker: a control drawn anywhere else would not have it.
      expect(control.querySelector('span')).not.toBeNull();
    }
  });
});

/**
 * `submit_deck` is a seated command. From the server lobby the player has no
 * seat, so the server answers `NotSeated` and leaves the command out of
 * `valid_commands` — the builder opened there is a deck LIBRARY, and offering
 * Submit sent a command the server had never advertised.
 */
describe('Deck builder — submission follows valid_commands (#546)', () => {
  /** Every `submit_deck` frame this socket has carried. */
  function submissions(socket: FakeSocket): string[] {
    return socket.sent.filter((frame) => frame.includes('submit_deck'));
  }

  it('offers no submission from the roomless lobby, and sends none', () => {
    const socket = mountLobby(LOBBY_ROOMLESS_JSON);

    fireEvent.click(screen.getByTestId('open-deck-builder-button'));
    expect(screen.getByTestId('deck-builder-cancel')).toBeDefined();
    expect(screen.queryByTestId('deck-builder-submit')).toBeNull();
    // The library still closes, and no deck ever reached the wire. (Opening it
    // may legitimately request the catalog; that is not a submission.)
    fireEvent.click(screen.getByTestId('deck-builder-cancel'));
    expect(submissions(socket)).toEqual([]);
  });

  it('offers submission in a room, where the seat may still submit', () => {
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);

    fireEvent.click(screen.getByTestId('open-deck-builder-button'));
    expect(screen.getByTestId('deck-builder-submit')).toBeDefined();
  });
});

/**
 * The room's deck dropdown is a read of the device-local store (ADR 0027), and
 * it used to be read exactly once. Saving or importing a deck inside the builder
 * therefore left the dropdown behind until `RoomPlace` remounted: a player could
 * save a deck and then not find it in the seat they were sitting in.
 */
describe('Ready room — the deck choices follow the device store (#546)', () => {
  afterEach(() => resetSavedDeckStore());

  it('offers a deck saved after the room was already on screen', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 1 });
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);

    expect(screen.queryByTestId('deck-option-saved:Ember Rites')).toBeNull();

    await act(async () => {
      await saveDeck({ name: 'Ember Rites', cards: countsToCards({ shock: 4 }) });
    });

    // No remount: the same dropdown now carries the row.
    expect(await screen.findByTestId('deck-option-saved:Ember Rites')).toBeDefined();
  });

  it('drops a deck deleted after the room was already on screen', async () => {
    configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 1 });
    await saveDeck({ name: 'Ember Rites', cards: countsToCards({ shock: 4 }) });
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(await screen.findByTestId('deck-option-saved:Ember Rites')).toBeDefined();

    await act(async () => {
      await deleteSavedDeck('Ember Rites');
    });

    await waitFor(() => expect(screen.queryByTestId('deck-option-saved:Ember Rites')).toBeNull());
  });
});
