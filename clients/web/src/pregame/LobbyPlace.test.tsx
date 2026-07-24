/**
 * The Lobby and Room compositions (issue #506; `front-door-and-lobby.md` §8
 * criteria 5, 7, 8, 9, 10, 11, 12, 13, 18, 21, 23, 24).
 *
 * The shipped `LobbyScreen.test.tsx` still owns every behavioral assertion and
 * passes unmigrated through the restyle; this suite covers what the new
 * composition adds — the gate in words, the pinned ready bar, the crest
 * treatment, the contained empty state, the matched share instructions, the
 * skeleton, the session menu, and the ephemeral last-match ribbon.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { LobbyScreen } from '../LobbyScreen';
import { useGameStore, type SocketFactory } from '../store';
import { SCENE_SEAT_ACCENTS } from '../sceneTokens';
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

describe('Lobby place — composition (criteria 11, 12, 13, 18)', () => {
  it('criterion 13: renders a skeleton directory plus a working Disconnect before the first view', () => {
    mountLobby();
    // The real composition, not a bare status sentence.
    expect(screen.getByTestId('lobby-waiting')).toBeDefined();
    expect(screen.getByTestId('room-directory')).toBeDefined();
    expect(screen.getByTestId('room-directory-skeleton')).toBeDefined();
    // Never a dead screen.
    expect(screen.getByTestId('lobby-disconnect-button')).toBeDefined();
  });

  it('criterion 11: the empty directory contains the create affordance', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    const empty = screen.getByTestId('room-directory-empty');
    // The action is INSIDE the empty state, not referred to elsewhere.
    expect(empty.contains(screen.getByTestId('room-directory-empty-create'))).toBe(true);

    // Pressing it opens the Start-a-game card in Create mode.
    fireEvent.click(screen.getByTestId('room-directory-empty-create'));
    expect(screen.getByTestId('start-mode-create').getAttribute('aria-pressed')).toBe('true');
  });

  it('criterion 12: the room-id chip and the join field carry matching instructions', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('start-mode-join'));
    const joinLabel = screen.getByTestId('join-room').textContent ?? '';
    cleanup();

    mountLobby(LOBBY_ROOM_DECKED_JSON);
    const shareLine = screen.getByTestId('room-share-line').textContent ?? '';

    // Both sides name the same object — an id you send and an id you paste.
    expect(shareLine).toContain('Send this id to a friend');
    expect(shareLine).toContain('paste it under Join');
    expect(joinLabel).toContain('paste the id a friend sent you');
  });

  it('criterion 18: a session menu holds display settings, card art, and Disconnect', () => {
    mountLobby(LOBBY_ROOMLESS_JSON);
    fireEvent.click(screen.getByTestId('session-menu-button'));
    expect(screen.getByTestId('session-menu-display-settings')).toBeDefined();
    expect(screen.getByTestId('session-menu-card-art')).toBeDefined();
    expect(screen.getByTestId('session-menu-disconnect')).toBeDefined();
  });

  it('criterion 18: the same menu is reachable inside a room', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    fireEvent.click(screen.getByTestId('session-menu-button'));
    expect(screen.getByTestId('session-menu-display-settings')).toBeDefined();
    expect(screen.getByTestId('session-menu-card-art')).toBeDefined();
  });

  it('shows occupancy as a shape as well as a number', () => {
    mountLobby(LOBBY_DIRECTORY_JSON);
    const pips = screen.getByTestId('room-r0-pips');
    // One pip per seat; the number stays the reading channel beside it.
    expect(pips.children).toHaveLength(2);
    expect(screen.getByTestId('room-r0-occupancy').textContent).toBe('1/2 filled');
  });
});

describe('Room place — identity (criteria 5, 7)', () => {
  it('criterion 5: a roster seat wears its SCENE_SEAT_ACCENTS accent', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    for (const seat of [0, 1]) {
      const row = screen.getByTestId(`seat-${seat}`);
      expect(row.style.getPropertyValue('--pregame-accent')).toBe(SCENE_SEAT_ACCENTS[seat]);
    }
  });

  it('criterion 7: the local row carries the crest treatment and the You tag', () => {
    mountLobby(LOBBY_ROOM_ALL_READY_JSON);
    const row = screen.getByTestId('seat-0');
    const crest = screen.getByTestId('seat-0-crest');
    expect(row.contains(crest)).toBe(true);
    // The monogram is the non-color channel beside the accent ring…
    expect(crest.textContent).toBe('P');
    expect(crest.style.getPropertyValue('--pregame-accent')).toBe(SCENE_SEAT_ACCENTS[0]);
    // …and identity is never color-only.
    expect(row.textContent).toContain('You');
    expect(row.textContent).toContain('Player 1');
  });

  it('leaves an open seat unaccented and dashed, with its own words', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    const open = screen.getByTestId('seat-1');
    expect(open.style.getPropertyValue('--pregame-accent')).toBe('');
    expect(open.textContent).toContain('Open seat');
  });
});

describe('Room place — the ready bar (criteria 8, 9, 10)', () => {
  it('criterion 8: the gold control lives in the pinned bar, not at the end of a scroll', () => {
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    const bar = screen.getByTestId('ready-bar');
    // Pinned (sticky) and OUTSIDE the scrolling body, so it is on screen at
    // every scroll position and every reference geometry.
    expect(bar.getAttribute('data-sticky')).toBe('true');
    expect(bar.contains(screen.getByTestId('ready-button'))).toBe(true);
    expect(screen.getByTestId('roster-panel').contains(bar)).toBe(false);
    expect(screen.getByTestId('deck-select-section').contains(bar)).toBe(false);
  });

  it('criterion 9: the bar states the gate in words, from the current view', () => {
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

  it('criterion 10: at most one gold affordance is on screen in each place', () => {
    // Room-less: the Start-a-game card's active mode holds the only gold.
    mountLobby(LOBBY_ROOMLESS_JSON);
    expect(document.querySelectorAll('[data-gold="true"]')).toHaveLength(1);
    fireEvent.click(screen.getByTestId('start-mode-join'));
    expect(document.querySelectorAll('[data-gold="true"]')).toHaveLength(1);
    cleanup();

    // In a room, undecked: Submit deck is the one gold.
    mountLobby(LOBBY_ROOM_UNDECKED_JSON);
    expect(document.querySelectorAll('[data-gold="true"]')).toHaveLength(1);
    expect(screen.getByTestId('submit-deck-button').getAttribute('data-gold')).toBe('true');
    cleanup();

    // Decked: Ready takes the gold and Resubmit goes quiet — never two.
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    expect(document.querySelectorAll('[data-gold="true"]')).toHaveLength(1);
    expect(screen.getByTestId('ready-button').getAttribute('data-gold')).toBe('true');
    expect(screen.getByTestId('submit-deck-button').getAttribute('data-gold')).toBeNull();
  });
});

describe('Lobby place — the last-match ribbon (criterion 24)', () => {
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

  it('Play again pre-fills the Start-a-game card and dismisses the ribbon', () => {
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
    // Honestly a NEW room, seeded with the finished configuration.
    expect(screen.getByTestId('start-mode-create').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('game-setup-commander').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('seat-count-4').getAttribute('aria-pressed')).toBe('true');
    // And the ribbon is spent.
    expect(screen.queryByTestId('last-match-ribbon')).toBeNull();

    fireEvent.click(screen.getByTestId('create-room-button'));
    expect(JSON.parse(socket.sent[socket.sent.length - 1]!)).toEqual({
      type: 'create_room',
      config: { seats: 4, game_setup: 'commander' },
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
    fireEvent.click(screen.getByTestId('join-directory-r0'));
    expect(useGameStore.getState().lastMatch).toBeNull();
    expect(socket.sent.length).toBeGreaterThan(0);
  });

  it('renders the lobby identically and fully functional with the ribbon absent (the reload case)', () => {
    // #506 ships the RENDERING ahead of #452/#509 producing the record, so the
    // lobby must be complete without it — the ribbon may never be the only
    // place a piece of information exists, and no control may depend on it.
    mountLobby(LOBBY_DIRECTORY_JSON);
    expect(screen.queryByTestId('last-match-ribbon')).toBeNull();
    const withoutRibbon = screen.getByTestId('room-directory').outerHTML;
    const goldWithout = document.querySelectorAll('[data-gold="true"]').length;

    act(() =>
      useGameStore
        .getState()
        .recordLastMatch({ outcome: 'victory', opponents: ['Bob'], gameSetup: '1v1', seats: 2 }),
    );
    // Everything below the ribbon is unchanged, and the one gold is still one.
    expect(screen.getByTestId('room-directory').outerHTML).toBe(withoutRibbon);
    expect(document.querySelectorAll('[data-gold="true"]')).toHaveLength(goldWithout);
    expect(screen.getByTestId('join-directory-r0')).toBeDefined();
  });
});

describe('Room place — every control clears the touch floor (criteria 21, 23)', () => {
  it('sizes every interactive pregame control from the shared 44 px token', () => {
    // jsdom has no layout, so the floor is enforced where it is authored: every
    // pregame control composes the `.button` base, whose min-height/min-width
    // are `var(--rune-touch)` (44 px) and which wraps rather than truncating.
    // This asserts the authored rule, with the geometry itself verified in the
    // browser at 1280×800, 1180×820, and 390×844 (see the PR's evidence).
    mountLobby(LOBBY_ROOM_DECKED_JSON);
    for (const testId of [
      'ready-button',
      'submit-deck-button',
      'leave-room-button',
      'copy-room-id-button',
      'open-deck-builder-button',
      'lobby-disconnect-button',
      'session-menu-button',
    ]) {
      expect(screen.getByTestId(testId).tagName).toBe('BUTTON');
    }
  });
});
