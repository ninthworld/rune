# ADR 0012: Lobby protocol — identity, rooms, ready gate, deck submission

- Status: accepted
- Date: 2026-07-11
- Issue: #105

## Context

RUNE has an in-game protocol and no way to *reach* it. `docs/protocol.md` opens by
declaring "the entire client/server contract is two message types over WebSocket"
(`GameView` out, `ChooseAction` in), and that contract is real and complete for a
game already in progress. But there is no protocol for the steps *before* a game
exists: getting an identity, creating or joining a room with a configuration,
submitting a deck, declaring readiness, and — after a refresh or a dropped socket —
returning to the seat you already held. The server today papers over all of this
with a hardcoded auto-seating path: a WebSocket upgrade is silently dropped into
the first open seat of a hardcoded two-seat room whose game is already "live" with
empty decks (`crates/rune-server/src/lobby.rs`, `docs/roadmap.md` baseline). There
is no create/join, no config, no ready gate, no deck, and no identity.

The roadmap makes closing this gap the whole of M1 ("Take a seat"): two people
launch the server, enter an address, create a room with a configuration, share the
room id, have a second player join, both submit decks and ready up, and the game
begins — and a refreshed browser reconnects to its seat. M1's exit criteria name
this ADR explicitly ("ADRs accepted for: … lobby protocol …") and require that
"`docs/protocol.md` documents the lobby contract; `rune-protocol` round-trips it;
the 'entire API is two messages' framing is amended." This decision settles the
*shape* of that contract before the implementation issues (#108 protocol types,
#110 rooms, #112 gate, #113 reconnect, #114/#115 clients) land on top of an
undecided design.

Two existing forces constrain the design tightly:

- **The GameView philosophy is load-bearing and must extend to the lobby.** The
  in-game contract sends the *full* personalized state every message, and the client
  reconstructs its entire UI from one `GameView` + pending prompt — "the client is
  stateless with respect to rules: a fresh GameView must fully reconstruct the UI
  (reconnect, spectate, resync all depend on this)" (`docs/protocol.md` Invariants;
  `AGENTS.md` hard rules). The pre-game screens (room list, seat roster, who is
  decked, who is ready) are exactly the kind of state that must survive a reconnect,
  so the lobby half of the protocol must obey the same "full-state-every-time, UI
  reconstructable from one message, zero client logic" discipline rather than
  inventing a stateful handshake.

- **The lobby has no identity, and that is a known, filed defect.** `lobby.rs`
  states it directly: the lobby "has **no identity binding**, so it cannot tell a
  returning player from a stranger; it must therefore treat every new connection as
  a stranger." That is why issue #48's fix retires a vacated seat one-way
  (`Open → Taken → Retired`, never reopened): with no way to prove a reconnecting
  socket is the same player, reissuing the seat would hand the departed player's
  private `my_hand` to whoever connected next. The module docs name the unblock
  precisely — "True reconnection … is consequently blocked on an identity /
  reconnect-token mechanism (a future milestone)." This ADR is that mechanism's
  decision. `SEATS_PER_ROOM = 2` is likewise a placeholder the roadmap wants
  retired: lobby/room plumbing must support 2–8 seats even while the engine stays
  two-player.

Adjacent to both: the game a room eventually constructs needs a *game-setup*
identifier (players, starting life, hand size — the `GameSetup` the engine gains in
#109). What setups exist and how they are defined is the concern of **ADR 0013
(forthcoming, card-identity/setup direction, #106)**; this ADR only decides that
room config *carries* such an identifier, not what its values are.

## Decision

RUNE gains a small **lobby message set** that flanks the two in-game messages. It
mirrors the GameView philosophy exactly — full lobby state pushed every time, the
client reconstructs its pre-game UI from one message and computes no legality — and
it hands off to the existing `GameView`/`ChooseAction` contract at the moment a game
is constructed. The rules below are what `rune-protocol`, `rune-server`, and the
clients will follow; the concrete wire types and the `docs/protocol.md` edits land
in the follow-up issues (below), not here.

### Message envelope: `LobbyView` (server→client) and `LobbyCommand` (client→server)

The pre-game contract is two messages, structurally parallel to the in-game two:

- **`LobbyView` (server → client)** is the pre-game analogue of `GameView`: the
  **full** current lobby state for this connection, pushed on every change, from
  which the client rebuilds its entire pre-game UI. It carries at least: the
  connection's identity/session state; the room the connection is in (if any) with
  its config; the seat roster (per seat: occupied-by, decked yes/no, ready yes/no);
  and — as `valid_actions` is the *only* source of interactivity in `GameView` — the
  set of lobby commands currently legal for this connection (create, join, submit
  deck, ready, leave). The client renders exactly that and derives no legality of its
  own. Hidden information stays redacted server-side: a `LobbyView` never leaks
  another seat's decklist contents, only the fact that the seat is decked.

- **`LobbyCommand` (client → server)** is the pre-game analogue of `ChooseAction`:
  a single tagged message the client sends to act — `Hello`, `CreateRoom`,
  `JoinRoom`, `SubmitDeck`, `Ready` (and un-ready / `Leave`). The server validates
  every command against authoritative state and answers with a fresh `LobbyView`;
  an invalid command is rejected and the current `LobbyView` re-sent, exactly as an
  illegal `ChooseAction` re-sends the current `GameView`. No command relies on the
  client having remembered prior lobby state — a fresh `LobbyView` is always enough
  to continue.

Both messages are added to the same WebSocket connection framing already in use;
the wire format stays serde JSON of `rune-protocol` types. Unknown fields are
ignored for forward compatibility, matching the in-game invariant.

### Session identity: a server-issued opaque token on hello, echoed on reconnect

On first contact the client sends `Hello` (optionally carrying a previously issued
token). The server issues an **opaque session token** — a string the client treats
as an unparsed handle, exactly like an entity id or the ADR 0009 content-binding
token — and returns it in the resulting `LobbyView`. The client stores it and
**echoes it verbatim** on any later `Hello` (i.e. after a refresh or a dropped
socket). A connection presenting a token that matches a seat the server is holding
open is reunited with that seat and its in-progress game; a connection presenting
no token, or an unknown one, is a new stranger and is issued a fresh identity.

This is the identity binding `lobby.rs` says it lacks, and it is the mechanism the
#48 seat-retirement fix was explicitly waiting for. Once a returning connection can
*prove* it is the same player, the one-way `Retired` policy can admit a
token-authenticated rejoin to a held-open seat without the hidden-hand leak #48
guards against. The token is an **identity** handle only — not an account, not a
credential store, not authentication of a *human*; see Out of scope.

### Room config: `seats: 2..=8`, a `game_setup` id, join-by-room-id

A room is created explicitly by a `CreateRoom` command carrying a **room config**:

- **`seats: 2..=8`** — the number of seats, validated into that inclusive range.
  This retires the hardcoded `SEATS_PER_ROOM = 2` (`lobby.rs`). The *lobby and room
  plumbing* support 2–8 seats now even though the engine remains two-player for M1
  (`docs/roadmap.md`); a config the engine cannot yet construct a game for is a
  server-side validation concern, not a protocol-shape concern.
- **`game_setup`** — an opaque game-setup identifier carried in the config. It names
  which setup (players, starting life, hand size, …) the room will build its game
  from. **The catalogue of setups and how they are defined is owned by ADR 0013
  (forthcoming); this ADR only fixes that the config carries the id.** The server
  validates the id against whatever ADR 0013 / the engine make available.
- **Join by room id.** `CreateRoom` yields a **room id** (an opaque string the
  creator shares out-of-band); a second player joins with `JoinRoom { room_id }`.
  There is deliberately no auto-seating, no matchmaking, and no room discovery —
  you either create a room or join one whose id you were given.

### Deck submission: a server-validated decklist of card identities

Before readying, each seated connection submits a deck with `SubmitDeck`, carrying
a **decklist** expressed as **card identities** (opaque card-identity handles, whose
identity-vs-printing model is ADR 0013's concern — not printings, not images; the
project's legal rules forbid card images and WotC branding, `docs/brief.md`). The
server **validates the decklist authoritatively** against the card database
(`CardDatabase`, already owned by the lobby) — every identity must exist; format /
size rules are enforced server-side — and reflects only *decked: yes/no* per seat in
other players' `LobbyView`s, never the contents. An invalid decklist is rejected
with the current `LobbyView` re-sent; the client computes no deck legality, matching
the "zero game logic in the client" rule.

### Ready gate: the game is constructed only when all seats are filled + decked + ready

A seat may send `Ready` only once it is occupied and has a validated deck. **The
room constructs its `GameState` and transitions from lobby to game exactly when
every seat is simultaneously filled, decked, and ready** — not before. Until that
gate is met the room stays in the lobby phase, pushing `LobbyView`s; the instant it
is met the server builds the game (seeded shuffle, opening hands — engine work in
#109/#111) and the connections begin receiving `GameView`s. A player un-readying, a
deck being resubmitted, or a seat emptying before the gate is met keeps the room in
the lobby phase. There is no auto-start and no game with empty decks — the current
"live with one player and empty decks" behavior is removed.

### The amended "two message types" framing

`docs/protocol.md` currently opens: "The entire client/server contract is two
message types over WebSocket." That framing is **amended, not discarded**: the two
message types are the two *in-game* messages (`GameView` / `ChooseAction`), and they
are now **flanked by a small lobby message set** (`LobbyView` / `LobbyCommand`) that
governs the pre-game phase and **hands off to the in-game contract once the game is
constructed**. The precise new wording is a `docs/protocol.md` edit that lands with
the protocol types (#108), since amending `docs/protocol.md` is a contract change
(`AGENTS.md`) and this ADR only *decides* the shape. The intended framing after that
edit: *the connection speaks a small lobby protocol (full-lobby-state-every-message,
same philosophy as GameView) until a game is constructed, at which point it speaks
exactly the two in-game message types for the life of that game.*

### Out of scope / deferred

This ADR deliberately decides only the minimum pre-game protocol M1 needs. The
following are **explicitly out of scope** and deferred to later milestones:

- **Matchmaking / queues** — no automatic pairing, ranking, or "find me a game."
  Rooms are created and joined by id only (M5+ concerns).
- **Auth / accounts** — the session token is an opaque *identity/reconnect* handle,
  not authentication of a human, not persistent accounts, not login.
- **Chat** — no lobby or in-game chat messages.
- **Spectators** — no spectator join path or spectator `LobbyView`/`GameView`
  variant (roadmap M5 builds spectating on the same full-redaction machinery later).

## Consequences

- **Easier.** The `LobbyView`/`LobbyCommand` pair reuses the exact discipline the
  in-game contract already proved — full state every message, UI reconstructable from
  one message, zero client legality — so the pre-game UI (#114/#115) is stateless and
  reconnect-safe by construction, and the connection screen (#103) has a defined
  next message to send. Rooms become explicit and configurable (2–8 seats,
  `game_setup` id, join-by-id), replacing the hardcoded auto-seat path. The ready gate
  gives a single, unambiguous moment of game construction, removing the "live with
  empty decks" placeholder.
- **Unblocks #48 seat retirement.** The server-issued session token is precisely the
  identity mechanism `lobby.rs` says it lacks and that the #48 one-way `Retired`
  policy was waiting for. With a token-authenticated rejoin, a returning connection
  can prove it owns a held-open seat, so reconnection can be allowed **without**
  reopening the hidden-hand leak #48 closes — the seat is handed back to the proven
  original player, never to a stranger.
- **Harder / given up.** The connection is no longer "one socket = one game": it now
  has a pre-game lifecycle (identity → room → deck → ready → game) and a phase
  transition, and the server must hold and validate more state (tokens, room configs,
  per-seat deck/ready status) than the auto-seating stub did. Deck validation puts a
  new authoritative responsibility on the server against `CardDatabase`. The
  `GameView`-only mental model of the protocol is replaced by a two-phase one that
  every client (web, CLI, future agents) must implement.
- **Implementation lands via follow-up issues; protocol.md + rune-protocol move
  together.** This ADR ships **no code and no `docs/protocol.md` edit** — it only
  fixes the design. Because protocol changes are contract changes (`AGENTS.md`), the
  concrete message types must land in `rune-protocol` **and** `docs/protocol.md` in
  the *same* PR: the lobby message types + amended framing (#108), explicit rooms
  create/join-by-id (#110), the deck-submission + ready gate (#112), and
  token-based reconnect to a held seat (#113), with the lobby UI/CLI (#114/#115)
  built against them. Until #108 lands, `docs/protocol.md` still reads "two message
  types"; this ADR is the record of why and how that changes.
- **Depends on a forthcoming sibling decision.** The `game_setup` identifier and the
  card-identity vocabulary the decklist uses are defined by **ADR 0013 (#106,
  forthcoming)**; this ADR treats both as opaque values carried by the lobby
  protocol and does not fix their internal shape.
