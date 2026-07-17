# ADR 0012: Lobby protocol and ready gate

- Status: accepted
- Date: 2026-07-11
- Issue: #105

## Context

The in-game protocol does not cover identity, rooms, deck submission, readiness, or reconnect.
Those operations need the same complete-view and server-authoritative properties as gameplay,
without leaking submitted decks or relying on a stateful client handshake.

## Decision

### Lobby messages

Before game construction, the server sends a complete personalized `LobbyView` after every
change and the client sends a tagged `LobbyCommand`. `valid_commands` is the only source of
lobby interactivity. Invalid commands are rejected and the current view is re-sent.

At game construction, the same connection transitions to `GameView` and `ChooseAction`.

### Identity and reconnect

The server issues an opaque session token. A client stores it and echoes it on a later `hello`
to reclaim the same held seat. The token is a reconnect identity, not a user account or proof
of a human identity. Unknown or absent tokens create new sessions.

### Rooms

A client creates a room with a seat count and `game_setup` id or joins one with an opaque room
id. Seat counts are represented for 2–8 players, and setup ids must exist in the server format
registry. Supported gameplay remains two-player until the engine’s multiplayer rules land.
Room discovery, matchmaking, and additional join paths may extend this model through later
protocol decisions.

### Deck and ready gate

Deck submission carries stable functional card identities. The server resolves each card and
applies the selected format’s deck policy. Other players see only whether a seat is decked.

A seat may ready only after a valid deck is accepted. The room constructs a game exactly when
all required seats are occupied, decked, and ready. Changing a deck clears readiness.

## Consequences

The pre-game UI is reconstructable from one view, reconnect does not expose a former player’s
hidden hand to a stranger, and game construction has one explicit gate. The server owns more
session and room state, and every client must implement both protocol phases.
