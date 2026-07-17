# ADR 0022: How spectators receive game views — a distinct `SpectatorView`

- Status: accepted
- Date: 2026-07-17
- Issue: #343

## Context

M5 ("More than two") requires **spectators**: non-seated observers who watch a
live game with all hidden information fully redacted. Nothing in the protocol has
an observer concept today, and every layer assumes a seated receiver:

- `personalized_view` requires a seat — it takes a `viewer: PlayerId` and indexes
  `state.players.get(viewer.0)` to build `my_hand` (`crates/rune-server/src/view.rs`).
  There is no way to project a view for a connection that owns no seat.
- `GameView` is defined *around its receiver*: `you` (the receiver's `p{N}` id),
  `my_hand`, `me: SelfView` (the receiver's own life/library), `mana_pool`, and
  `valid_actions` all describe one seated player (`rune-protocol`
  `pub struct GameView`). Those fields are the contract's core guarantee to seated
  clients — a seat always has a `you`, a `me`, and a `valid_actions` list that is
  the *only* source of interactivity.
- Rooms track seats only; the lobby (ADR 0012) has no join-as-observer path, and
  its deferral list explicitly parked spectators here: "no spectator join path or
  spectator `LobbyView`/`GameView` variant (roadmap M5 builds spectating on the
  same full-redaction machinery later)."

The redaction machinery ADR 0012 promised already exists and is per-viewer: the
game log is redacted per receiver (ADR 0021), `personalized_view` shares public
battlefield/stack/zone state verbatim and reduces every non-receiver seat to an
`OpponentView` of public counts. A spectator is, informationally, a receiver
entitled to *only* the public intersection — every seat is an opponent, none is
"me".

Before the implementation issue (#351) touches server, protocol, and client, the
spectator **view model** must be settled: does a spectator receive a `GameView`
with an absent seat, a distinct `SpectatorView` type, or a server-composed public
subset of `GameView`? The choice shapes #351's redaction-safety story, what happens
to eliminated players (#342), and what the lobby contract (ADR 0012) adds for
observers. It also gates the M5 exit criterion — "a 4-player FFA plays to a single
winner **with a spectator watching**" (#350 covers the FFA half; #351 the spectator
half).

## Decision

**A spectator receives a distinct `SpectatorView` type that shares `GameView`'s
public component types but carries no receiver fields — Option 2.** Redaction is
**structural**: the type simply has no hidden-zone or decision fields, so there is
nothing a projection could forget to redact.

Concretely, the rules the follow-up (#351) implements:

### `SpectatorView` (server → spectator connection)

- Reuses the existing public component types verbatim — `Permanent`, `StackItem`,
  `ZonePile`, `GameLogEntry`, `Phase`, `PlayerId`, the terminal `GameResult` — so a
  spectator gets every future public-view feature for free the moment it is added to
  those shared types.
- Carries the **public intersection only**: the battlefield, the stack, every
  player's public zone piles (graveyard, exile) and public per-seat stats
  (life, hand *size*, library *size* — the `OpponentView`-shaped facts), the phase,
  turn number, active player, seat order, the **public** game log window, and the
  terminal result.
- **Has no `you`, no `me`, no `my_hand`, no `mana_pool`, no `valid_actions`, no
  `action_deadline`, no per-seat prompt** — the fields do not exist on the type, so
  the compiler, not a reviewer, guarantees a spectator can never receive hidden-zone
  contents or a decision surface. Every seat appears as the public `OpponentView`
  shape; there is no privileged "self".

### How a spectator joins (extends ADR 0012, settled in #351)

- Join is a **lobby command** (`JoinRoom` gains an observer variant, or a sibling
  `SpectateRoom` — the exact spelling is #351's to fix in `docs/protocol.md` +
  `rune-protocol` + the TS mirror together, per the contract-discipline hard rule).
- **Room capacity for spectators is separate from seat capacity.** Seats stay
  `2..=8` (ADR 0012); a spectator does not consume a seat and may join a room whose
  seats are full, including **mid-game** — the complete-view principle means a
  spectator reconstructs the whole public board from its first `SpectatorView` with
  no history.
- `RoomSummary` gains a spectator **count** so the room directory can advertise
  observers. Spectators are visible to players **as a count only**; spectator
  identities/names are **not** surfaced to seated players (no social layer in M5).

### Eliminated players (#342) and spectating

An eliminated player (#342) **keeps their seated connection and their `GameView`**;
they do not silently convert into a `SpectatorView`. Their `valid_actions` simply
stay empty for the rest of the game (they receive no more decisions), and they
continue to see their now-public former hand/library through the normal seated
projection — a player who has lost has no hidden information left to protect, and
keeping them on the seated path avoids a live connection swapping its message type
mid-game. A distinct spectator is always a **non-seated** observer. (Whether an
eliminated player may *additionally* open a spectator connection is not special —
it is just the ordinary join path.)

### Live vs. delayed views — deferred

Spectator views are **live** (same freshness as seated views). Optional delayed
spectator views for anti-collusion in open rooms are **explicitly deferred** beyond
M5; the decision is recorded here so #351 does not build for it. If a later
milestone wants delay, it layers on top of the live `SpectatorView` (a server-side
buffer), not a new type.

## Consequences

- **Redaction is safe by construction, not by vigilance.** Because `SpectatorView`
  has no field capable of holding a hand, a library, a mana pool, or a
  `valid_actions` list, a projection bug cannot leak hidden information to a
  spectator — the worst case is a *missing* public fact, never a *leaked* private
  one. #351's redaction proof becomes "the type has no hidden fields" plus a test
  that the projected public facts equal the intersection of all seated players'
  public information, rather than an audit of every optional field on a shared type.
  This directly answers #351's core risk ("leaks … are privacy/contract violations,
  so the projection must be safe by construction … proven by tests, not review").
- **Seated `GameView` guarantees stay intact.** `you`, `me`, and `valid_actions`
  remain non-optional in spirit for every seated consumer — we do **not** weaken
  them to `Option` for all clients (the cost of Option 1). Existing seated clients,
  tests, and the two-player wire traffic are untouched.
- **Cost: a parallel type and a second client entry path.** `rune-protocol` gains
  `SpectatorView`, the TypeScript mirror gains its shape and normalization defaults,
  and the client gains a **spectate mode** (no hand row, no action tray, read-only
  inspection + zone browsers + log + game-over verdict). This is real duplication
  versus Option 3's "no new type," but it is duplication of *shape*, not of
  *rendering*: the shared component types mean the board/stack/log renderers are
  reused; only the shell (which chrome to show) branches on view type.
- **Rejected — Option 1 (reuse `GameView` with an absent seat).** Making `you`
  optional, `my_hand` empty, `me` omitted, and `valid_actions` always empty for
  spectators weakens the receiver guarantees for *every* seated consumer: every
  client and test that relies on "a view has a `me`" must now handle the
  seatless case, and redaction becomes "remember to blank five fields correctly"
  — exactly the by-vigilance failure mode this ADR exists to avoid.
- **Rejected — Option 3 (server-composed public subset of `GameView`, no new
  type).** Sending a `GameView` populated as if for a virtual seat that is owed
  nothing hidden keeps the wire type stable but makes redaction *by construction of
  a value* (easy to leak through — one forgotten field and a hidden fact ships) and
  still forces the client to special-case a receiver that is not a player. It trades
  a compile-time guarantee for a runtime convention, which the redaction risk does
  not justify.
- **Extends ADR 0012 rather than reopening it.** The lobby's deferred "spectators"
  bullet is now decided: observers join via a lobby command, do not consume seats,
  may join mid-game, and are advertised as a count. #351 lands the concrete wire
  shapes (contract change: `docs/protocol.md` + `rune-protocol` + TS mirror in one
  PR), the redacted projection with tests, and the client spectate mode.
- **Ships no code and no `docs/protocol.md` edit.** Like ADR 0012, this ADR fixes
  the design only. The `SpectatorView` type, the lobby join path, the projection,
  and the client mode all land in #351; until then the protocol has no observer
  concept and this ADR is the record of the chosen shape.
